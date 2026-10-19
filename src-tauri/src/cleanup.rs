//! Deleting data: `delete_library` (FR-022, SC-13f) and `runout.exe --uninstall-cleanup` (TSD §4.11, FR-024).
use std::path::Path;

use rusqlite::Connection;

use crate::error::{AppError, Result};

/// The phrase SC-13f asks for (`src/pages/Settings.tsx` DeleteLibraryDialog).
pub const CONFIRM: &str = "DELETE";
/// Credential Manager target holding the Spotify tokens (TSD §2). M5 must write exactly this target.
pub const CREDENTIAL: &str = "Runout/spotify";
pub const CACHE_DB: &str = "spotify_cache.db";
pub const WEBVIEW_DIR: &str = "EBWebView";

/// Empty every table in library.db (schema kept), then VACUUM and truncate the WAL so deleted
/// text doesn't linger in free pages or the WAL, and remove `library.db.pre-restore` (+ marker).
/// Backups are untouched (they live outside the app folders, D28). Refused while a failed restore's
/// marker exists: pre-restore may then be the real library, and start-up recovery puts it back first.
pub fn delete_library(conn: &mut Connection, library_path: &Path, confirm: &str) -> Result<()> {
    if confirm != CONFIRM {
        return Err(AppError::validation("Type DELETE in capitals to confirm."));
    }
    if crate::backup::marker(library_path).exists() {
        return Err(AppError::new(
            crate::error::Code::Conflict,
            "A restore didn't finish. Restart Runout first.",
        ));
    }
    let tables: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table'
               AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'item_fts_%'",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let tx = conn.transaction()?;
    for t in &tables {
        tx.execute(&format!("DELETE FROM \"{t}\""), [])?;
    }
    tx.commit()?;
    // merge FTS segments so no pre-secure-delete tombstoned text survives VACUUM (migration 0002)
    conn.execute("INSERT INTO item_fts(item_fts) VALUES('optimize')", [])?;
    conn.execute_batch("VACUUM")?;
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    crate::backup::remove_pre_restore(library_path)?;
    Ok(())
}

fn remove(p: &Path) -> std::io::Result<()> {
    let r = if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    };
    match r {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Always-run uninstall step: `spotify_cache.db` (+ WAL files), the `EBWebView` folder (both in
/// `%LOCALAPPDATA%\app.runout.desktop`) and the `Runout/spotify` credential. Missing = fine.
/// Every step runs; the first error is returned.
pub fn uninstall_cleanup(local_data_dir: &Path) -> Result<()> {
    cleanup(local_data_dir, CREDENTIAL)
}

/// Tests pass their own credential target so they never touch a real sign-in.
fn cleanup(local_data_dir: &Path, credential: &str) -> Result<()> {
    let mut first: Option<AppError> = None;
    for name in [
        CACHE_DB.to_string(),
        format!("{CACHE_DB}-wal"),
        format!("{CACHE_DB}-shm"),
        WEBVIEW_DIR.to_string(),
    ] {
        if let Err(e) = remove(&local_data_dir.join(name)) {
            first.get_or_insert(e.into());
        }
    }
    if let Err(e) = delete_credential(credential) {
        first.get_or_insert(e);
    }
    first.map_or(Ok(()), Err)
}

#[cfg(windows)]
fn delete_credential(target: &str) -> Result<()> {
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW};
    use windows::core::HSTRING;
    match unsafe { CredDeleteW(&HSTRING::from(target), CRED_TYPE_GENERIC, None) } {
        Err(e) if e.code() != ERROR_NOT_FOUND.to_hresult() => Err(AppError::new(
            crate::error::Code::Io,
            "Couldn't remove the saved Spotify sign-in.",
        )
        .with_detail(e.to_string())),
        _ => Ok(()),
    }
}

#[cfg(not(windows))]
fn delete_credential(_target: &str) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn delete_library_needs_the_phrase_and_empties_everything() {
        let d = std::env::temp_dir().join(format!("runout-delete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let lib = d.join("library.db");
        let mut c = db::open(&lib).unwrap();
        db::fixture(&c, 4);
        let pre = d.join("library.db.pre-restore");
        let leftovers = [
            pre.clone(),
            d.join("library.db.pre-restore-wal"),
            d.join("library.db.restore-failed"),
        ];
        for p in &leftovers {
            std::fs::write(p, b"old notes").unwrap();
        }
        // a failed restore's marker → refused, nothing deleted (restart recovers first, TSD §4.10)
        let e = delete_library(&mut c, &lib, "DELETE").unwrap_err();
        assert_eq!(e.code, crate::error::Code::Conflict);
        assert!(pre.exists());
        std::fs::remove_file(&leftovers[2]).unwrap();
        for wrong in ["delete", "DELETE ", "", "Delete"] {
            assert!(delete_library(&mut c, &lib, wrong).is_err(), "{wrong:?}");
        }
        assert!(pre.exists(), "wrong phrase deletes nothing");
        delete_library(&mut c, &lib, "DELETE").unwrap();
        for p in &leftovers {
            assert!(!p.exists(), "{p:?}");
        }
        let wal = std::fs::metadata(d.join("library.db-wal")).map_or(0, |m| m.len());
        assert_eq!(wal, 0, "WAL truncated: no deleted text left in it");
        for t in [
            "item",
            "tag",
            "item_tag",
            "item_link",
            "status_history",
            "saved_filter",
            "usage_event",
            "field_provenance",
            "item_fts",
        ] {
            let n: i64 = c
                .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{t}");
        }
        // schema intact: the library is usable straight away
        c.execute(
            "INSERT INTO item(type, display_name) VALUES ('album', 'Fresh')",
            [],
        )
        .unwrap();
        let v: i64 = c
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, db::SCHEMA_VERSION);
        drop(c);
        let _ = std::fs::remove_dir_all(d);
    }

    /// Deleted review text must not survive in the file: FTS5 deletes only add tombstones unless
    /// secure-delete is on, and VACUUM copies the old segments (TSD §4.11, FR-022).
    fn file_has(dir: &Path, needle: &[u8]) -> bool {
        ["library.db", "library.db-wal"].iter().any(|f| {
            std::fs::read(dir.join(f)).is_ok_and(|b| b.windows(needle.len()).any(|w| w == needle))
        })
    }

    fn lib_with_review(tag: &str) -> (std::path::PathBuf, Connection, i64) {
        let d = std::env::temp_dir().join(format!("runout-fts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let c = db::open(&d.join("library.db")).unwrap();
        let id = c
            .query_row(
                "INSERT INTO item(type, display_name, review) VALUES ('album', 'Kept', 'qzxvwsecret') RETURNING id",
                [],
                |r| r.get(0),
            )
            .unwrap();
        db::fixture(&c, 300); // later writes: the token sits in an old FTS segment
        c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .unwrap();
        assert!(file_has(&d, b"qzxvwsecret"), "precondition: token on disk");
        (d, c, id)
    }

    #[test]
    fn delete_library_leaves_no_review_tokens_on_disk() {
        let (d, mut c, _) = lib_with_review("lib");
        delete_library(&mut c, &d.join("library.db"), CONFIRM).unwrap();
        assert!(!file_has(&d, b"qzxvwsecret"));
        drop(c);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn delete_item_then_vacuum_leaves_no_review_tokens_on_disk() {
        let (d, c, id) = lib_with_review("item");
        crate::library::items::delete_item(&c, id).unwrap();
        c.execute_batch("VACUUM").unwrap();
        c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .unwrap();
        assert!(!file_has(&d, b"qzxvwsecret"));
        drop(c);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn uninstall_cleanup_removes_cache_and_webview_and_tolerates_missing() {
        let d = std::env::temp_dir().join(format!("runout-uninstall-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join(WEBVIEW_DIR).join("Default")).unwrap();
        std::fs::write(d.join(CACHE_DB), b"x").unwrap();
        std::fs::write(d.join(format!("{CACHE_DB}-wal")), b"x").unwrap();
        std::fs::write(d.join("library.db"), b"keep").unwrap();
        let target = "Runout/test-uninstall-cleanup";
        cleanup(&d, target).unwrap();
        assert!(!d.join(CACHE_DB).exists() && !d.join(WEBVIEW_DIR).exists());
        assert!(
            d.join("library.db").exists(),
            "library is the NSIS checkbox's job"
        );
        cleanup(&d, target).unwrap(); // again: everything missing is fine (incl. the credential)
        let _ = std::fs::remove_dir_all(d);
    }
}
