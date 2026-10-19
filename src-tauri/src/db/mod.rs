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
];

/// Schema version a fully migrated library has.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

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
    conn.busy_timeout(Duration::from_secs(5))?;
    migrate(conn)
}

/// Run every migration newer than `user_version`, each in its own transaction.
pub fn migrate(conn: &mut Connection) -> Result<(), AppError> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current.max(0) as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
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
