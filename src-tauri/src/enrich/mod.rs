//! Source M, the open music database (BRD §5.1, TSD §5.4, design M10 §7, kb/open-music-db.md): a
//! background worker looks up each artist and album **name** from listening events (sources S, E in
//! library.db) once, in MusicBrainz then Wikidata, at ≤ 1 request/s, and keeps the answers (misses too)
//! in the 0012 `enrich_*` tables. The Settings switch (`set_music_db`) stops lookups and empties those
//! tables in one transaction. Never reads Spotify-provided data (source A).
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::{Connection, params};

use crate::error::Result;

pub mod genres;
pub mod lookup;

use lookup::{AlbumInfo, ArtistInfo, Halt, Http};

/// Project contact for the User-Agent, required by both hosts and by U-11: not about the user (a
/// project address or page). **U-12** done 31 Oct 2026: the user's chosen project page. No scheme: it is a
/// contact, never fetched, so it stays out of the allowed-hosts check.
pub const CONTACT: Option<&str> = Some("github.com/HUMBLEF0OL");

/// Credit wherever genres show (MusicBrainz tags are CC BY-NC-SA 3.0; kb Licence).
pub const ATTRIBUTION: &str = "Genres from MusicBrainz, CC BY-NC-SA 3.0";

/// At most one request per second across both hosts (MusicBrainz rule), with a margin.
pub const PACE: Duration = Duration::from_millis(1100);
/// Idle worker: look for newly played names this often (never at a fixed clock time).
pub const EVERY: Duration = Duration::from_secs(10 * 60);
/// First wait after a 503 / maxlag; doubles up to [`BACKOFF_MAX`].
pub const BACKOFF_MIN: Duration = Duration::from_secs(60);
pub const BACKOFF_MAX: Duration = Duration::from_secs(60 * 60);

/// The switch as the worker sees it. Set from settings at start and by `set_switch` before it deletes,
/// and read with the library lock held before every write: a lookup in flight when the switch goes off
/// never lands after the delete.
static ON: AtomicBool = AtomicBool::new(false);

/// `Runout/<version> ( <contact> )` (kb gotcha), or `None` while there is no contact (U-12).
pub fn user_agent() -> Option<String> {
    CONTACT.map(|c| format!("Runout/{} ( {c} )", env!("CARGO_PKG_VERSION")))
}

/// How long to wait before the next request so two never come closer than [`PACE`].
pub fn wait(last: Option<Instant>, now: Instant) -> Duration {
    last.map_or(Duration::ZERO, |l| {
        (l + PACE).saturating_duration_since(now)
    })
}

/// Next back-off after a `Busy`.
pub fn backoff(prev: Option<Duration>) -> Duration {
    prev.map_or(BACKOFF_MIN, |d| (d * 2).min(BACKOFF_MAX))
}

/// Real HTTP: `net.rs` (allow-list, redirect rule) with the agreed User-Agent, paced.
pub struct Net {
    ua: String,
}

static LAST: Mutex<Option<Instant>> = Mutex::new(None);

impl Http for Net {
    fn get(&mut self, url: &str) -> std::result::Result<(u16, String), Halt> {
        {
            let mut last = LAST.lock().unwrap();
            std::thread::sleep(wait(*last, Instant::now()));
            *last = Some(Instant::now());
        }
        let ua = self.ua.clone();
        tauri::async_runtime::block_on(async move {
            let r = crate::net::send(reqwest::Method::GET, url, |r| {
                r.header(reqwest::header::USER_AGENT, ua)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .timeout(Duration::from_secs(20))
            })
            .await
            .map_err(|_| Halt::Offline)?;
            let status = r.status().as_u16();
            let body = r.text().await.map_err(|_| Halt::Offline)?;
            Ok((status, body))
        })
    }
}

/// Lookups are on (setting) and allowed (a contact exists, U-12).
pub fn active() -> bool {
    ON.load(Ordering::SeqCst) && CONTACT.is_some()
}

pub fn is_on() -> bool {
    ON.load(Ordering::SeqCst)
}

pub fn set_on(on: bool) {
    ON.store(on, Ordering::SeqCst);
}

/// The switch (TSD §5.4): off stops lookups and deletes every `enrich` row in one transaction.
/// The rows go first: a failed delete leaves the switch as it was. Also run whenever a library is
/// opened or swapped in (start-up, restore), so a saved "off" holds over any rows it brings.
pub fn set_switch(conn: &mut Connection, on: bool) -> Result<()> {
    if !on {
        let tx = conn.transaction()?;
        tx.execute_batch(
            "DELETE FROM enrich_artist_genre; DELETE FROM enrich_artist; DELETE FROM enrich_album;",
        )?;
        tx.commit()?;
    }
    set_on(on);
    Ok(())
}

/// A name still to look up, most listened first.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    /// `(key, name as played)`.
    Artist(String, String),
    /// `(artist key, album key, album name, artist name)`.
    Album(String, String, String, String),
}

/// Names not looked up yet: artists, then albums, each by listening time. From `listening_rollup`
/// (sources S, E; private sessions left out: their names stay on the PC).
pub fn pending(conn: &Connection) -> Result<Vec<Pending>> {
    let mut out: Vec<Pending> = conn
        .prepare(
            "SELECT lower(artist_name), MIN(artist_name) FROM listening_rollup
             WHERE private_session = 0 AND events > 0 AND artist_name <> ''
               AND lower(artist_name) NOT IN (SELECT artist FROM enrich_artist)
             GROUP BY 1 ORDER BY SUM(ms_played) DESC",
        )?
        .query_map([], |r| Ok(Pending::Artist(r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let albums = conn
        .prepare(
            "SELECT lower(artist_name), lower(album_name), MIN(album_name), MIN(artist_name)
             FROM listening_rollup
             WHERE private_session = 0 AND events > 0 AND artist_name <> '' AND album_name <> ''
               AND NOT EXISTS (SELECT 1 FROM enrich_album x
                               WHERE x.artist = lower(artist_name) AND x.album = lower(album_name))
             GROUP BY 1, 2 ORDER BY SUM(ms_played) DESC",
        )?
        .query_map([], |r| {
            Ok(Pending::Album(r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    out.extend(albums);
    Ok(out)
}

/// Store one artist answer unless the switch went off meanwhile (checked under the caller's lock).
pub fn save_artist(conn: &mut Connection, key: &str, a: &ArtistInfo) -> Result<bool> {
    if !ON.load(Ordering::SeqCst) {
        return Ok(false);
    }
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT OR REPLACE INTO enrich_artist (artist, match, via, country, looked_up_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![key, a.matched.as_str(), a.via, a.country, crate::db::now()],
    )?;
    tx.execute("DELETE FROM enrich_artist_genre WHERE artist = ?1", [key])?;
    for (g, votes) in &a.genres {
        tx.execute(
            "INSERT OR IGNORE INTO enrich_artist_genre (artist, genre, votes) VALUES (?1, ?2, ?3)",
            params![key, crate::text::clean_line(g, 80), votes],
        )?;
    }
    tx.commit()?;
    Ok(true)
}

pub fn save_album(conn: &Connection, artist: &str, album: &str, a: &AlbumInfo) -> Result<bool> {
    if !ON.load(Ordering::SeqCst) {
        return Ok(false);
    }
    conn.execute(
        "INSERT OR REPLACE INTO enrich_album (artist, album, match, first_year, looked_up_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            artist,
            album,
            a.matched.as_str(),
            a.first_year,
            crate::db::now()
        ],
    )?;
    Ok(true)
}

/// One pass over the pending names: look each up (no lock held), then store it under the lock.
/// Stops at the first `Halt` (offline: wait for the next pass; busy: back off) or when the switch
/// goes off. Returns how many were stored.
pub fn run(db: &Mutex<Connection>, http: &mut impl Http) -> std::result::Result<usize, Halt> {
    let Ok(todo) = pending(&db.lock().unwrap()) else {
        return Ok(0);
    };
    let mut n = 0;
    for p in todo {
        if !ON.load(Ordering::SeqCst) {
            break;
        }
        let saved = match &p {
            Pending::Artist(key, name) => {
                let a = lookup::artist(http, name)?;
                save_artist(&mut db.lock().unwrap(), key, &a)
            }
            Pending::Album(artist, album, album_name, artist_name) => {
                let a = lookup::album(http, album_name, artist_name)?;
                save_album(&db.lock().unwrap(), artist, album, &a)
            }
        };
        n += usize::from(saved.unwrap_or(false));
    }
    Ok(n)
}

/// Start the background worker (lib.rs): background only, one request at a time, never blocking
/// insights (the library lock is held only to read the to-do list and to store each answer).
pub fn spawn(app: tauri::AppHandle) {
    use tauri::Manager;
    let on = app
        .state::<crate::state::AppState>()
        .settings
        .lock()
        .unwrap()
        .music_db;
    // off: rows left by an earlier failure go now (TSD §5.4)
    let st = app.state::<crate::state::AppState>();
    // ponytail: a failed delete here is retried at the next start or switch change
    if set_switch(&mut st.db.lock().unwrap(), on).is_err() {
        set_on(on);
    }
    let Some(ua) = user_agent() else {
        return; // U-12: no contact, no lookups
    };
    std::thread::spawn(move || {
        let mut net = Net { ua };
        let mut back = None;
        loop {
            // Spread from start-up rather than a fixed clock time (MusicBrainz asks for it).
            std::thread::sleep(back.unwrap_or(EVERY));
            if !active() || crate::net::known_offline() {
                continue;
            }
            let st = app.state::<crate::state::AppState>();
            back = match run(&st.db, &mut net) {
                Err(Halt::Busy) => Some(backoff(back)),
                _ => None,
            };
        }
    });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::lookup::tests::Stub;
    use super::*;
    use serde_json::json;

    /// `ON` is global: tests that flip it run under this lock.
    pub(crate) static SWITCH: Mutex<()> = Mutex::new(());

    fn lib() -> Mutex<Connection> {
        let c = crate::db::open_in_memory().unwrap();
        c.execute_batch(
            "INSERT INTO listening_rollup (private_session, item_id, norm_key, track_name, artist_name,
               album_name, events, counts_as_play, ms_played, skips, exported, phone) VALUES
             (0, 0, 'a|1', 't1', 'Synthetic Band', 'First Album', 1, 1, 9000, 0, 1, 0),
             (0, 0, 'b|1', 't2', 'Second Act', 'Other Album', 1, 1, 5000, 0, 1, 0),
             (1, 0, 'c|1', 't3', 'Private Only', 'Hidden', 1, 1, 99000, 0, 1, 0);",
        )
        .unwrap();
        Mutex::new(c)
    }

    #[test]
    fn pace_and_backoff() {
        let t = Instant::now();
        assert_eq!(wait(None, t), Duration::ZERO);
        assert_eq!(wait(Some(t), t), PACE);
        assert_eq!(wait(Some(t), t + PACE * 2), Duration::ZERO);
        assert_eq!(backoff(None), BACKOFF_MIN);
        assert_eq!(backoff(Some(BACKOFF_MIN)), BACKOFF_MIN * 2);
        assert_eq!(backoff(Some(BACKOFF_MAX)), BACKOFF_MAX);
    }

    #[test]
    fn no_contact_no_user_agent_no_lookups() {
        // U-12: until a project contact is set, nothing goes out.
        assert_eq!(CONTACT.is_none(), user_agent().is_none());
        if CONTACT.is_none() {
            let _g = SWITCH.lock().unwrap();
            set_on(true);
            assert!(!active());
        }
    }

    #[test]
    fn pending_is_by_time_and_skips_private_and_done() {
        let db = lib();
        let c = db.lock().unwrap();
        let p = pending(&c).unwrap();
        assert_eq!(
            p,
            [
                Pending::Artist("synthetic band".into(), "Synthetic Band".into()),
                Pending::Artist("second act".into(), "Second Act".into()),
                Pending::Album(
                    "synthetic band".into(),
                    "first album".into(),
                    "First Album".into(),
                    "Synthetic Band".into()
                ),
                Pending::Album(
                    "second act".into(),
                    "other album".into(),
                    "Other Album".into(),
                    "Second Act".into()
                ),
            ]
        );
    }

    #[test]
    fn run_stores_each_name_once_then_switch_off_deletes_and_stops() {
        let _g = SWITCH.lock().unwrap();
        set_on(true);
        let db = lib();
        let artist = |id: &str, name: &str| json!({"artists": [{"id": id, "name": name, "score": 100, "country": "GB"}]});
        let mut h = Stub::new(vec![
            Ok((200, artist("a-1", "Synthetic Band"))),
            Ok((200, json!({"genres": [{"name": "synthpop", "count": 2}]}))),
            // Second Act: two artists of that name
            Ok((
                200,
                json!({"artists": [{"id": "b", "name": "Second Act", "score": 100}, {"id": "c", "name": "Second Act", "score": 99}]}),
            )),
            // its top hit's genres (none), kept as "probably"
            Ok((200, json!({"genres": []}))),
            Ok((
                200,
                json!({"release-groups": [{"title": "First Album", "score": 100, "first-release-date": "1999-01-01",
                "artist-credit": [{"name": "Synthetic Band"}]}]}),
            )),
            Ok((503, json!({}))),
        ]);
        assert_eq!(run(&db, &mut h), Err(Halt::Busy));
        // only names left the PC, never the private-session one
        assert!(h.asked.iter().all(|u| !u.contains("Private")));
        {
            let c = db.lock().unwrap();
            let rows: Vec<(String, String, Option<String>)> = c
                .prepare("SELECT artist, match, country FROM enrich_artist ORDER BY 1")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            assert_eq!(
                rows,
                [
                    ("second act".into(), "same_name".into(), None),
                    ("synthetic band".into(), "matched".into(), Some("GB".into()))
                ]
            );
            // the 503 album is left for later; the other is stored with its year
            assert_eq!(pending(&c).unwrap().len(), 1);
        }
        // a second pass asks only for what is left
        let mut h = Stub::new(vec![
            Ok((200, json!({"release-groups": []}))),
            Ok((200, json!({"search": []}))), // then Wikidata, which has nothing either
        ]);
        assert_eq!(run(&db, &mut h), Ok(1));
        assert_eq!(h.asked.len(), 2);

        set_switch(&mut db.lock().unwrap(), false).unwrap();
        let c = db.lock().unwrap();
        let n: i64 = c
            .query_row(
                "SELECT (SELECT count(*) FROM enrich_artist) + (SELECT count(*) FROM enrich_artist_genre)
                      + (SELECT count(*) FROM enrich_album)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        drop(c);
        // off: nothing is asked and a lookup that was in flight is not stored
        let mut h = Stub::new(vec![]);
        assert_eq!(run(&db, &mut h), Ok(0));
        assert!(h.asked.is_empty());
        let a = lookup::ArtistInfo {
            matched: lookup::Match::None,
            via: None,
            country: None,
            genres: vec![],
        };
        assert!(!save_artist(&mut db.lock().unwrap(), "x", &a).unwrap());
    }
}
