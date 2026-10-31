//! library.db connection: WAL, foreign keys, busy timeout, migrations over `PRAGMA user_version` (TSD §1, §3).
use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use crate::error::AppError;

/// Embedded migrations; index + 1 = the `user_version` after it runs. Append only.
const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/0001_init.sql"),
    include_str!("../../migrations/0002_fts_secure_delete.sql"),
    include_str!("../../migrations/0003_listening.sql"),
    include_str!("../../migrations/0004_listening_perf.sql"),
    include_str!("../../migrations/0005_listening_rollup.sql"),
    include_str!("../../migrations/0006_added_via_inbox.sql"),
    include_str!("../../migrations/0007_backlog_history.sql"),
    include_str!("../../migrations/0008_listening_v3.sql"),
    include_str!("../../migrations/0009_import_already_had.sql"),
    include_str!("../../migrations/0010_explore_view.sql"),
    include_str!("../../migrations/0011_insights_perf.sql"),
];

/// Schema version a fully migrated library has.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Recount the 0008 rollups (`listening_day`, `listening_month_artist`, `artist_first_play`) from
/// `listening_event` in one pass: the block between 0008's rebuild markers, so the migration's first
/// fill and this are one definition. Run after an import (design/M10-insights-v3.md §4.3.4).
pub fn rebuild_rollups(conn: &Connection) -> Result<(), AppError> {
    let sql = MIGRATIONS[7];
    let (a, b) = (
        sql.find("-- rebuild:begin").expect("0008 rebuild marker"),
        sql.find("-- rebuild:end").expect("0008 rebuild marker"),
    );
    conn.execute_batch(&sql[a..b])?;
    Ok(())
}

/// Test helper: the 0008 rollups as text (rows with no events left out, as readers do), and the
/// same after a full recount (rolled back). Equal = the triggers kept them right.
#[cfg(test)]
pub fn rollups_vs_recount(conn: &Connection) -> (String, String) {
    let dump = |c: &Connection| -> String {
        [
            "SELECT day, private_session, events, plays, ms_played FROM listening_day WHERE events > 0 ORDER BY 1, 2",
            "SELECT month, artist, private_session, events, plays, ms_played FROM listening_month_artist WHERE events > 0 ORDER BY 1, 2, 3",
            "SELECT private_session, artist, event_id, ended_at_utc FROM artist_first_play ORDER BY 1, 2",
        ]
        .iter()
        .map(|q| {
            let mut st = c.prepare(q).unwrap();
            let n = st.column_count();
            st.query_map([], |r| {
                Ok((0..n)
                    .map(|i| format!("{:?}", r.get::<_, rusqlite::types::Value>(i).unwrap()))
                    .collect::<Vec<_>>()
                    .join("|"))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
            .join("
")
        })
        .collect::<Vec<_>>()
        .join("
--
")
    };
    let kept = dump(conn);
    conn.execute_batch("SAVEPOINT recount").unwrap();
    rebuild_rollups(conn).unwrap();
    let recount = dump(conn);
    conn.execute_batch("ROLLBACK TO recount; RELEASE recount")
        .unwrap();
    (kept, recount)
}

/// Current time as the ISO-8601 UTC string every table stores (TSD §3).
pub fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Open (creating if needed) and migrate the library at `path`.
pub fn open(path: &Path) -> Result<Connection, AppError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    configure(&mut conn)?;
    Ok(conn)
}

/// A migrated in-memory library, for tests.
pub fn open_in_memory() -> Result<Connection, AppError> {
    let mut conn = Connection::open_in_memory()?;
    configure(&mut conn)?;
    Ok(conn)
}

fn configure(conn: &mut Connection) -> Result<(), AppError> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Deleted notes/reviews are zeroed on disk, not left in freed pages (FR-022).
    conn.pragma_update(None, "secure_delete", "ON")?;
    // 64 MB page cache (default 2 MB): random dedupe_hash index inserts at 500k events ran ~3x
    // slower without it (NFR-04 import, M4 WI 9). A ceiling, filled only as pages are read.
    conn.pragma_update(None, "cache_size", -65536)?;
    conn.pragma_update(None, "mmap_size", 1i64 << 30)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    migrate(conn)
}

/// Run every migration newer than `user_version`, each in its own transaction. Foreign keys are
/// off meanwhile (a table rebuild's DROP TABLE must not cascade, 0006) and `legacy_alter_table` on
/// (its rename must not rewrite views/triggers); a migration that leaves a dangling reference
/// fails `foreign_key_check` and rolls back. Both pragmas are restored either way.
pub fn migrate(conn: &mut Connection) -> Result<(), AppError> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current >= SCHEMA_VERSION {
        return Ok(());
    }
    let fk: bool = conn.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
    conn.pragma_update(None, "foreign_keys", "OFF")?; // no-op inside a transaction, so outside
    conn.pragma_update(None, "legacy_alter_table", "ON")?;
    let run = |conn: &mut Connection| -> Result<(), AppError> {
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current.max(0) as usize) {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            let dangling: i64 =
                tx.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                    r.get(0)
                })?;
            if dangling > 0 {
                return Err(AppError::internal("A library upgrade left broken links.")
                    .with_detail(format!("migration {} foreign_key_check", i + 1)));
            }
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    };
    let r = run(conn);
    conn.pragma_update(None, "legacy_alter_table", "OFF")?;
    conn.pragma_update(None, "foreign_keys", fk)?;
    r
}

/// Synthetic library for tests (TSD §8: never real values): random 22-char ids, made-up names.
// ponytail: test-only; step 12's 10k perf generator can promote it.
#[cfg(test)]
pub fn fixture(conn: &Connection, n: usize) {
    use rand::RngExt;
    const CH: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    let mut id = || -> String {
        (0..22)
            .map(|_| CH[rng.random_range(0..CH.len())] as char)
            .collect()
    };
    conn.execute_batch(
        r#"INSERT INTO tag(id, name, description) VALUES (1, 'late night', 'after midnight'), (2, 'jazz', NULL);
           INSERT INTO saved_filter(name, definition_json) VALUES ('Top jazz', '{"tags":["jazz"],"min_rating":4}');"#,
    )
    .unwrap();
    let artist: i64 = conn
        .query_row(
            "INSERT INTO item(type, display_name, spotify_uri) VALUES ('artist', 'Synthetic Quartet', ?1) RETURNING id",
            [format!("spotify:artist:{}", id())],
            |r| r.get(0),
        )
        .unwrap();
    for i in 0..n {
        let kind = if i % 3 == 0 { "album" } else { "track" };
        let it: i64 = conn
            .query_row(
                "INSERT INTO item(type, display_name, spotify_uri, status, rating, review, backlog_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) RETURNING id",
                rusqlite::params![
                    kind,
                    format!("Fixture {kind} {i}"),
                    format!("spotify:{kind}:{}", id()),
                    if i % 2 == 0 { "explored" } else { "planned" },
                    (i % 2 == 0).then_some(4.5),
                    (i % 4 == 0).then_some("a made-up review"),
                    i as i64,
                ],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute_batch(&format!(
            "INSERT INTO field_provenance VALUES ({it}, 'display_name', 'promoted'), ({it}, 'rating', 'user');
             INSERT INTO item_tag VALUES ({it}, {});
             INSERT INTO item_link VALUES ({it}, {artist}, 'by_artist');
             INSERT INTO status_history(item_id, from_status, to_status) VALUES ({it}, NULL, 'planned');",
            1 + i % 2
        ))
        .unwrap();
    }
    conn.execute("INSERT INTO usage_event(kind) VALUES ('app_open')", [])
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(c: &Connection) -> i64 {
        c.pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn fresh_db_migrates_to_latest_and_rerun_is_noop() {
        let mut c = open_in_memory().unwrap();
        assert_eq!(version(&c), SCHEMA_VERSION);
        c.execute(
            "INSERT INTO item(type, display_name) VALUES ('album', 'A')",
            [],
        )
        .unwrap();
        migrate(&mut c).unwrap();
        assert_eq!(version(&c), SCHEMA_VERSION);
        let n: i64 = c
            .query_row("SELECT count(*) FROM item", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "re-running migrations must not touch data");
    }

    /// 0011 (M10 WI 5): `listening_event_norm` is replaced by `listening_event_norm_time`, which
    /// report.rs names with INDEXED BY (a missing index there is a query error, not a slow query).
    #[test]
    fn v11_swaps_the_norm_index() {
        let c = open_in_memory().unwrap();
        let idx: Vec<String> = c
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'listening_event_norm%'",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(idx, ["listening_event_norm_time"]);
    }

    #[test]
    fn file_db_uses_wal_and_foreign_keys() {
        let dir = std::env::temp_dir().join(format!("runout-db-{}", std::process::id()));
        let path = dir.join("library.db");
        let c = open(&path).unwrap();
        let mode: String = c
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        let fk: i64 = c
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert_eq!((mode.as_str(), fk), ("wal", 1));
        drop(c);
        let c = open(&path).unwrap(); // reopen: already migrated
        assert_eq!(version(&c), SCHEMA_VERSION);
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Upgrade path (SOURCE_MAP "fresh + upgrade"): a review deleted under schema 1 is gone from the
    /// file once the library is migrated, vacuumed and checkpointed.
    #[test]
    fn deleted_review_is_gone_after_upgrade_from_v1() {
        const TOKEN: &str = "zqxv-upgrade-token-7731";
        let dir = std::env::temp_dir().join(format!("runout-upgrade-{}", std::process::id()));
        let path = dir.join("library.db");
        std::fs::create_dir_all(&dir).unwrap();
        let c = Connection::open(&path).unwrap();
        c.pragma_update(None, "journal_mode", "WAL").unwrap();
        c.execute_batch(MIGRATIONS[0]).unwrap();
        c.pragma_update(None, "user_version", 1).unwrap();
        c.execute(
            "INSERT INTO item(id, type, display_name, review) VALUES (1, 'album', 'A', ?1)",
            [TOKEN],
        )
        .unwrap();
        c.execute("DELETE FROM item WHERE id = 1", []).unwrap();
        drop(c);
        let c = open(&path).unwrap();
        assert_eq!(version(&c), SCHEMA_VERSION);
        c.execute_batch("VACUUM").unwrap();
        c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .unwrap();
        drop(c);
        for f in ["library.db", "library.db-wal"] {
            let bytes = std::fs::read(dir.join(f)).unwrap_or_default();
            assert!(
                !bytes.windows(TOKEN.len()).any(|w| w == TOKEN.as_bytes()),
                "token left in {f}"
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// v7 library with plays → v8 (M10 WI 2): plays untouched, `duration_ms` NULL on old rows, the
    /// rollups filled from them (reconciled rows out, private apart, local date, first play per artist).
    #[test]
    fn v7_with_plays_upgrades_to_v8() {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "legacy_alter_table", "ON").unwrap(); // as migrate() runs 0006
        for m in &MIGRATIONS[..7] {
            c.execute_batch(m).unwrap();
        }
        c.pragma_update(None, "legacy_alter_table", "OFF").unwrap();
        c.pragma_update(None, "foreign_keys", "ON").unwrap();
        c.pragma_update(None, "user_version", 7).unwrap();
        // synthetic: two artists (one differing only in case), a reconciled logger twin, a private
        // play, a play with no artist, one with no local time (UTC date used)
        c.execute_batch(
            "INSERT INTO listening_event(id, source, ended_at_utc, ended_at_local, ms_played, artist_name, private_session, dedupe_hash) VALUES
               (1, 'export', '2026-03-01T23:30:00Z', '2026-03-02T00:30:00', 40000, 'Fixture Band', 0, 'a'),
               (2, 'logger', '2026-03-01T23:30:10Z', '2026-03-02T00:30:10', 40000, 'Fixture Band', 0, 'b'),
               (3, 'export', '2026-02-27T10:00:00Z', '2026-02-27T11:00:00', 10000, 'fixture band', 0, 'c'),
               (4, 'export', '2026-01-05T10:00:00Z', '2026-01-05T11:00:00', 90000, 'Other Duo', 1, 'd'),
               (5, 'export', '2026-03-02T08:00:00Z', NULL, 30000, NULL, 0, 'e');
             UPDATE listening_event SET reconciled_with = 1 WHERE id = 2;",
        )
        .unwrap();
        migrate(&mut c).unwrap();
        assert_eq!(version(&c), SCHEMA_VERSION);
        let q = |sql: &str| -> String {
            let mut st = c.prepare(sql).unwrap();
            let n = st.column_count();
            st.query_map([], |r| {
                Ok((0..n)
                    .map(|i| format!("{:?}", r.get::<_, rusqlite::types::Value>(i).unwrap()))
                    .collect::<Vec<_>>()
                    .join(" "))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
            .join("; ")
        };
        assert_eq!(
            q("SELECT count(*), count(duration_ms), sum(ms_played) FROM listening_event"),
            "Integer(5) Integer(0) Integer(210000)"
        );
        let t = |s: &str| format!("Text(\"{s}\")");
        let i = |n: i64| format!("Integer({n})");
        assert_eq!(
            q(
                "SELECT day, private_session, events, plays, ms_played FROM listening_day ORDER BY 1"
            ),
            [
                [t("2026-01-05"), i(1), i(1), i(1), i(90000)].join(" "),
                [t("2026-02-27"), i(0), i(1), i(0), i(10000)].join(" "),
                [t("2026-03-02"), i(0), i(2), i(2), i(70000)].join(" "),
            ]
            .join("; ")
        );
        assert_eq!(
            q("SELECT month, artist, events, ms_played FROM listening_month_artist ORDER BY 1, 2"),
            [
                [t("2026-01"), t("other duo"), i(1), i(90000)].join(" "),
                [t("2026-02"), t("fixture band"), i(1), i(10000)].join(" "),
                [t("2026-03"), t(""), i(1), i(30000)].join(" "),
                [t("2026-03"), t("fixture band"), i(1), i(40000)].join(" "),
            ]
            .join("; ")
        );
        assert_eq!(
            q("SELECT private_session, artist, event_id FROM artist_first_play ORDER BY 1, 2"),
            [
                // e3 (10 s) is earlier but not a play (D25): first play is e1
                [i(0), t("fixture band"), i(1)].join(" "),
                [i(1), t("other duo"), i(4)].join(" "),
            ]
            .join("; ")
        );
        assert_eq!(q("SELECT count(*) FROM logger_run"), "Integer(0)");
        let (kept, recount) = rollups_vs_recount(&c);
        assert_eq!(kept, recount);
        assert!(
            c.execute(
                "UPDATE listening_event SET duration_ms = 0 WHERE id = 1",
                []
            )
            .is_err(),
            "a track length is positive or NULL"
        );
    }

    /// v2 library with data → v3: data intact, R2 tables usable, play rule and one-to-one link enforced.
    #[test]
    fn v2_with_data_upgrades_to_v3() {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "foreign_keys", "ON").unwrap();
        c.execute_batch(MIGRATIONS[0]).unwrap();
        c.execute_batch(MIGRATIONS[1]).unwrap();
        c.pragma_update(None, "user_version", 2).unwrap();
        fixture(&c, 6);
        let count = |c: &Connection, t: &str| -> i64 {
            c.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
                .unwrap()
        };
        let before: Vec<i64> = ["item", "item_link", "tag", "item_fts"]
            .iter()
            .map(|t| count(&c, t))
            .collect();
        migrate(&mut c).unwrap();
        assert_eq!(version(&c), MIGRATIONS.len() as i64); // latest, whatever comes after 0003
        let after: Vec<i64> = ["item", "item_link", "tag", "item_fts"]
            .iter()
            .map(|t| count(&c, t))
            .collect();
        assert_eq!(before, after);

        c.execute_batch(
            "INSERT INTO listening_event(id, source, ended_at_utc, ms_played, dedupe_hash, item_id)
               VALUES (1, 'logger', '2026-01-01T00:00:00Z', 29999, 'a', 2),
                      (2, 'export', '2026-01-01T00:00:30Z', 30000, 'b', NULL),
                      (3, 'export', '2026-01-01T00:01:00Z', 30000, 'c', NULL);
             UPDATE listening_event SET reconciled_with = 2 WHERE id = 1;",
        )
        .unwrap();
        let plays: Vec<i64> = c
            .prepare("SELECT counts_as_play FROM listening_event ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(plays, vec![0, 1, 1]);
        let dup = "INSERT INTO listening_event(source, ended_at_utc, ms_played, dedupe_hash) VALUES ('export', 'x', 1, 'a')";
        assert!(c.execute(dup, []).is_err(), "dedupe_hash is unique");
        assert!(
            c.execute("INSERT INTO listening_event(source, ended_at_utc, ms_played, dedupe_hash) VALUES ('logger', 'x', 1, 'd')", [])
                .is_ok()
        );
        assert!(
            c.execute(
                "UPDATE listening_event SET reconciled_with = 2 WHERE id = 4",
                []
            )
            .is_err(),
            "an export event pairs with one logger event"
        );
        c.execute("DELETE FROM item WHERE id = 2", []).unwrap();
        let item: Option<i64> = c
            .query_row(
                "SELECT item_id FROM listening_event WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(item, None, "deleting an item keeps its plays");
    }

    /// 0006 rebuilds `item` to widen `added_via`: with data, nothing that points at items is lost
    /// (no cascade), search and the FTS triggers still work, 'inbox' is accepted.
    #[test]
    fn v5_with_data_upgrades_to_v6_keeping_everything() {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "foreign_keys", "ON").unwrap();
        for m in &MIGRATIONS[..5] {
            c.execute_batch(m).unwrap();
        }
        c.pragma_update(None, "user_version", 5).unwrap();
        fixture(&c, 6);
        c.execute_batch(
            "INSERT INTO listening_event(source, ended_at_utc, ms_played, dedupe_hash, item_id)
               VALUES ('export', '2026-01-01T00:00:00Z', 40000, 'v6', 2);",
        )
        .unwrap();
        let tables = [
            "item",
            "item_link",
            "item_tag",
            "field_provenance",
            "status_history",
            "item_fts",
            "listening_event WHERE item_id IS NOT NULL",
        ];
        let counts = |c: &Connection| -> Vec<i64> {
            tables
                .iter()
                .map(|t| {
                    c.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
                        .unwrap()
                })
                .collect()
        };
        let before = counts(&c);
        assert!(
            c.execute(
                "INSERT INTO item(type, added_via) VALUES ('track', 'inbox')",
                []
            )
            .is_err()
        );
        migrate(&mut c).unwrap();
        assert_eq!(version(&c), MIGRATIONS.len() as i64);
        assert_eq!(counts(&c), before);
        let fk: bool = c
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert!(fk, "foreign keys back on");
        c.execute(
            "INSERT INTO item(type, added_via) VALUES ('track', 'inbox')",
            [],
        )
        .unwrap();
        assert!(
            c.execute(
                "INSERT INTO item(type, added_via) VALUES ('track', 'bus')",
                []
            )
            .is_err()
        );
        c.execute(
            "UPDATE item SET display_name = 'Renamedzz' WHERE id = 2",
            [],
        )
        .unwrap();
        assert_eq!(search(&c, "Renamedzz"), vec![2]);
        c.execute("DELETE FROM item WHERE id = 2", []).unwrap(); // cascades work again
        assert_eq!(search(&c, "Renamedzz"), Vec::<i64>::new());
        let links: i64 = c
            .query_row(
                "SELECT count(*) FROM item_link WHERE from_id = 2 OR to_id = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(links, 0);
    }

    #[test]
    fn checks_reject_bad_status_and_rating() {
        let c = open_in_memory().unwrap();
        let bad_status = c.execute(
            "INSERT INTO item(type, status) VALUES ('album', 'done')",
            [],
        );
        assert!(bad_status.is_err());
        let bad_rating = c.execute("INSERT INTO item(type, rating) VALUES ('album', 0.7)", []);
        assert!(bad_rating.is_err());
        for ok in [0.5, 3.0, 4.5, 5.0] {
            c.execute("INSERT INTO item(type, rating) VALUES ('track', ?1)", [ok])
                .unwrap();
        }
        assert!(
            c.execute("INSERT INTO item(type, rating) VALUES ('album', 5.5)", [])
                .is_err()
        );
        assert!(
            c.execute("INSERT INTO item(type) VALUES ('episode')", [])
                .is_err()
        );
    }

    fn search(c: &Connection, q: &str) -> Vec<i64> {
        let mut st = c
            .prepare("SELECT rowid FROM item_fts WHERE item_fts MATCH ?1 ORDER BY rowid")
            .unwrap();
        st.query_map([q], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn fts_follows_reviews_tags_and_linked_artists() {
        let c = open_in_memory().unwrap();
        c.execute_batch(
            "INSERT INTO item(id, type, display_name) VALUES (1, 'artist', 'Zorblax');
             INSERT INTO item(id, type, display_name, review) VALUES (2, 'album', 'Night Songs', 'a luminous record');
             INSERT INTO tag(id, name) VALUES (1, 'ambient');
             INSERT INTO item_tag VALUES (2, 1);
             INSERT INTO item_link VALUES (2, 1, 'by_artist');",
        )
        .unwrap();
        assert_eq!(search(&c, "luminous"), vec![2]);
        assert_eq!(search(&c, "ambient"), vec![2]);
        assert_eq!(search(&c, "artists:zorblax"), vec![2]);

        c.execute("UPDATE tag SET name = 'drone' WHERE id = 1", [])
            .unwrap();
        assert!(search(&c, "ambient").is_empty());
        assert_eq!(search(&c, "drone"), vec![2]);

        c.execute("UPDATE item SET display_name = 'Quuxley' WHERE id = 1", [])
            .unwrap();
        assert!(search(&c, "zorblax").is_empty());
        assert_eq!(search(&c, "artists:quuxley"), vec![2]);

        c.execute("DELETE FROM item_link", []).unwrap();
        assert!(search(&c, "artists:quuxley").is_empty());
        c.execute("DELETE FROM tag WHERE id = 1", []).unwrap();
        assert!(search(&c, "drone").is_empty());
    }

    #[test]
    fn deleting_items_leaves_no_fts_rows() {
        let c = open_in_memory().unwrap();
        c.execute_batch(
            "INSERT INTO item(id, type, display_name) VALUES (1, 'artist', 'Zorblax');
             INSERT INTO item(id, type, display_name) VALUES (2, 'album', 'Night Songs');
             INSERT INTO tag(id, name) VALUES (1, 'ambient');
             INSERT INTO item_tag VALUES (2, 1);
             INSERT INTO item_link VALUES (2, 1, 'by_artist');
             DELETE FROM item WHERE id = 2;",
        )
        .unwrap();
        assert_eq!(search(&c, "night OR ambient"), Vec::<i64>::new());
        c.execute("DELETE FROM item WHERE id = 1", []).unwrap();
        let n: i64 = c
            .query_row("SELECT count(*) FROM item_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
}
