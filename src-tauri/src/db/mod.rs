//! library.db connection: WAL, foreign keys, busy timeout, migrations over `PRAGMA user_version` (TSD §1, §3).
use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use crate::error::AppError;

/// Embedded migrations; index + 1 = the `user_version` after it runs. Append only.
const MIGRATIONS: &[&str] = &[include_str!("../../migrations/0001_init.sql")];

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
    fn fresh_db_migrates_to_v1_and_rerun_is_noop() {
        let mut c = open_in_memory().unwrap();
        assert_eq!(version(&c), 1);
        c.execute(
            "INSERT INTO item(type, display_name) VALUES ('album', 'A')",
            [],
        )
        .unwrap();
        migrate(&mut c).unwrap();
        assert_eq!(version(&c), 1);
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
        let c = open(&path).unwrap(); // reopen: already at v1
        assert_eq!(version(&c), SCHEMA_VERSION);
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
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
