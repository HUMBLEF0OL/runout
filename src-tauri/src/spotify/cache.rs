//! `spotify_cache.db` (TSD §3): source-A data only, deleted as a file on disconnect. Own tiny
//! migration over `user_version`; cache entries live 7 days (O5), playlist snapshots 30 days (O4).
use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

use crate::error::Result;

pub const CACHE_DAYS: i64 = 7;
pub const SNAPSHOT_DAYS: i64 = 30;
/// Key of the `GET /me` entry (display name for `get_spotify_status`).
pub const ME: &str = "spotify:user:me";

/// Index + 1 = `user_version` after it runs. Append only.
const MIGRATIONS: &[&str] = &[
    "
CREATE TABLE cache_entry(
  uri TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  fetched_at TEXT NOT NULL,
  expires_at TEXT NOT NULL);
CREATE INDEX cache_entry_expires ON cache_entry(expires_at);
CREATE TABLE playlist_snapshot(
  id INTEGER PRIMARY KEY,
  playlist_uri TEXT NOT NULL,
  spotify_snapshot_id TEXT NOT NULL,
  taken_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  items_json TEXT NOT NULL);
CREATE INDEX playlist_snapshot_uri ON playlist_snapshot(playlist_uri, taken_at);
",
    "
ALTER TABLE playlist_snapshot ADD COLUMN applied_snapshot_id TEXT;
",
    "
ALTER TABLE playlist_snapshot ADD COLUMN pending_json TEXT;
",
    // 2026-10-20: undo checks track content, not snapshot ids (GET snapshot_id is stale after a
    // write, TSD §5.2). Older snapshots carry only ids, so they can't be checked and are dropped.
    "
DELETE FROM playlist_snapshot;
ALTER TABLE playlist_snapshot DROP COLUMN applied_snapshot_id;
ALTER TABLE playlist_snapshot RENAME COLUMN pending_json TO expected_json;
",
    // 2026-10-20 (audit r3): when Runout's last write had an unknown outcome, so undo can wait
    // for it to land instead of trusting one read of the list before it.
    "
ALTER TABLE playlist_snapshot ADD COLUMN unsure_at TEXT;
",
];

fn iso(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Open (creating) and migrate. Rollback journal, not WAL: no `-wal` file outlives a disconnect.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut c = Connection::open(path)?;
    c.busy_timeout(Duration::from_secs(5))?;
    c.pragma_update(None, "secure_delete", "ON")?;
    let v: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(v.max(0) as usize) {
        let tx = c.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(c)
}

/// Cached payload for `uri`, if not expired at `now`.
pub fn get(c: &Connection, uri: &str, now: DateTime<Utc>) -> Result<Option<Value>> {
    let s: Option<String> = c
        .query_row(
            "SELECT payload_json FROM cache_entry WHERE uri = ?1 AND expires_at > ?2",
            params![uri, iso(now)],
            |r| r.get(0),
        )
        .optional()?;
    Ok(s.map(|s| serde_json::from_str(&s)).transpose()?)
}

/// Insert or replace `uri` (expires [`CACHE_DAYS`] after `now`).
pub fn put(
    c: &Connection,
    uri: &str,
    kind: &str,
    payload: &Value,
    now: DateTime<Utc>,
) -> Result<()> {
    c.execute(
        "INSERT OR REPLACE INTO cache_entry(uri, kind, payload_json, fetched_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            uri,
            kind,
            payload.to_string(),
            iso(now),
            iso(now + chrono::Duration::days(CACHE_DAYS))
        ],
    )?;
    Ok(())
}

/// API.md `get_last_apply` shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub undo_id: i64,
    pub applied_at: String,
    pub expires_at: String,
}

/// Store the pre-apply playlist (30-day undo, O4); returns its id (`undo_id`). `spotify_snapshot_id`
/// is the draft's id, informational only. Until a write lands, the playlist should still hold
/// `items`, so that is the expected list.
pub fn put_snapshot(
    c: &Connection,
    playlist_uri: &str,
    spotify_snapshot_id: &str,
    items: &Value,
    now: DateTime<Utc>,
) -> Result<Snapshot> {
    let (taken, expires) = (iso(now), iso(now + chrono::Duration::days(SNAPSHOT_DAYS)));
    c.execute(
        "INSERT INTO playlist_snapshot(playlist_uri, spotify_snapshot_id, taken_at, expires_at, items_json, expected_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![playlist_uri, spotify_snapshot_id, taken, expires, items.to_string(), Value::Array(vec![items.clone()]).to_string()],
    )?;
    Ok(Snapshot {
        undo_id: c.last_insert_rowid(),
        applied_at: taken,
        expires_at: expires,
    })
}

/// Newest unexpired snapshot of `playlist_uri`: `(record, expected, items)`. `expected` is a JSON
/// array of the URI lists the playlist may hold after Runout's last write to it (one list, or the
/// before/after pair of a write with unknown outcome). Undo restores only when it holds one.
pub fn last_snapshot(
    c: &Connection,
    playlist_uri: &str,
    now: DateTime<Utc>,
) -> Result<Option<(Snapshot, Value, Value)>> {
    let row = c
        .query_row(
            "SELECT id, taken_at, expires_at, COALESCE(expected_json, '[]'), items_json
             FROM playlist_snapshot
             WHERE playlist_uri = ?1 AND expires_at > ?2 ORDER BY taken_at DESC, id DESC LIMIT 1",
            params![playlist_uri, iso(now)],
            |r| {
                Ok((
                    Snapshot {
                        undo_id: r.get(0)?,
                        applied_at: r.get(1)?,
                        expires_at: r.get(2)?,
                    },
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    row.map(|(s, e, j)| Ok((s, serde_json::from_str(&e)?, serde_json::from_str(&j)?)))
        .transpose()
}

/// Record the URI lists the playlist may now hold (see [`last_snapshot`]) after an apply/undo write,
/// and `unsure`: when that write failed with an unknown outcome (`None` once a write is answered).
pub fn set_expected(
    c: &Connection,
    undo_id: i64,
    lists: &Value,
    unsure: Option<DateTime<Utc>>,
) -> Result<()> {
    c.execute(
        "UPDATE playlist_snapshot SET expected_json = ?2, unsure_at = ?3 WHERE id = ?1",
        params![undo_id, lists.to_string(), unsure.map(iso)],
    )?;
    Ok(())
}

/// When snapshot `undo_id`'s last write failed with an unknown outcome, if it did.
pub fn unsure_since(c: &Connection, undo_id: i64) -> Result<Option<DateTime<Utc>>> {
    let s: Option<String> = c
        .query_row(
            "SELECT unsure_at FROM playlist_snapshot WHERE id = ?1",
            [undo_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(s.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|t| t.with_timezone(&Utc)))
}

/// One undo per playlist (as the mock): once snapshot `keep_id` stands for the playlist's latest
/// apply, older snapshots of it are dead (their expected list was overwritten by that apply).
pub fn drop_older_snapshots(c: &Connection, playlist_uri: &str, keep_id: i64) -> Result<()> {
    c.execute(
        "DELETE FROM playlist_snapshot WHERE playlist_uri = ?1 AND id <> ?2",
        params![playlist_uri, keep_id],
    )?;
    Ok(())
}

pub fn delete_snapshot(c: &Connection, undo_id: i64) -> Result<()> {
    c.execute("DELETE FROM playlist_snapshot WHERE id = ?1", [undo_id])?;
    Ok(())
}

/// Delete everything expired at `now`; returns rows removed.
pub fn sweep(c: &Connection, now: DateTime<Utc>) -> Result<usize> {
    let t = iso(now);
    Ok(
        c.execute("DELETE FROM cache_entry WHERE expires_at <= ?1", [&t])?
            + c.execute("DELETE FROM playlist_snapshot WHERE expires_at <= ?1", [&t])?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spotify::testing::*;
    use serde_json::json;

    #[test]
    fn expired_entries_are_gone_after_sweep() {
        let d = temp_dir("cache-sweep");
        let c = open(&d.join("c.db")).unwrap();
        let t0 = Utc::now();
        put(
            &c,
            "spotify:album:AAAAAAAAAAAAAAAAAAAAAA",
            "album",
            &json!({"name": "x"}),
            t0,
        )
        .unwrap();
        put_snapshot(
            &c,
            "spotify:playlist:PPPPPPPPPPPPPPPPPPPPPP",
            "s1",
            &json!([]),
            t0,
        )
        .unwrap();
        let day = chrono::Duration::days(1);
        assert!(
            get(&c, "spotify:album:AAAAAAAAAAAAAAAAAAAAAA", t0 + day * 6)
                .unwrap()
                .is_some()
        );
        assert_eq!(sweep(&c, t0 + day * 6).unwrap(), 0);
        // day 7: cache entry expired (unconfirmed data vanishes), snapshot still kept
        assert!(
            get(&c, "spotify:album:AAAAAAAAAAAAAAAAAAAAAA", t0 + day * 7)
                .unwrap()
                .is_none()
        );
        assert_eq!(sweep(&c, t0 + day * 7).unwrap(), 1);
        let n: i64 = c
            .query_row("SELECT count(*) FROM cache_entry", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "row gone, not just hidden");
        assert!(
            last_snapshot(&c, "spotify:playlist:PPPPPPPPPPPPPPPPPPPPPP", t0 + day * 29)
                .unwrap()
                .is_some()
        );
        assert_eq!(sweep(&c, t0 + day * 30).unwrap(), 1);
        assert!(
            last_snapshot(&c, "spotify:playlist:PPPPPPPPPPPPPPPPPPPPPP", t0)
                .unwrap()
                .is_none()
        );
        drop(c);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn last_snapshot_is_newest_and_reopen_keeps_schema() {
        let d = temp_dir("cache-snap");
        let p = d.join("c.db");
        let c = open(&p).unwrap();
        let t0 = Utc::now();
        let u = "spotify:playlist:PPPPPPPPPPPPPPPPPPPPPP";
        put_snapshot(&c, u, "s1", &json!(["a"]), t0).unwrap();
        let s2 = put_snapshot(
            &c,
            u,
            "s2",
            &json!(["b"]),
            t0 + chrono::Duration::seconds(5),
        )
        .unwrap();
        set_expected(&c, s2.undo_id, &json!([["w"]]), Some(t0)).unwrap();
        assert!(unsure_since(&c, s2.undo_id).unwrap().is_some());
        let (rec, exp, items) = last_snapshot(&c, u, t0).unwrap().unwrap();
        assert_eq!(
            (rec, exp, items),
            (s2.clone(), json!([["w"]]), json!(["b"]))
        );
        delete_snapshot(&c, s2.undo_id).unwrap();
        let (_, exp, items) = last_snapshot(&c, u, t0).unwrap().unwrap();
        assert_eq!((exp, items), (json!([["a"]]), json!(["a"]))); // nothing written: pre-apply list
        drop(c);
        let c = open(&p).unwrap(); // migration doesn't re-run
        assert_eq!(last_snapshot(&c, u, t0).unwrap().unwrap().2, json!(["a"]));
        drop(c);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn disconnect_leaves_no_file_and_no_tokens() {
        let d = temp_dir("cache-disconnect");
        let stub = Stub::new(&[]);
        let sp = connected(&d, &stub);
        sp.with_cache(|c| {
            put(
                c,
                ME,
                "me",
                &json!({"display_name": "Synthetic"}),
                Utc::now(),
            )
        })
        .unwrap();
        let file = d.join(crate::cleanup::CACHE_DB);
        assert!(file.exists());
        assert_eq!(
            sp.status_now().unwrap().display_name.as_deref(),
            Some("Synthetic")
        );
        block(sp.disconnect()).unwrap();
        let left: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(left.is_empty(), "left behind: {left:?}");
        assert!(sp.tokens().is_none());
        assert_eq!(
            sp.status_now().unwrap().state,
            crate::spotify::State::Disconnected
        );
        block(sp.disconnect()).unwrap(); // again: nothing there is fine
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn sweep_never_creates_the_file() {
        let d = temp_dir("cache-nofile");
        let sp = spotify(&d, &Stub::new(&[]));
        assert_eq!(sp.sweep().unwrap(), 0);
        assert!(!d.join(crate::cleanup::CACHE_DB).exists());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn backup_zip_contains_no_cache_content() {
        // the cache sits beside library.db's folder in %LOCALAPPDATA%; backup reads library.db only
        let d = temp_dir("cache-backup");
        let sp = connected(&d, &Stub::new(&[]));
        let marker = "qzxvw-cache-only-marker";
        sp.with_cache(|c| put(c, ME, "me", &json!({ "display_name": marker }), Utc::now()))
            .unwrap();
        let lib = crate::db::open(&d.join("library.db")).unwrap();
        crate::db::fixture(&lib, 3);
        let b = crate::backup::backup_now(&lib, &d.join("backups"), 100).unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&b.path).unwrap()).unwrap();
        for i in 0..z.len() {
            let mut f = z.by_index(i).unwrap();
            let name = f.name().unwrap().to_string();
            assert!(!name.contains("spotify_cache"), "{name}");
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut f, &mut bytes).unwrap();
            assert!(
                !bytes.windows(marker.len()).any(|w| w == marker.as_bytes()),
                "{name}"
            );
            assert!(
                !bytes.windows(10).any(|w| w == b"cache_entr"),
                "cache schema in {name}"
            );
        }
        drop(lib);
        drop(sp);
        let _ = std::fs::remove_dir_all(d);
    }
}
