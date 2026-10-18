//! Backups (TSD §4.10, FR-021, D16): `VACUUM INTO` → zip (deflate) with `manifest.json` → keep newest 3
//! → size warning. Restore verifies sha256 + `integrity_check` + counts before swapping files, and
//! keeps the old file as `library.db.pre-restore` until the next successful start.
//! library.db holds no source-A data, so backups never contain Spotify-provided data.
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, Code, Result};

pub const KEEP: usize = 3;
const PREFIX: &str = "runout-backup-";
const DB_ENTRY: &str = "library.db";
const MANIFEST: &str = "manifest.json";
const WEEK: Duration = Duration::from_secs(7 * 24 * 3600);
const CHECK_EVERY: Duration = Duration::from_secs(24 * 3600);

/// Partial API.md `Counts`, as stored in the manifest and shown before a restore.
pub type Counts = BTreeMap<String, i64>;

/// API.md `Backup`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Backup {
    pub path: String,
    pub created_at: String,
    pub size_bytes: u64,
    pub item_count: i64,
    pub size_warning: bool,
}

/// API.md `RestoreCheck`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RestoreCheck {
    pub ok: bool,
    pub schema_version: i64,
    pub counts: Counts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    app_version: String,
    schema_version: i64,
    sha256: String,
    counts: Counts,
    created_at: String,
}

fn counts(conn: &Connection) -> Result<Counts> {
    let row: [i64; 5] = conn.query_row(
        "SELECT count(*), COALESCE(sum(type = 'album'), 0), COALESCE(sum(type = 'track'), 0),
                COALESCE(sum(type = 'artist'), 0), COALESCE(sum(in_backlog), 0) FROM item",
        [],
        |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?]),
    )?;
    Ok(["library", "albums", "tracks", "artists", "backlog"]
        .into_iter()
        .map(String::from)
        .zip(row)
        .collect())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn zip_err(e: zip::result::ZipError) -> AppError {
    AppError::new(Code::Io, "Couldn't write the backup.").with_detail(e.to_string())
}

fn integrity(message: &str, detail: impl ToString) -> AppError {
    AppError::new(Code::Integrity, message).with_detail(detail.to_string())
}

/// `<path>.<ext>`, e.g. `library.db.pre-restore`.
fn sibling(path: &Path, ext: &str) -> PathBuf {
    PathBuf::from(format!("{}.{ext}", path.display()))
}

/// `backup_now`: snapshot `conn` into `dir`, rotate, flag size (> `size_warn_mb`).
pub fn backup_now(conn: &Connection, dir: &Path, size_warn_mb: u32) -> Result<Backup> {
    backup_at(conn, dir, size_warn_mb, Local::now())
}

fn backup_at(
    conn: &Connection,
    dir: &Path,
    size_warn_mb: u32,
    now: DateTime<Local>,
) -> Result<Backup> {
    std::fs::create_dir_all(dir)?;
    let name = format!("{PREFIX}{}.zip", now.format("%Y%m%d-%H%M%S"));
    let path = dir.join(&name);
    let snap = dir.join(format!(".{name}.db"));
    let _ = std::fs::remove_file(&snap);
    conn.execute("VACUUM INTO ?1", [snap.to_string_lossy()])?;
    let result = (|| {
        // ponytail: whole snapshot in memory (NFR-11 estimate is tens of MB); stream if libraries grow.
        let bytes = std::fs::read(&snap)?;
        let (schema_version, counts) = {
            let c = Connection::open(&snap)?;
            let v: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0))?;
            (v, counts(&c)?)
        };
        let manifest = Manifest {
            app_version: env!("CARGO_PKG_VERSION").into(),
            schema_version,
            sha256: sha256_hex(&bytes),
            counts,
            created_at: now
                .with_timezone(&Utc)
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
        };
        let tmp = sibling(&path, "tmp");
        let mut z = zip::ZipWriter::new(File::create(&tmp)?);
        let opt = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        z.start_file(DB_ENTRY, opt).map_err(zip_err)?;
        z.write_all(&bytes)?;
        z.start_file(MANIFEST, opt).map_err(zip_err)?;
        z.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
        z.finish().map_err(zip_err)?.sync_all()?;
        std::fs::rename(&tmp, &path)?;
        Ok::<_, AppError>(manifest)
    })();
    let _ = std::fs::remove_file(&snap);
    let manifest = result?;
    rotate(dir)?;
    let size_bytes = std::fs::metadata(&path)?.len();
    Ok(Backup {
        path: path.to_string_lossy().into_owned(),
        created_at: manifest.created_at,
        size_bytes,
        item_count: manifest.counts.get("library").copied().unwrap_or(0),
        size_warning: size_bytes > u64::from(size_warn_mb) * 1024 * 1024,
    })
}

/// Backup zips in `dir`, newest first (the timestamped names sort by time).
fn zips(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(PREFIX) && n.ends_with(".zip"))
        })
        .collect();
    v.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    v
}

fn rotate(dir: &Path) -> Result<()> {
    for old in zips(dir).iter().skip(KEEP) {
        std::fs::remove_file(old)?;
    }
    // Leftovers of a backup that was killed mid-way (`.zip.tmp`, `.<name>.db` snapshot; NFR-05).
    // Callers hold the library lock, so no other backup is in flight in this process
    // (a second app instance could lose its in-flight .tmp; single-instance is M6).
    for e in std::fs::read_dir(dir)?.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if (n.starts_with(PREFIX) && n.ends_with(".zip.tmp"))
            || (n.starts_with(&format!(".{PREFIX}")) && n.ends_with(".zip.db"))
        {
            let _ = std::fs::remove_file(e.path());
        }
    }
    Ok(())
}

fn read_manifest(path: &Path) -> Result<Manifest> {
    let not_backup =
        |e: &dyn ToString| integrity("That file isn't a Runout backup.", e.to_string());
    let mut z = zip::ZipArchive::new(File::open(path)?).map_err(|e| not_backup(&e))?;
    let mut s = String::new();
    z.by_name(MANIFEST)
        .map_err(|e| not_backup(&e))?
        .read_to_string(&mut s)
        .map_err(|e| not_backup(&e))?;
    serde_json::from_str(&s).map_err(|e| not_backup(&e))
}

/// `list_backups`, newest first. Unreadable zips are skipped.
pub fn list_backups(dir: &Path, size_warn_mb: u32) -> Vec<Backup> {
    zips(dir)
        .into_iter()
        .filter_map(|p| {
            let m = read_manifest(&p).ok()?;
            let size_bytes = std::fs::metadata(&p).ok()?.len();
            Some(Backup {
                path: p.to_string_lossy().into_owned(),
                created_at: m.created_at,
                size_bytes,
                item_count: m.counts.get("library").copied().unwrap_or(0),
                size_warning: size_bytes > u64::from(size_warn_mb) * 1024 * 1024,
            })
        })
        .collect()
}

/// Extract the backup's DB to `out` and verify it: sha256, `integrity_check`, schema version, counts.
fn extract_verified(zip_path: &Path, out: &Path) -> Result<RestoreCheck> {
    let manifest = read_manifest(zip_path)?;
    let damaged = |d: &dyn ToString| integrity("The backup file is damaged.", d.to_string());
    let mut z = zip::ZipArchive::new(File::open(zip_path)?).map_err(|e| damaged(&e))?;
    let mut bytes = Vec::new();
    z.by_name(DB_ENTRY)
        .map_err(|e| damaged(&e))?
        .read_to_end(&mut bytes)
        .map_err(|e| damaged(&e))?;
    if sha256_hex(&bytes) != manifest.sha256 {
        return Err(damaged(&"sha256 mismatch"));
    }
    let _ = std::fs::remove_file(out);
    std::fs::write(out, &bytes)?;
    let c = Connection::open(out)?;
    let ok: String = c
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| damaged(&e))?;
    if ok != "ok" {
        return Err(damaged(&ok));
    }
    let schema_version: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if schema_version > crate::db::SCHEMA_VERSION {
        return Err(AppError::validation(
            "This backup was made by a newer Runout. Update Runout first.",
        ));
    }
    let counts = counts(&c).map_err(|e| damaged(&e.message))?;
    if counts != manifest.counts {
        return Err(damaged(&"counts differ from the manifest"));
    }
    Ok(RestoreCheck {
        ok: true,
        schema_version,
        counts,
        error: None,
    })
}

/// `check_backup`: verify without touching the library (extracts next to it, then deletes).
/// A damaged backup is an `integrity` error.
pub fn check_backup(zip_path: &Path, library_path: &Path) -> Result<RestoreCheck> {
    let staged = sibling(library_path, "check");
    let r = extract_verified(zip_path, &staged);
    let _ = std::fs::remove_file(&staged);
    r
}

/// Move `from` (+ any `-wal`/`-shm` left beside it) to `to`.
fn move_db(from: &Path, to: &Path) -> std::io::Result<()> {
    for side in ["-wal", "-shm"] {
        let (f, t) = (
            PathBuf::from(format!("{}{side}", from.display())),
            PathBuf::from(format!("{}{side}", to.display())),
        );
        let _ = std::fs::remove_file(&t);
        if f.exists() {
            std::fs::rename(f, t)?;
        }
    }
    std::fs::rename(from, to)
}

/// `restore_backup`: verify, then swap files under the AppState lock. The live connection is closed,
/// the old file becomes `library.db.pre-restore`, and the restored file is reopened (and migrated).
/// Any failure before the swap leaves the library untouched.
pub fn restore_backup(
    db: &Mutex<Connection>,
    library_path: &Path,
    zip_path: &Path,
) -> Result<RestoreCheck> {
    let staged = sibling(library_path, "restore");
    let check = match extract_verified(zip_path, &staged) {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_file(&staged);
            return Err(e);
        }
    };
    let pre = sibling(library_path, "pre-restore");
    let mut conn = db.lock().unwrap();
    let old = std::mem::replace(&mut *conn, Connection::open_in_memory()?);
    if let Err((old, e)) = old.close() {
        *conn = old;
        let _ = std::fs::remove_file(&staged);
        return Err(e.into());
    }
    let reopen_old = |conn: &mut Connection| {
        if !library_path.exists() {
            let _ = move_db(&pre, library_path);
        }
        if let Ok(c) = crate::db::open(library_path) {
            *conn = c;
        }
    };
    let _ = std::fs::remove_file(&pre);
    if let Err(e) = move_db(library_path, &pre).and_then(|_| std::fs::rename(&staged, library_path))
    {
        reopen_old(&mut conn);
        return Err(e.into());
    }
    match crate::db::open(library_path) {
        Ok(c) => *conn = c,
        Err(e) => {
            let _ = std::fs::remove_file(library_path);
            reopen_old(&mut conn);
            return Err(e);
        }
    }
    Ok(check)
}

/// At a successful start: the pre-restore copy is no longer needed (TSD §4.10).
pub fn cleanup_pre_restore(library_path: &Path) {
    let pre = sibling(library_path, "pre-restore");
    for side in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{side}", pre.display()));
    }
}

/// Weekly backup is due: no backup in `dir`, or the newest is ≥ 7 days old.
pub fn due(dir: &Path, now: SystemTime) -> bool {
    zips(dir)
        .first()
        .and_then(|p| std::fs::metadata(p).ok()?.modified().ok())
        .is_none_or(|t| now.duration_since(t).unwrap_or_default() >= WEEK)
}

/// Weekly scheduler thread (lib.rs spawns it): checks at start, then daily; emits `backup-finished`.
pub fn spawn_scheduler(app: tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    std::thread::spawn(move || {
        loop {
            let st = app.state::<crate::state::AppState>();
            let s = st.settings.lock().unwrap().clone();
            let dir = Path::new(&s.backup_dir);
            if s.backup_weekly && due(dir, SystemTime::now()) {
                let r = backup_now(&st.db.lock().unwrap(), dir, s.size_warn_mb);
                if let Ok(b) = r {
                    let _ = app.emit("backup-finished", &b);
                }
            }
            std::thread::sleep(CHECK_EVERY);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use chrono::TimeZone;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("runout-backup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn at(day: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, day, 9, 30, 0).unwrap()
    }
    fn n_items(c: &Connection) -> i64 {
        c.query_row("SELECT count(*) FROM item", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn rotation_x4_keeps_newest_3() {
        let d = tmp("rotate");
        let c = db::open_in_memory().unwrap();
        db::fixture(&c, 3);
        for day in 1..=4 {
            let b = backup_at(&c, &d.join("b"), 100, at(day)).unwrap();
            assert!(!b.size_warning);
            assert_eq!(b.item_count, 4);
        }
        let names: Vec<String> = list_backups(&d.join("b"), 100)
            .iter()
            .map(|b| {
                Path::new(&b.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(
            names,
            [
                "runout-backup-20261004-093000.zip",
                "runout-backup-20261003-093000.zip",
                "runout-backup-20261002-093000.zip"
            ]
        );
        assert_eq!(
            std::fs::read_dir(d.join("b")).unwrap().count(),
            3,
            "no temp files left"
        );
        assert!(backup_at(&c, &d.join("b"), 0, at(5)).unwrap().size_warning);
        assert!(!due(&d.join("b"), SystemTime::now()));
        assert!(due(&d.join("nothing"), SystemTime::now()));
        assert!(due(&d.join("b"), SystemTime::now() + WEEK));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn corrupted_zip_is_integrity_error_and_library_untouched() {
        let d = tmp("corrupt");
        let lib = d.join("library.db");
        let db = Mutex::new(db::open(&lib).unwrap());
        db::fixture(&db.lock().unwrap(), 4);
        let b = backup_at(&db.lock().unwrap(), &d, 100, at(1)).unwrap();
        let mut bytes = std::fs::read(&b.path).unwrap();
        let mid = bytes.len() / 3;
        for x in &mut bytes[mid..mid + 64] {
            *x ^= 0x5a;
        }
        let bad = d.join("runout-backup-bad.zip");
        std::fs::write(&bad, &bytes).unwrap();
        let notzip = d.join("notes.zip");
        std::fs::write(&notzip, b"not a zip at all").unwrap();
        for p in [&bad, &notzip] {
            assert_eq!(
                check_backup(p, &lib).unwrap_err().code,
                Code::Integrity,
                "{p:?}"
            );
            assert_eq!(
                restore_backup(&db, &lib, p).unwrap_err().code,
                Code::Integrity
            );
        }
        assert_eq!(n_items(&db.lock().unwrap()), 5);
        assert!(!sibling(&lib, "pre-restore").exists());
        assert!(!sibling(&lib, "restore").exists() && !sibling(&lib, "check").exists());
        assert!(check_backup(Path::new(&b.path), &lib).unwrap().ok);
        drop(db);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn restore_of_oldest_verifies_counts() {
        let d = tmp("restore");
        let lib = d.join("library.db");
        let bdir = d.join("Backups");
        let db = Mutex::new(db::open(&lib).unwrap());
        db::fixture(&db.lock().unwrap(), 2); // 3 items
        let mut made = Vec::new();
        for day in 1..=4 {
            let c = db.lock().unwrap();
            c.execute(
                "INSERT INTO item(type, display_name) VALUES ('album', 'More')",
                [],
            )
            .unwrap();
            made.push(backup_at(&c, &bdir, 100, at(day)).unwrap());
        }
        let oldest = list_backups(&bdir, 100).pop().unwrap();
        assert_eq!(oldest.path, made[1].path, "day 1 was rotated out");
        assert_eq!(oldest.item_count, 5);
        assert_eq!(n_items(&db.lock().unwrap()), 7);

        let check = check_backup(Path::new(&oldest.path), &lib).unwrap();
        assert_eq!(
            (check.ok, check.schema_version, check.counts["library"]),
            (true, db::SCHEMA_VERSION, 5)
        );
        let r = restore_backup(&db, &lib, Path::new(&oldest.path)).unwrap();
        assert_eq!(r.counts["library"], 5);
        assert_eq!(
            n_items(&db.lock().unwrap()),
            5,
            "live connection now reads the restored file"
        );
        let pre = sibling(&lib, "pre-restore");
        assert_eq!(
            n_items(&Connection::open(&pre).unwrap()),
            7,
            "old library kept"
        );
        // the swapped-in connection is a normal migrated library
        let fk: i64 = db
            .lock()
            .unwrap()
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        cleanup_pre_restore(&lib);
        assert!(!pre.exists());
        drop(db);
        let _ = std::fs::remove_dir_all(d);
    }
}
