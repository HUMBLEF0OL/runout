//! Insights and the plays list (TSD §4.9, API.md §4): SQL over `item`, `status_history` and
//! `listening_event` only. Every function takes the library connection and nothing else: this
//! module has no handle to the Spotify cache DB (FR-031; `scripts/check.py` greps for it).
use chrono::{DateTime, Months, TimeDelta, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::Result;

pub mod collection;
pub mod coverage;
pub mod explore;
pub mod habits;
pub mod heuristics;
pub mod history;
pub mod listening;
pub mod plays;
pub mod report;
pub mod take;
pub mod terms;
pub mod v3;
#[cfg(test)]
mod v3_tests;

/// API.md §2 `Range`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Range {
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "30d")]
    Month,
    #[serde(rename = "12m")]
    Year,
    #[serde(rename = "all")]
    All,
}

impl Range {
    /// `(start, start of the period before)` as stored UTC strings; `None` for `all`.
    fn bounds(self, now: DateTime<Utc>) -> Option<(String, String)> {
        let (a, b) = match self {
            Self::Week => (now - TimeDelta::days(7), now - TimeDelta::days(14)),
            Self::Month => (now - TimeDelta::days(30), now - TimeDelta::days(60)),
            Self::Year => (now - Months::new(12), now - Months::new(24)),
            Self::All => return None,
        };
        Some((utc(a), utc(b)))
    }

    /// Start of the range (`""` for `all`, which every stored timestamp sorts after).
    fn start(self, now: DateTime<Utc>) -> String {
        self.bounds(now).map(|b| b.0).unwrap_or_default()
    }
}

/// API.md §2 `Freshness`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Freshness {
    pub logger_last_event: Option<String>,
    pub export_latest_event: Option<String>,
}

/// API.md §2 `Sourced<T>` (FR-032). `sources` never holds `A` (DESIGN §7).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sourced<T> {
    pub data: T,
    pub sources: Vec<&'static str>,
    pub freshness: Freshness,
}

/// API.md §4.1 `Covered<T>`: `Sourced<T>` plus the FR-048 `coverage` block.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Covered<T> {
    #[serde(flatten)]
    pub sourced: Sourced<T>,
    pub coverage: coverage::Coverage,
}

/// The stored UTC format (`ended_at_utc`, column defaults).
pub(crate) fn utc(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// The base view as a CTE (`base`): events minus reconciled logger rows (TSD §4.7) and, unless
/// `include_private`, private sessions (TSD §4.8). `local` = local time, UTC when unknown.
// ponytail: a CTE rather than a stored view, because the private-session filter is a setting.
pub(crate) fn base(include_private: bool) -> String {
    let private = if include_private {
        ""
    } else {
        " AND e.private_session = 0"
    };
    format!(
        "WITH base AS NOT MATERIALIZED (SELECT e.*, COALESCE(e.ended_at_local, substr(e.ended_at_utc, 1, 19)) AS local
           FROM listening_event e WHERE e.reconciled_with IS NULL{private})"
    )
}

#[cfg(test)]
thread_local!(static NO_ROLLUP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });

/// True for range `all` (`start` empty): read the rollup. Tests turn it off to compare with `base`.
pub(crate) fn use_rollup(start: &str) -> bool {
    #[cfg(test)]
    if NO_ROLLUP.get() {
        return false;
    }
    start.is_empty()
}

/// Range `all` as a `base` CTE over the 0005 rollup (NFR-04, G-17): one row per record and name,
/// same filters as `base`; `counts_as_play`, `ms_played` are sums, plus `events`, `skips`,
/// `exported`, `phone`. NULL keys are stored as 0 / '' and mapped back here.
pub(crate) fn rollup(include_private: bool) -> String {
    let private = if include_private {
        ""
    } else {
        " AND private_session = 0"
    };
    format!(
        "WITH base AS NOT MATERIALIZED (SELECT private_session, NULLIF(item_id, 0) AS item_id,
           NULLIF(norm_key, '') AS norm_key, track_name, artist_name, album_name, events, counts_as_play,
           ms_played, skips, exported, phone FROM listening_rollup WHERE events > 0{private})"
    )
}

/// Appended to `base(..)`: `src(s, t)` says plays of record `s` count for record `t`: itself, its
/// album (`on_album`) and artists (`by_artist`, direct or through the album). A play linked to an
/// album counts for the album and its artists. Built from links, not events (NFR-04): an
/// insight joins it to events by `item_id`, so each event counts once per record.
pub(crate) const SRC: &str = ", src(s, t) AS (
    SELECT id, id FROM item
    UNION SELECT from_id, to_id FROM item_link WHERE kind IN ('on_album', 'by_artist')
    UNION SELECT l.from_id, l2.to_id FROM item_link l
      JOIN item_link l2 ON l2.from_id = l.to_id AND l2.kind = 'by_artist' WHERE l.kind = 'on_album')";

/// `{col} >= {param}` on an `ended_at_utc` column; for `all` (`start` empty) the `+` keeps SQLite off the time
/// index, whose row-by-row lookups are far slower than one table scan (NFR-04, TSD §3).
pub(crate) fn since(start: &str, col: &str, param: &str) -> String {
    let plus = if start.is_empty() { "+" } else { "" };
    format!("{plus}{col} >= {param}")
}

/// `ms` → hours, one decimal.
pub(crate) fn hours(ms: i64) -> f64 {
    (ms as f64 / 360_000.0).round() / 10.0
}

/// Share as a percentage with one decimal; 0 when there is nothing to divide.
pub(crate) fn pct(part: i64, whole: i64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 * 1000.0 / whole as f64).round() / 10.0
    }
}

pub fn freshness(conn: &Connection) -> Result<Freshness> {
    Ok(conn.query_row(
        // unfiltered: freshness is the newest stored row per source, whatever the filters
        "SELECT (SELECT MAX(ended_at_utc) FROM listening_event WHERE source = 'logger'),
                (SELECT MAX(ended_at_utc) FROM listening_event WHERE source = 'export')",
        [],
        |r| {
            Ok(Freshness {
                logger_last_event: r.get(0)?,
                export_latest_event: r.get(1)?,
            })
        },
    )?)
}

/// `data` with `U` plus `S` (logger) / `E` (export) when the base view holds such events.
pub(crate) fn sourced<T>(conn: &Connection, include_private: bool, data: T) -> Result<Sourced<T>> {
    let (s, e): (bool, bool) = conn.query_row(
        &format!(
            "{} SELECT EXISTS(SELECT 1 FROM base WHERE source = 'logger'),
                       EXISTS(SELECT 1 FROM base WHERE source = 'export')",
            base(include_private)
        ),
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut sources = vec!["U"];
    sources.extend(s.then_some("S"));
    sources.extend(e.then_some("E"));
    Ok(Sourced {
        data,
        sources,
        freshness: freshness(conn)?,
    })
}

/// `YYYY-MM` of the last `n` months up to `now`, oldest first.
pub(crate) fn last_months(now: DateTime<Utc>, n: u32) -> Vec<String> {
    (0..n)
        .rev()
        .map(|k| (now - Months::new(k)).format("%Y-%m").to_string())
        .collect()
}

/// Synthetic library + events shared by the insight tests; every total below is hand-computed
/// in the tests from this list.
#[cfg(test)]
pub(crate) mod fixture {
    use chrono::{DateTime, TimeZone, Utc};
    use rusqlite::Connection;

    use crate::db;
    use crate::importer::events::{Event, Source, insert};

    pub fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 19, 12, 0, 0).unwrap()
    }

    pub const ARTIST: i64 = 1;
    pub const ALBUM: i64 = 2;
    pub const LONELY_ALBUM: i64 = 3;
    pub const T1: i64 = 10;
    pub const T2: i64 = 11;
    pub const T3: i64 = 12;
    pub const TWIN_A: i64 = 20;
    pub const TWIN_B: i64 = 21;

    pub struct Ev<'a> {
        pub source: Source,
        pub utc: &'a str,
        pub ms: i64,
        pub track: &'a str,
        pub artist: &'a str,
        pub album: &'a str,
        pub item: Option<i64>,
        pub platform: &'a str,
        pub skipped: bool,
        pub private: bool,
    }

    pub fn ev<'a>(utc: &'a str, ms: i64, track: &'a str, item: Option<i64>) -> Ev<'a> {
        Ev {
            source: Source::Export,
            utc,
            ms,
            track,
            artist: "Zorblax",
            album: "Album One",
            item,
            platform: "windows",
            skipped: false,
            private: false,
        }
    }

    pub fn put(c: &Connection, e: Ev) -> i64 {
        insert(
            c,
            &Event {
                source: e.source,
                ended_at_utc: e.utc.into(),
                // fixture zone: local = UTC
                ended_at_local: Some(e.utc[..19].into()),
                ms_played: e.ms,
                track_name: e.track.into(),
                artist_name: e.artist.into(),
                album_name: e.album.into(),
                spotify_track_uri: None,
                platform: Some(e.platform.into()),
                reason_start: None,
                reason_end: None,
                shuffle: None,
                duration_ms: None,
                skipped: Some(e.skipped),
                offline: None,
                private_session: e.private,
                item_id: e.item,
                import_batch_id: None,
            },
        )
        .unwrap()
        .unwrap()
    }

    /// Ids of the events, in the order listed (`e1`..`e9`).
    #[allow(clippy::vec_init_then_push)] // one commented push per fixture event reads better
    pub fn build() -> (Connection, Vec<i64>) {
        let c = db::open_in_memory().unwrap();
        c.execute_batch(
            "INSERT INTO item(id, type, display_name, rating, status, in_backlog, added_at, explored_at, review, reviewed_at, modified_at) VALUES
               (1, 'artist', 'Zorblax', NULL, 'in_progress', 0, '2026-01-05T10:00:00Z', NULL, NULL, NULL, '2026-03-01T00:00:00Z'),
               (2, 'album', 'Album One', 4.0, 'explored', 0, '2026-02-01T10:00:00Z', '2026-03-10T10:00:00Z', 'Good.', '2026-03-10T10:00:00Z', '2026-03-01T00:00:00Z'),
               (3, 'album', 'Lonely Album', NULL, 'planned', 1, '2026-09-25T10:00:00Z', NULL, NULL, NULL, '2026-03-01T00:00:00Z'),
               (10, 'track', 'Song One', NULL, 'planned', 0, '2026-02-01T10:00:00Z', NULL, NULL, NULL, '2026-03-01T00:00:00Z'),
               (11, 'track', 'Song Two', 5.0, 'explored', 0, '2025-11-01T10:00:00Z', '2025-12-01T10:00:00Z', NULL, NULL, '2026-03-01T00:00:00Z'),
               (12, 'track', 'Song Three', NULL, 'planned', 1, '2026-02-01T10:00:00Z', NULL, NULL, NULL, '2026-03-01T00:00:00Z'),
               (20, 'track', 'Twin', NULL, 'planned', 0, '2026-02-01T10:00:00Z', NULL, NULL, NULL, '2026-03-01T00:00:00Z'),
               (21, 'track', 'Twin', NULL, 'planned', 0, '2026-02-01T10:00:00Z', NULL, NULL, NULL, '2026-03-01T00:00:00Z');
             INSERT INTO item_link(from_id, to_id, kind) VALUES
               (10, 2, 'on_album'), (11, 2, 'on_album'), (12, 2, 'on_album'),
               (10, 1, 'by_artist'), (11, 1, 'by_artist'), (12, 1, 'by_artist'), (2, 1, 'by_artist');",
        )
        .unwrap();
        let mut ids = vec![];
        // e1: in 7d, phone, counted
        ids.push(put(
            &c,
            Ev {
                platform: "iOS 17.1 (iPhone15,2)",
                ..ev("2026-10-18T10:00:00Z", 200_000, "Song One", Some(T1))
            },
        ));
        // e2: in 7d, skipped, counted
        ids.push(put(
            &c,
            Ev {
                skipped: true,
                ..ev("2026-10-17T10:00:00Z", 100_000, "Song One", Some(T1))
            },
        ));
        // e3: in 7d, not counted (< 30 s), skipped, new artist, no record
        ids.push(put(
            &c,
            Ev {
                artist: "Quuxley",
                album: "Elsewhere",
                skipped: true,
                ..ev("2026-10-16T10:00:00Z", 20_000, "Other Song", None)
            },
        ));
        // e4: logger duplicate of e1 → reconciled below, never counted
        ids.push(put(
            &c,
            Ev {
                source: Source::Logger,
                ..ev("2026-10-18T10:00:40Z", 200_000, "Song One", Some(T1))
            },
        ));
        // e5: private session → excluded unless the setting is on
        ids.push(put(
            &c,
            Ev {
                private: true,
                ..ev("2026-10-15T10:00:00Z", 60_000, "Secret", None)
            },
        ));
        // e6: 2025, Song Two
        ids.push(put(
            &c,
            ev("2025-01-10T10:00:00Z", 120_000, "Song Two", Some(T2)),
        ));
        // e7: the 7 days before the 7d range
        ids.push(put(
            &c,
            ev("2026-10-10T10:00:00Z", 90_000, "Song One", Some(T1)),
        ));
        // e8, e9: ambiguous name ("Twin" ×2), unmatched
        ids.push(put(
            &c,
            Ev {
                artist: "Nobody",
                album: "Album One",
                ..ev("2026-06-01T10:00:00Z", 40_000, "Twin", None)
            },
        ));
        ids.push(put(
            &c,
            Ev {
                artist: "Nobody",
                album: "Album One",
                ..ev("2026-06-02T10:00:00Z", 10_000, "Twin", None)
            },
        ));
        c.execute(
            "UPDATE listening_event SET reconciled_with = ?1 WHERE id = ?2",
            [ids[0], ids[3]],
        )
        .unwrap();
        (c, ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_and_sources() {
        let (c, _) = fixture::build();
        let f = freshness(&c).unwrap();
        assert_eq!(f.logger_last_event.as_deref(), Some("2026-10-18T10:00:40Z"));
        assert_eq!(
            f.export_latest_event.as_deref(),
            Some("2026-10-18T10:00:00Z")
        );
        // the only logger row is reconciled → no S
        assert_eq!(sourced(&c, false, ()).unwrap().sources, vec!["U", "E"]);
    }

    #[test]
    fn helpers() {
        assert_eq!(hours(320_000), 0.1);
        assert_eq!(hours(5_400_000), 1.5);
        assert_eq!(pct(2, 3), 66.7);
        assert_eq!(pct(1, 0), 0.0);
        assert_eq!(
            last_months(fixture::now(), 3),
            vec!["2026-08", "2026-09", "2026-10"]
        );
        assert_eq!(
            Range::Week.bounds(fixture::now()),
            Some(("2026-10-12T12:00:00Z".into(), "2026-10-05T12:00:00Z".into()))
        );
        assert_eq!(Range::All.start(fixture::now()), "");
    }

    /// G-17: every all-time read gives the same answer from the rollup as from the base view,
    /// with private sessions on and off, after each kind of write the triggers must follow.
    #[test]
    fn rollup_equals_base_view() {
        use plays::{PlayQuery, Tab, list_plays, list_unmatched};
        let (c, ids) = fixture::build();
        let now = fixture::now();
        let check = |step: &str| {
            for p in [false, true] {
                let read = |off: bool| {
                    NO_ROLLUP.set(off);
                    let tabs: Vec<_> = [Tab::All, Tab::Unrated, Tab::Missing]
                        .map(|tab| {
                            let q = PlayQuery {
                                tab,
                                range: Range::All,
                                offset: 0,
                                limit: 50,
                                date: None,
                            };
                            list_plays(&c, &q, p, now).unwrap()
                        })
                        .into();
                    let r = (
                        listening::listening(&c, Range::All, p, now).unwrap(),
                        tabs,
                        list_unmatched(&c, p).unwrap(),
                    );
                    NO_ROLLUP.set(false);
                    r
                };
                assert_eq!(read(false), read(true), "{step}, include_private {p}");
            }
        };
        check("fixture");
        plays::link_play(&c, ids[7], fixture::TWIN_A).unwrap();
        check("link_play");
        c.execute(
            "UPDATE listening_event SET reconciled_with = NULL WHERE id = ?1",
            [ids[3]],
        )
        .unwrap();
        check("unreconcile");
        c.execute(
            "UPDATE listening_event SET private_session = 1 WHERE id = ?1",
            [ids[5]],
        )
        .unwrap();
        check("private flip");
        // G-29: names the importer never writes (NULL, not ''): an album whose artist is all NULL
        // (rank `by`), an unmatched group with no artist (list_unmatched); raw rows, synthetic.
        c.execute_batch(
            "INSERT INTO listening_event(source, ended_at_utc, ended_at_local, ms_played, track_name,
               artist_name, album_name, norm_key, dedupe_hash) VALUES
             ('export', '2026-10-17T09:00:00Z', '2026-10-17T09:00:00', 50000, 'Twin', NULL, 'Solo Album', '|twin', 'g29a'),
             ('export', '2026-10-17T09:05:00Z', '2026-10-17T09:05:00', 50000, NULL, NULL, 'Solo Album', NULL, 'g29b')",
        )
        .unwrap();
        check("NULL names");
        c.execute("DELETE FROM item WHERE id = ?1", [fixture::T1])
            .unwrap();
        check("record deleted (FK SET NULL)");
        // unfiltered: a delete, testing the rollup triggers
        c.execute("DELETE FROM listening_event WHERE id = ?1", [ids[0]])
            .unwrap();
        check("event deleted (reconciled partner set NULL)");
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM listening_rollup WHERE events < 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }

    /// Empty library, one play, a record with no plays: every insights read answers (no NULL
    /// aggregate decode error, no panic) and no figure is NaN or infinite.
    #[test]
    fn sparse_libraries_give_finite_answers() {
        use fixture::{ev, now, put};
        use plays::{PlayQuery, Tab};
        let c = crate::db::open_in_memory().unwrap();
        let all = |c: &Connection, item: Option<i64>| {
            let mut out = format!("{:?}", freshness(c).unwrap());
            for r in [Range::Week, Range::Month, Range::Year, Range::All] {
                let q = PlayQuery {
                    tab: Tab::All,
                    range: r,
                    offset: 0,
                    limit: 50,
                    date: None,
                };
                out += &format!(
                    "{:?}{:?}{:?}",
                    listening::listening(c, r, true, now()).unwrap(),
                    collection::collection(c, r, now()).unwrap(),
                    plays::list_plays(c, &q, true, now()).unwrap(),
                );
            }
            out += &format!(
                "{:?}{:?}{:?}",
                take::take(c, true, now()).unwrap(),
                listening::year_review(c, 2026, true).unwrap(),
                plays::list_unmatched(c, true).unwrap(),
            );
            if let Some(id) = item {
                out += &format!("{:?}", plays::item_listening(c, id, true, now()).unwrap());
            }
            assert!(!out.contains("NaN") && !out.contains("inf"), "{out}");
        };
        all(&c, None);
        c.execute_batch(
            "INSERT INTO item(id, type, display_name, status, added_at, modified_at) VALUES
               (1, 'track', 'Lone', 'planned', '2026-10-01T10:00:00Z', '2026-10-01T10:00:00Z'),
               (2, 'album', 'Unplayed', 'explored', '2026-10-01T10:00:00Z', '2026-10-01T10:00:00Z')",
        )
        .unwrap();
        all(&c, Some(2));
        put(&c, ev("2026-10-18T10:00:00Z", 0, "Lone", Some(1)));
        put(&c, ev("2026-10-18T11:00:00Z", 45_000, "Lone", Some(1)));
        all(&c, Some(1));
        all(&c, Some(2));
    }
}

/// A text column without a copy (NFR-04 row passes); NULL is an error.
pub(crate) fn text<'a>(r: &'a rusqlite::Row, i: usize) -> rusqlite::Result<&'a str> {
    r.get_ref(i)?.as_str().map_err(Into::into)
}

/// A nullable text column without a copy.
pub(crate) fn text_opt<'a>(r: &'a rusqlite::Row, i: usize) -> rusqlite::Result<Option<&'a str>> {
    r.get_ref(i)?.as_str_or_null().map_err(Into::into)
}
