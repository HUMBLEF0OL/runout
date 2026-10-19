//! Listening logger (TSD §4.5, FR-026): SMTC snapshots → `accumulator` → `listening_event` rows via
//! the shared writer (`importer::events`), matched by `importer::matching`. Pause/off write nothing.
pub mod accumulator;
pub mod poll;
pub mod session;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use accumulator::{Accumulator, Closed};
use session::Snapshot;

use crate::error::{AppError, Result};
use crate::importer::events::{self, Event, Source};
use crate::importer::matching::Matcher;
use crate::library::items::{self, ItemSummary};
use crate::settings::Settings;

/// Longest timed pause `pause_logger` accepts (a week); `null` pauses until resumed.
pub const MAX_PAUSE_MIN: i64 = 7 * 24 * 60;

/// API.md §4 `NowPlaying`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NowPlaying {
    pub track_name: String,
    pub artist_name: String,
    pub album_name: String,
    pub spotify_uri: Option<String>,
    pub ms_played: i64,
    pub duration_ms: i64,
    pub item: Option<ItemSummary>,
}

/// API.md §4 `get_logger_status` shape (also the `logger-status` event).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoggerStatus {
    pub state: &'static str,
    pub now_playing: Option<NowPlaying>,
    pub last_event_at: Option<String>,
    pub paused_until: Option<String>,
    pub autostart: bool,
    pub include_private: bool,
}

/// API.md §4 `Play` (the `play-logged` event).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Play {
    pub id: i64,
    pub ended_at_local: String,
    pub ms_played: i64,
    pub counts_as_play: bool,
    pub skipped: bool,
    pub track_name: String,
    pub artist_name: String,
    pub album_name: String,
    pub source: String,
    pub spotify_uri: Option<String>,
    pub item: Option<ItemSummary>,
    pub album_rating: Option<f64>,
}

/// `set_logger` patch (API.md §4).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggerPatch {
    pub on: Option<bool>,
    pub autostart: Option<bool>,
    pub include_private: Option<bool>,
}

fn paused_until(s: &Settings) -> Option<DateTime<Utc>> {
    s.paused_until
        .as_deref()
        .and_then(|u| DateTime::parse_from_rfc3339(u).ok())
        .map(|u| u.with_timezone(&Utc))
}

/// Paused at `at`: until resumed, or a timed pause that hasn't run out.
pub fn paused(s: &Settings, at: DateTime<Utc>) -> bool {
    s.logger_paused && paused_until(s).is_none_or(|u| u > at)
}

/// Writing plays at `at`: on and not paused.
pub fn active(s: &Settings, at: DateTime<Utc>) -> bool {
    s.logger_on && !paused(s, at)
}

/// `pause_logger`: `minutes` = 1..=`MAX_PAUSE_MIN`, `None` = until resumed.
pub fn pause(s: &mut Settings, minutes: Option<i64>, at: DateTime<Utc>) -> Result<()> {
    s.paused_until = match minutes {
        None => None,
        Some(m @ 1..=MAX_PAUSE_MIN) => Some(iso(at + chrono::Duration::minutes(m))),
        Some(_) => {
            return Err(AppError::validation(
                "Pause for between 1 minute and 7 days.",
            ));
        }
    };
    s.logger_paused = true;
    Ok(())
}

pub fn resume(s: &mut Settings) {
    s.logger_paused = false;
    s.paused_until = None;
}

fn iso(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Live logger state, managed by Tauri as `Mutex<Logger>`. Lock order: logger → db → settings.
#[derive(Debug, Default)]
pub struct Logger {
    acc: Accumulator,
    /// SMTC unavailable (TSD §6).
    pub error: bool,
}

impl Logger {
    /// One snapshot at `at`. Paused or off: nothing accumulates and nothing is written. Returns the
    /// id of a newly written play.
    pub fn observe(
        &mut self,
        conn: &Connection,
        s: &Settings,
        snap: Snapshot,
        at: DateTime<Utc>,
    ) -> Result<Option<i64>> {
        self.error = false;
        if !active(s, at) {
            self.acc = Accumulator::default();
            return Ok(None);
        }
        write(conn, self.acc.observe(snap, at))
    }

    /// Finish the current play now (before pausing or turning off), writing it if ≥ 1 s.
    pub fn close(&mut self, conn: &Connection, at: DateTime<Utc>) -> Result<Option<i64>> {
        write(conn, self.acc.close(at))
    }

    pub fn status(
        &self,
        conn: &Connection,
        s: &Settings,
        at: DateTime<Utc>,
    ) -> Result<LoggerStatus> {
        let now_playing = match self.acc.track() {
            Some(t) if active(s, at) => Some(NowPlaying {
                track_name: t.title.clone(),
                artist_name: t.artist.clone(),
                album_name: t.album.clone(),
                spotify_uri: None, // SMTC has no URI (kb/spotify.md)
                ms_played: self.acc.ms_played(at),
                duration_ms: t.duration_ms,
                item: item_for(conn, &t.title, &t.artist)?,
            }),
            _ => None,
        };
        let state = if !s.logger_on {
            "off"
        } else if paused(s, at) {
            "paused"
        } else if self.error {
            "error"
        } else if now_playing.is_some() {
            "listening"
        } else {
            "idle"
        };
        let last_event_at = conn.query_row(
            "SELECT max(ended_at_utc) FROM listening_event WHERE source = 'logger'",
            [],
            |r| r.get(0),
        )?;
        Ok(LoggerStatus {
            state,
            now_playing,
            last_event_at,
            paused_until: s.paused_until.clone().filter(|_| paused(s, at)),
            autostart: s.autostart,
            include_private: s.include_private,
        })
    }
}

/// ponytail: loads the track index per call; one call per finished play or status read is cheap.
fn item_for(conn: &Connection, track: &str, artist: &str) -> Result<Option<ItemSummary>> {
    Matcher::load(conn)?
        .find(None, track, artist)
        .map(|id| items::summary(conn, id))
        .transpose()
}

fn write(conn: &Connection, c: Option<Closed>) -> Result<Option<i64>> {
    let Some(c) = c else { return Ok(None) };
    let item_id = Matcher::load(conn)?.find(None, &c.track.title, &c.track.artist);
    events::insert(
        conn,
        &Event {
            source: Source::Logger,
            ended_at_utc: iso(c.ended_at),
            ended_at_local: Some(
                c.ended_at
                    .with_timezone(&chrono::Local)
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
            ),
            ms_played: c.ms_played,
            track_name: c.track.title,
            artist_name: c.track.artist,
            album_name: c.track.album,
            spotify_track_uri: None,
            platform: None,
            reason_start: None,
            reason_end: None,
            shuffle: None,
            skipped: None,
            offline: None,
            private_session: false, // SMTC can't see private sessions (TSD §4.5)
            item_id,
            import_batch_id: None,
        },
    )
}

/// One `listening_event` as API.md `Play`. `album_rating` = the rating of the album the track item
/// is linked to (`on_album`).
pub fn play(conn: &Connection, id: i64) -> Result<Play> {
    let (mut p, item_id) = conn
        .query_row(
            "SELECT e.id, coalesce(e.ended_at_local, e.ended_at_utc), e.ms_played, e.counts_as_play,
                    coalesce(e.skipped, 0), coalesce(e.track_name, ''), coalesce(e.artist_name, ''),
                    coalesce(e.album_name, ''), e.source, e.spotify_track_uri, e.item_id,
                    (SELECT a.rating FROM item_link l JOIN item a ON a.id = l.to_id
                      WHERE l.from_id = e.item_id AND l.kind = 'on_album')
             FROM listening_event e WHERE e.id = ?1",
            [id],
            |r| {
                Ok((
                    Play {
                        id: r.get(0)?,
                        ended_at_local: r.get(1)?,
                        ms_played: r.get(2)?,
                        counts_as_play: r.get(3)?,
                        skipped: r.get(4)?,
                        track_name: r.get(5)?,
                        artist_name: r.get(6)?,
                        album_name: r.get(7)?,
                        source: r.get(8)?,
                        spotify_uri: r.get(9)?,
                        item: None,
                        album_rating: r.get(11)?,
                    },
                    r.get::<_, Option<i64>>(10)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That play no longer exists."))?;
    p.item = item_id.map(|i| items::summary(conn, i)).transpose()?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::accumulator::{PlayState, Track};
    use super::*;
    use crate::db;

    fn t(s: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_770_000_000 + s, 0).unwrap()
    }

    fn snap(name: &str, st: PlayState) -> Snapshot {
        Some((
            Track {
                title: name.into(),
                artist: "Fixture Artist".into(),
                album: "Fixture Album".into(),
                duration_ms: 180_000,
                position_ms: 0,
            },
            st,
        ))
    }

    fn settings() -> Settings {
        Settings::defaults(std::path::Path::new("d"), std::path::Path::new("d"))
    }

    fn rows(c: &Connection) -> Vec<(String, i64, bool, Option<i64>)> {
        let mut st = c
            .prepare("SELECT track_name, ms_played, counts_as_play, item_id FROM listening_event ORDER BY id")
            .unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn writes_plays_with_the_30_s_rule_and_matches_items() {
        let c = db::open_in_memory().unwrap();
        c.execute_batch(
            "INSERT INTO item(id, type, display_name, rating) VALUES (1, 'track', 'Short', NULL), (2, 'album', 'Fixture Album', 4.5);
             INSERT INTO item_link VALUES (1, 2, 'on_album');",
        )
        .unwrap();
        let (mut lg, s) = (Logger::default(), settings());
        use PlayState::*;
        lg.observe(&c, &s, snap("Short", Playing), t(0)).unwrap();
        // 29.9 s doesn't count, 30 s does (D25)
        let id = lg
            .close(&c, t(0) + chrono::Duration::milliseconds(29_900))
            .unwrap();
        lg.observe(&c, &s, snap("Long", Playing), t(100)).unwrap();
        let id2 = lg.observe(&c, &s, None, t(130)).unwrap();
        assert!(id.is_some() && id2.is_some());
        assert_eq!(
            rows(&c),
            vec![
                ("Short".into(), 29_900, false, Some(1)),
                ("Long".into(), 30_000, true, None),
            ]
        );
        let p = play(&c, id.unwrap()).unwrap();
        assert_eq!((p.source.as_str(), p.album_rating), ("logger", Some(4.5)));
        assert_eq!(p.item.unwrap().id, 1);
        let st = lg.status(&c, &s, t(131)).unwrap();
        assert_eq!(st.state, "idle");
        assert_eq!(st.last_event_at.as_deref(), Some("2026-02-02T02:42:10Z"));
    }

    #[test]
    fn pause_and_off_write_nothing() {
        let c = db::open_in_memory().unwrap();
        let (mut lg, mut s) = (Logger::default(), settings());
        use PlayState::*;
        // paused until resumed: a whole play passes, nothing is written
        pause(&mut s, None, t(0)).unwrap();
        for (at, sn) in [
            (0, snap("A", Playing)),
            (60, snap("B", Playing)),
            (120, None),
        ] {
            assert_eq!(lg.observe(&c, &s, sn, t(at)).unwrap(), None);
        }
        assert_eq!(lg.status(&c, &s, t(120)).unwrap().state, "paused");
        // resumed: counts from the first snapshot after resuming, not from before
        resume(&mut s);
        lg.observe(&c, &s, snap("C", Playing), t(200)).unwrap();
        assert_eq!(lg.status(&c, &s, t(210)).unwrap().state, "listening");
        // timed pause: during it nothing accrues; after it runs out logging resumes by itself
        lg.close(&c, t(240)).unwrap();
        pause(&mut s, Some(1), t(240)).unwrap();
        let st = lg.status(&c, &s, t(250)).unwrap();
        assert_eq!(
            (st.state, st.paused_until.as_deref()),
            ("paused", Some("2026-02-02T02:45:00Z"))
        );
        lg.observe(&c, &s, snap("D", Playing), t(250)).unwrap();
        lg.observe(&c, &s, snap("D", Playing), t(300)).unwrap(); // pause over
        lg.observe(&c, &s, None, t(310)).unwrap();
        // off
        s.logger_on = false;
        lg.observe(&c, &s, snap("E", Playing), t(400)).unwrap();
        assert_eq!(lg.observe(&c, &s, None, t(500)).unwrap(), None);
        assert_eq!(lg.status(&c, &s, t(500)).unwrap().state, "off");
        assert_eq!(
            rows(&c),
            vec![
                ("C".into(), 40_000, true, None),
                ("D".into(), 10_000, false, None)
            ]
        );
        for bad in [0, -5, MAX_PAUSE_MIN + 1] {
            assert!(pause(&mut s, Some(bad), t(0)).is_err(), "{bad}");
        }
    }
}
