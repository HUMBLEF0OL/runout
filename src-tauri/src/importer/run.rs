//! `run_import` (TSD §4.8): files are read and parsed before the db lock, then written in one
//! transaction. Each file set gets an `import_batch` keyed by content checksum, so the same files
//! again reuse their batch and every row is a dedupe-hash duplicate: 0 added.
use std::path::Path;
use std::time::Instant;

use rusqlite::{Connection, Transaction, params};
use serde::Serialize;

use super::events::insert;
use super::matching::Matcher;
use super::playlists::{self, Playlist};
use super::reconcile::reconcile;
use super::source::{self, checksum};
use super::streaming::{self, LocalTime, Parsed};
use crate::error::Result;

/// A parsed export, ready to write.
pub struct Loaded {
    pub started: Instant,
    pub files: Vec<String>,
    pub streaming: Option<(String, Parsed)>,
    pub playlists: Option<(String, Vec<Playlist>)>,
}

/// Read + parse; any unreadable file → `import_unrecognized` naming the file and field (TSD §6).
pub fn load(path: &Path) -> Result<Loaded> {
    let started = Instant::now();
    let e = source::read(path)?;
    let streaming = if e.streaming.is_empty() {
        None
    } else {
        let parsed = streaming::parse(&e.streaming, &mut LocalTime::system())?;
        Some((checksum("streaming_history", &e.streaming), parsed))
    };
    let playlists = if e.playlists.is_empty() {
        None
    } else {
        let parsed = playlists::parse(&e.playlists)?;
        Some((checksum("account_data", &e.playlists), parsed))
    };
    Ok(Loaded {
        started,
        files: e.names(),
        streaming,
        playlists,
    })
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ImportResult {
    pub batch_id: i64,
    pub added: usize,
    pub duplicates: usize,
    pub reconciled: usize,
    pub playlists: usize,
    pub took_ms: u64,
}

fn batch(tx: &Transaction, kind: &str, sum: &str) -> Result<i64> {
    Ok(tx.query_row(
        "INSERT INTO import_batch(kind, checksum) VALUES (?1, ?2)
         ON CONFLICT(checksum) DO UPDATE SET checksum = excluded.checksum RETURNING id",
        params![kind, sum],
        |r| r.get(0),
    )?)
}

/// All writes inside `tx`; the caller commits (`run`) or rolls back (`preview`).
pub fn apply(tx: &Transaction, l: &Loaded) -> Result<ImportResult> {
    let mut r = ImportResult {
        batch_id: 0,
        added: 0,
        duplicates: 0,
        reconciled: 0,
        playlists: 0,
        took_ms: 0,
    };
    if let Some((sum, lists)) = &l.playlists {
        r.batch_id = batch(tx, "account_data", sum)?;
        r.playlists = playlists::store(tx, r.batch_id, lists)?;
        tx.execute(
            "UPDATE import_batch SET record_count = ?2 WHERE id = ?1",
            params![r.batch_id, r.playlists as i64],
        )?;
    }
    if let Some((sum, parsed)) = &l.streaming {
        r.batch_id = batch(tx, "streaming_history", sum)?;
        let m = Matcher::load(tx)?;
        for e in &parsed.events {
            let mut e = e.clone();
            e.import_batch_id = Some(r.batch_id);
            e.item_id = m.find(
                e.spotify_track_uri.as_deref(),
                &e.track_name,
                &e.artist_name,
            );
            match insert(tx, &e)? {
                Some(_) => r.added += 1,
                None => r.duplicates += 1,
            }
        }
        r.reconciled = reconcile(tx)?;
        // ponytail: the triggers already kept them; one recount pass guards against drift (design
        // §4.3.4). Drop the triggers during import instead if perf_listening's import time needs it.
        crate::db::rebuild_rollups(tx)?;
        tx.execute(
            "UPDATE import_batch SET
               record_count = (SELECT count(*) FROM listening_event WHERE import_batch_id = ?1),
               latest_event_utc = (SELECT max(ended_at_utc) FROM listening_event WHERE import_batch_id = ?1)
             WHERE id = ?1",
            [r.batch_id],
        )?;
    }
    r.took_ms = l.started.elapsed().as_millis() as u64;
    Ok(r)
}

pub fn run(conn: &mut Connection, l: &Loaded) -> Result<ImportResult> {
    let tx = conn.transaction()?;
    let mut r = apply(&tx, l)?;
    tx.commit()?;
    r.took_ms = l.started.elapsed().as_millis() as u64;
    Ok(r)
}

#[derive(Debug, Serialize)]
pub struct ImportRecord {
    pub id: i64,
    /// API.md `list_imports`: `extended` (streaming history) | `account` (playlists).
    pub kind: &'static str,
    pub imported_at: String,
    pub record_count: i64,
    pub latest_event_utc: Option<String>,
}

/// Streaming-history batches first (Settings reads "covers plays up to" from row 0), newest first.
pub fn list_imports(conn: &Connection) -> Result<Vec<ImportRecord>> {
    let mut st = conn.prepare(
        "SELECT id, kind = 'streaming_history', imported_at, record_count, latest_event_utc
         FROM import_batch ORDER BY kind = 'streaming_history' DESC, id DESC",
    )?;
    let rows = st.query_map([], |r| {
        Ok(ImportRecord {
            id: r.get(0)?,
            kind: if r.get(1)? { "extended" } else { "account" },
            imported_at: r.get(2)?,
            record_count: r.get(3)?,
            latest_event_utc: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::importer::events::{Event, Source};
    use crate::importer::source::fixture;

    fn count(c: &Connection, sql: &str) -> i64 {
        c.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn second_import_adds_nothing() {
        let mut c = db::open_in_memory().unwrap();
        let l = load(&fixture()).unwrap();
        let r = run(&mut c, &l).unwrap();
        // 16 music rows (15 Audio + 1 Video), one repeated across the Audio files
        assert_eq!((r.added, r.duplicates, r.playlists), (15, 1, 2));
        let again = run(&mut c, &load(&fixture()).unwrap()).unwrap();
        assert_eq!(
            (again.added, again.duplicates, again.batch_id),
            (0, 16, r.batch_id)
        );
        assert_eq!(count(&c, "SELECT count(*) FROM listening_event"), 15);
        assert_eq!(count(&c, "SELECT count(*) FROM exported_playlist"), 2);
        assert_eq!(count(&c, "SELECT count(*) FROM import_batch"), 2);
        let imports = list_imports(&c).unwrap();
        assert_eq!(imports[0].kind, "extended");
        assert_eq!(imports[0].record_count, 15);
        assert_eq!(
            imports[0].latest_event_utc.as_deref(),
            Some("2026-10-25T01:00:00Z")
        );
        assert_eq!((imports[1].kind, imports[1].record_count), ("account", 2));
    }

    /// Rows with null metadata, 0 ms and exact repeats inside one file import without error;
    /// a repeat is a duplicate, a 0 ms row is stored but isn't a play.
    #[test]
    fn sparse_and_repeated_rows_import_cleanly() {
        let d = std::env::temp_dir().join(format!("runout-sparse-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let row = |ts: &str, ms: u32| {
            format!(
                r#"{{"ts": "{ts}", "ms_played": {ms}, "platform": null, "master_metadata_track_name": null,
                    "master_metadata_album_artist_name": null, "master_metadata_album_album_name": null,
                    "spotify_track_uri": "spotify:track:AAAAAAAAAAAAAAAAAAAAAA", "reason_start": null,
                    "reason_end": null, "shuffle": null, "skipped": null, "offline": null, "incognito_mode": null}}"#
            )
        };
        let a = row("2026-01-01T00:00:00Z", 0);
        let b = row("2026-01-01T00:05:00Z", 90_000);
        std::fs::write(
            d.join("Streaming_History_Audio_2026.json"),
            format!("[{a}, {b}, {b}]"),
        )
        .unwrap();
        let mut c = db::open_in_memory().unwrap();
        let r = run(&mut c, &load(&d).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!((r.added, r.duplicates), (2, 1));
        assert_eq!(
            count(
                &c,
                "SELECT count(*) FROM listening_event WHERE counts_as_play"
            ),
            1
        );
    }

    #[test]
    fn bad_file_aborts_everything() {
        let d = std::env::temp_dir().join(format!("runout-bad-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for f in std::fs::read_dir(fixture()).unwrap() {
            let p = f.unwrap().path();
            std::fs::copy(&p, d.join(p.file_name().unwrap())).unwrap();
        }
        std::fs::write(
            d.join("Streaming_History_Audio_2027_2.json"),
            r#"[{"ts": 5}]"#,
        )
        .unwrap();
        let e = load(&d).err().unwrap();
        assert_eq!(e.code, crate::error::Code::ImportUnrecognized);
        assert!(
            e.detail
                .unwrap()
                .contains("Streaming_History_Audio_2027_2.json")
        );
        let _ = std::fs::remove_dir_all(d);
    }

    /// Acceptance (M4): no IP / user agent / username value in any TEXT column of library.db.
    #[test]
    fn forbidden_fields_never_reach_the_db() {
        let mut c = db::open_in_memory().unwrap();
        run(&mut c, &load(&fixture()).unwrap()).unwrap();
        let tables: Vec<String> = c
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        let mut checked = 0;
        for t in tables {
            let cols: Vec<String> = c
                .prepare(&format!("SELECT name FROM pragma_table_info('{t}')"))
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            for col in cols {
                for bad in [
                    "198.51.100.23",
                    "203.0.113.77",
                    "sentinel-agent",
                    "sentinel-user",
                    "SENTINEL_LIBRARY",
                    "ZZ",
                ] {
                    let sql = format!(
                        "SELECT count(*) FROM \"{t}\" WHERE CAST(\"{col}\" AS TEXT) LIKE '%{bad}%'"
                    );
                    assert_eq!(count(&c, &sql), 0, "{bad} in {t}.{col}");
                }
                checked += 1;
            }
        }
        assert!(checked > 50, "walked every column ({checked})");
    }

    /// M10 WI 2: the 0008 rollups equal a full recount after inserts, reconcile, import, deletes
    /// (incl. an artist's first play and an export partner, which un-reconciles its logger twin)
    /// and edits of counted columns. Synthetic rows.
    #[test]
    fn rollups_equal_a_recount_after_every_write() {
        let mut c = db::open_in_memory().unwrap();
        let check = |c: &Connection, step: &str| {
            let (kept, recount) = db::rollups_vs_recount(c);
            assert_eq!(kept, recount, "{step}");
            assert!(!kept.is_empty() || step == "empty", "{step}");
        };
        let ev = |ts: &str, local: Option<&str>, track: &str, artist: &str, private: bool| Event {
            source: Source::Logger,
            ended_at_utc: ts.into(),
            ended_at_local: local.map(str::to_string),
            ms_played: 45_000,
            track_name: track.into(),
            artist_name: artist.into(),
            album_name: String::new(),
            spotify_track_uri: None,
            platform: None,
            reason_start: None,
            reason_end: None,
            shuffle: Some(true),
            duration_ms: Some(200_000),
            skipped: None,
            offline: None,
            private_session: private,
            item_id: None,
            import_batch_id: None,
        };
        let mut ids = vec![];
        for e in [
            ev(
                "2026-03-01T10:00:30Z",
                Some("2026-03-01T11:00:30"),
                "Glass Harbor",
                "Fixture Lanterns",
                false,
            ),
            ev(
                "2020-01-01T23:30:00Z",
                Some("2020-01-02T00:30:00"),
                "Early One",
                "FIXTURE LANTERNS",
                false,
            ),
            ev(
                "2026-05-01T20:01:05Z",
                None,
                "Loop Song",
                "Fixture Lanterns",
                true,
            ),
            ev("2026-06-01T08:00:00Z", None, "No Artist", "", false),
        ] {
            ids.push(insert(&c, &e).unwrap().unwrap());
        }
        check(&c, "insert");
        // a 5 s event before the artist's first play: not a play (D25), so not its first play
        let short = Event {
            ms_played: 5_000,
            ..ev(
                "2019-06-01T10:00:00Z",
                None,
                "Skipped",
                "Fixture Lanterns",
                false,
            )
        };
        let short_id = insert(&c, &short).unwrap().unwrap();
        check(&c, "earlier non-play");
        let first: i64 = c
            .query_row(
                "SELECT event_id FROM artist_first_play WHERE artist = 'fixture lanterns' AND private_session = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(first, ids[1], "first play skips the 5 s event {short_id}");
        run(&mut c, &load(&fixture()).unwrap()).unwrap();
        check(&c, "import");
        let reconciled = count(
            &c,
            "SELECT count(*) FROM listening_event WHERE reconciled_with IS NOT NULL",
        );
        assert!(reconciled > 0, "the fixture reconciles a logger play");
        c.execute("UPDATE listening_event SET reconciled_with = NULL", [])
            .unwrap();
        check(&c, "unreconcile");
        reconcile(&c).unwrap();
        check(&c, "reconcile");
        // unfiltered: deletes and edits, testing the rollup triggers
        c.execute("DELETE FROM listening_event WHERE id = ?1", [ids[1]])
            .unwrap();
        check(&c, "first play of an artist deleted");
        c.execute(
            "DELETE FROM listening_event WHERE id = (SELECT reconciled_with FROM listening_event
               WHERE reconciled_with IS NOT NULL LIMIT 1)",
            [],
        )
        .unwrap();
        check(&c, "export partner deleted (logger twin counts again)");
        c.execute(
            "UPDATE listening_event SET private_session = 1 - private_session WHERE id = ?1",
            [ids[0]],
        )
        .unwrap();
        check(&c, "private flip");
        // ids[0] is now the private partition's first play; under 30 s it stops being one
        c.execute(
            "UPDATE listening_event SET ms_played = 5000 WHERE id = ?1",
            [ids[0]],
        )
        .unwrap();
        check(&c, "first play shortened");
        c.execute(
            "UPDATE listening_event SET ended_at_utc = '2019-12-31T00:00:00Z', ended_at_local = NULL,
               ms_played = 10, artist_name = 'Renamed Duo' WHERE id = ?1",
            [ids[2]],
        )
        .unwrap();
        check(&c, "time, length and artist edited");
        c.execute("UPDATE listening_event SET item_id = NULL", [])
            .unwrap();
        check(&c, "link change");
        c.execute("DELETE FROM listening_event", []).unwrap();
        check(&c, "empty");
        assert_eq!(count(&c, "SELECT count(*) FROM artist_first_play"), 0);
        assert_eq!(
            count(
                &c,
                "SELECT count(*) FROM listening_day WHERE events <> 0 OR ms_played <> 0"
            ),
            0
        );
    }

    /// Acceptance (M4): logger plays already in the library are reconciled, never double counted —
    /// featured artist, compilation and back-to-back repeat included.
    #[test]
    fn overlap_has_no_double_counts() {
        let mut c = db::open_in_memory().unwrap();
        let logger = |ts: &str, track: &str, artist: &str| Event {
            source: Source::Logger,
            ended_at_utc: ts.into(),
            ended_at_local: None,
            ms_played: 59_000,
            track_name: track.into(),
            artist_name: artist.into(),
            album_name: String::new(),
            spotify_track_uri: None, // SMTC gives no uri
            platform: None,
            reason_start: None,
            reason_end: None,
            shuffle: None,
            duration_ms: None,
            skipped: None,
            offline: None,
            private_session: false,
            item_id: None,
            import_batch_id: None,
        };
        for e in [
            logger("2026-03-01T10:00:30Z", "Glass Harbor", "Fixture Lanterns"),
            logger(
                "2026-04-02T18:00:50Z",
                "Copper Lines (feat. Mira Vale)",
                "Odd Meridian, Mira Vale",
            ),
            logger("2026-04-10T20:59:20Z", "Night Ferry", "Harbor Kid"), // compilation: album artist differs
            logger("2026-05-01T20:01:05Z", "Loop Song", "Fixture Lanterns"),
            logger("2026-05-01T20:02:03Z", "Loop Song", "Fixture Lanterns"),
            logger("2026-08-01T10:00:00Z", "Logger Only", "Fixture Lanterns"),
            logger("2026-03-01T10:05:00Z", "Glass Harbor", "Fixture Lanterns"), // 300 s off
        ] {
            insert(&c, &e).unwrap().unwrap();
        }
        let r = run(&mut c, &load(&fixture()).unwrap()).unwrap();
        assert_eq!(r.reconciled, 5);
        // counted plays = export plays + the two unmatched logger plays
        let export_plays = count(
            &c,
            "SELECT count(*) FROM listening_event WHERE source = 'export' AND counts_as_play",
        );
        let counted = count(
            &c,
            "SELECT count(*) FROM listening_event WHERE counts_as_play
             AND NOT (source = 'logger' AND reconciled_with IS NOT NULL)",
        );
        assert_eq!(counted, export_plays + 2);
        // each repeat got its own partner
        let partners: Vec<String> = c
            .prepare(
                "SELECT e.ended_at_utc FROM listening_event l JOIN listening_event e ON e.id = l.reconciled_with
                 WHERE l.track_name = 'Loop Song' ORDER BY l.ended_at_utc",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(partners, ["2026-05-01T20:01:00Z", "2026-05-01T20:02:00Z"]);
        assert_eq!(
            run(&mut c, &load(&fixture()).unwrap()).unwrap().reconciled,
            0
        );
    }
}
