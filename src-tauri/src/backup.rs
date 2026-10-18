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

use chrono::{DateTime, NaiveDateTime, Utc};
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

/// Marker beside the library: a restore failed half-way and `library.db.pre-restore` may hold the
/// only good copy, so it must not be cleaned up.
fn marker(library_path: &Path) -> PathBuf {
    sibling(library_path, "restore-failed")
}

#[cfg(test)]
thread_local! {
    /// Tests: renames *from* these paths fail, to exercise restore's recovery paths.
    static FAIL_RENAME_FROM: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if FAIL_RENAME_FROM.with(|f| f.borrow().iter().any(|p| p == from)) {
        return Err(std::io::Error::other("injected rename failure"));
    }
    std::fs::rename(from, to)
}

/// Write `tmp` with `write`, then rename it to `path`. `tmp` is removed on every error path.
pub(crate) fn write_then_rename(
    tmp: &Path,
    path: &Path,
    write: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let r = write(tmp).and_then(|_| Ok(rename(tmp, path)?));
    if r.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
    r
}

/// UTC stamp and same-second sequence of a backup file name: `runout-backup-YYYYMMDD-HHMMSS.zip`,
/// or `…-HHMMSS-N.zip` (N ≥ 2) for the N-th backup in one second. Anything else is not a backup.
fn parse_name(name: &str) -> Option<(DateTime<Utc>, u32)> {
    let rest = name.strip_prefix(PREFIX)?.strip_suffix(".zip")?;
    let stamp = rest.get(..15)?;
    let seq = match &rest[15..] {
        "" => 1,
        s => s
            .strip_prefix('-')
            .filter(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))?
            .parse::<u32>()
            .ok()
            .filter(|n| *n >= 2)?,
    };
    let shape = stamp.bytes().enumerate().all(|(i, c)| {
        if i == 8 {
            c == b'-'
        } else {
            c.is_ascii_digit()
        }
    });
    if !shape {
        return None;
    }
    let t = NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S").ok()?;
    Some((t.and_utc(), seq))
}

/// `backup_now`: snapshot `conn` into `dir`, rotate, flag size (> `size_warn_mb`).
pub fn backup_now(conn: &Connection, dir: &Path, size_warn_mb: u32) -> Result<Backup> {
    backup_at(conn, dir, size_warn_mb, Utc::now())
}

fn backup_at(
    conn: &Connection,
    dir: &Path,
    size_warn_mb: u32,
    now: DateTime<Utc>,
) -> Result<Backup> {
    std::fs::create_dir_all(dir)?;
    // Names are UTC so a DST or time-zone change can't reorder them.
    let stamp = now.format("%Y%m%d-%H%M%S");
    let path = (1..)
        .map(|n| match n {
            1 => dir.join(format!("{PREFIX}{stamp}.zip")),
            n => dir.join(format!("{PREFIX}{stamp}-{n}.zip")),
        })
        .find(|p| !p.exists())
        .expect("a free name");
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
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
            created_at: now.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        };
        write_then_rename(&sibling(&path, "tmp"), &path, |tmp| {
            let mut z = zip::ZipWriter::new(File::create(tmp)?);
            let opt = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            z.start_file(DB_ENTRY, opt).map_err(zip_err)?;
            z.write_all(&bytes)?;
            z.start_file(MANIFEST, opt).map_err(zip_err)?;
            z.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
            z.finish().map_err(zip_err)?.sync_all()?;
            Ok(())
        })?;
        Ok::<_, AppError>(manifest)
    })();
    let _ = std::fs::remove_file(&snap);
    let manifest = result?;
    rotate(dir, &path)?;
    let size_bytes = std::fs::metadata(&path)?.len();
    Ok(Backup {
        path: path.to_string_lossy().into_owned(),
        created_at: manifest.created_at,
        size_bytes,
        item_count: manifest.counts.get("library").copied().unwrap_or(0),
        size_warning: size_bytes > u64::from(size_warn_mb) * 1024 * 1024,
    })
}

/// Runout backup files in `dir` with their UTC stamp, newest first (by parsed stamp, then
/// same-second sequence). Other files, even `runout-backup-old.zip`, are never listed or rotated.
fn zips(dir: &Path) -> Vec<(PathBuf, DateTime<Utc>)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<(PathBuf, (DateTime<Utc>, u32))> = rd
        .filter_map(|e| {
            let e = e.ok()?;
            let key = parse_name(e.file_name().to_str()?)?;
            e.file_type().ok()?.is_file().then(|| (e.path(), key))
        })
        .collect();
    v.sort_by_key(|a| std::cmp::Reverse(a.1));
    v.into_iter().map(|(p, (t, _))| (p, t)).collect()
}

/// Keep `just_made` (even if the clock went back) plus the newest `KEEP - 1` others.
fn rotate(dir: &Path, just_made: &Path) -> Result<()> {
    let others = zips(dir).into_iter().filter(|(p, _)| p != just_made);
    for (old, _) in others.skip(KEEP - 1) {
        std::fs::remove_file(old)?;
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
        .filter_map(|(p, _)| {
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
            rename(&f, &t)?;
        }
    }
    rename(from, to)
}

fn remove_db_files(path: &Path) {
    for side in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{side}", path.display()));
    }
}

/// After a failed swap, put the old library back into `conn`. Never opens a missing path (that
/// would create an empty library). If the old file can't be moved back it is opened in place as
/// `pre-restore`, and the marker stops `cleanup_pre_restore` from deleting it.
fn reopen_old(conn: &mut Connection, library_path: &Path, pre: &Path) -> Result<()> {
    let keep_pre = || std::fs::write(marker(library_path), b"");
    let target = if pre.exists() {
        if !library_path.exists() && move_db(pre, library_path).is_ok() {
            library_path
        } else {
            // Couldn't move back, or library.db is the restored file we couldn't remove.
            let _ = keep_pre();
            pre
        }
    } else if library_path.exists() {
        library_path // nothing was moved
    } else {
        return Err(reopen_failed("no library file left"));
    };
    match crate::db::open(target) {
        Ok(c) => {
            *conn = c;
            Ok(())
        }
        Err(e) => {
            let _ = keep_pre();
            Err(reopen_failed(e.message))
        }
    }
}

fn reopen_failed(detail: impl Into<String>) -> AppError {
    AppError::new(
        Code::Io,
        "Restore failed and the library couldn't be reopened. Restart Runout; nothing was deleted.",
    )
    .with_detail(detail)
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
    if marker(library_path).exists() {
        let _ = std::fs::remove_file(&staged);
        return Err(AppError::new(
            Code::Conflict,
            "A previous restore didn't finish. Restart Runout before restoring again.",
        ));
    }
    let mut conn = db.lock().unwrap();
    let old = std::mem::replace(&mut *conn, Connection::open_in_memory()?);
    if let Err((old, e)) = old.close() {
        *conn = old;
        let _ = std::fs::remove_file(&staged);
        return Err(e.into());
    }
    remove_db_files(&pre);
    if let Err(e) = move_db(library_path, &pre).and_then(|_| rename(&staged, library_path)) {
        let _ = std::fs::remove_file(&staged);
        reopen_old(&mut conn, library_path, &pre)?;
        return Err(e.into());
    }
    match crate::db::open(library_path) {
        Ok(c) => *conn = c,
        Err(e) => {
            let _ = std::fs::remove_file(library_path);
            reopen_old(&mut conn, library_path, &pre)?;
            return Err(e);
        }
    }
    Ok(check)
}

/// At a successful start the pre-restore copy is no longer needed (TSD §4.10) — but it is only
/// deleted when library.db opens and passes `integrity_check` and no failed restore left a marker.
pub fn cleanup_pre_restore(library_path: &Path) {
    let pre = sibling(library_path, "pre-restore");
    if !pre.exists() || marker(library_path).exists() {
        return;
    }
    let healthy =
        Connection::open_with_flags(library_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .and_then(|c| c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)))
            .is_ok_and(|ok| ok == "ok");
    if healthy {
        remove_db_files(&pre);
    }
}

/// `delete_library` (cleanup.rs): the pre-restore copy and its marker go too.
pub(crate) fn remove_pre_restore(library_path: &Path) -> std::io::Result<()> {
    remove_db_files(&sibling(library_path, "pre-restore"));
    match std::fs::remove_file(marker(library_path)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Weekly backup is due: no backup in `dir`, or the newest (by its UTC name stamp) is ≥ 7 days old.
pub fn due(dir: &Path, now: SystemTime) -> bool {
    zips(dir)
        .first()
        .is_none_or(|(_, t)| now.duration_since(SystemTime::from(*t)).unwrap_or_default() >= WEEK)
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
    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, 9, 30, 0).unwrap()
    }
    fn name(p: &str) -> String {
        Path::new(p)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn names_are_strict_utc_stamps() {
        let t = at(4);
        assert_eq!(
            parse_name("runout-backup-20261004-093000.zip"),
            Some((t, 1))
        );
        assert_eq!(
            parse_name("runout-backup-20261004-093000-3.zip"),
            Some((t, 3))
        );
        for bad in [
            "runout-backup-old.zip",
            "runout-backup-bad.zip",
            "runout-backup-2026100-093000.zip",
            "runout-backup-20261004-093000-.zip",
            "runout-backup-20261004-093000-1.zip",
            "runout-backup-20261004-093000-x.zip",
            "runout-backup-20261004-093000.zip.tmp",
            "runout-backup-20261304-093000.zip",
            "runout-backup-20261004+093000.zip",
        ] {
            assert_eq!(parse_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn same_second_backups_get_a_suffix_and_sort_newest_first() {
        let d = tmp("same-second");
        let c = db::open_in_memory().unwrap();
        // a foreign file that sorts after every stamp by name must be ignored, not kept as "newest"
        std::fs::write(d.join("runout-backup-old.zip"), b"x").unwrap();
        for _ in 0..3 {
            backup_at(&c, &d, 100, at(4)).unwrap();
        }
        let names = || -> Vec<String> {
            list_backups(&d, 100)
                .iter()
                .map(|b| name(&b.path))
                .collect()
        };
        assert_eq!(
            names(),
            [
                "runout-backup-20261004-093000-3.zip",
                "runout-backup-20261004-093000-2.zip",
                "runout-backup-20261004-093000.zip",
            ]
        );
        // clock set back: the new (older-stamped) backup is kept, the oldest other one goes
        backup_at(&c, &d, 100, at(3)).unwrap();
        assert_eq!(
            names(),
            [
                "runout-backup-20261004-093000-3.zip",
                "runout-backup-20261004-093000-2.zip",
                "runout-backup-20261003-093000.zip",
            ]
        );
        assert!(
            d.join("runout-backup-old.zip").exists(),
            "foreign file untouched"
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn failed_write_leaves_no_tmp() {
        let d = tmp("tmp-cleanup");
        let tmp_file = d.join("x.tmp");
        // write fails after creating the temp file
        let r = write_then_rename(&tmp_file, &d.join("x"), |t| {
            std::fs::write(t, b"half")?;
            Err(AppError::internal("boom"))
        });
        assert!(r.is_err() && !tmp_file.exists());
        // rename fails: the target is a directory
        std::fs::create_dir(d.join("dir")).unwrap();
        let r = write_then_rename(&tmp_file, &d.join("dir"), |t| Ok(std::fs::write(t, b"x")?));
        assert!(r.is_err() && !tmp_file.exists());
        let _ = std::fs::remove_dir_all(d);
    }

    fn fail_renames_from(paths: &[&Path]) {
        FAIL_RENAME_FROM
            .with(|f| *f.borrow_mut() = paths.iter().map(|p| p.to_path_buf()).collect());
    }

    /// Swap fails after library.db was moved aside; moving it back works → library as before.
    #[test]
    fn failed_swap_moves_the_old_library_back() {
        let d = tmp("swap-back");
        let lib = d.join("library.db");
        let db = Mutex::new(db::open(&lib).unwrap());
        db::fixture(&db.lock().unwrap(), 2);
        let b = backup_at(&db.lock().unwrap(), &d.join("B"), 100, at(1)).unwrap();
        db.lock()
            .unwrap()
            .execute(
                "INSERT INTO item(type, display_name) VALUES ('album', 'New')",
                [],
            )
            .unwrap();
        fail_renames_from(&[&sibling(&lib, "restore")]);
        assert!(restore_backup(&db, &lib, Path::new(&b.path)).is_err());
        fail_renames_from(&[]);
        assert_eq!(n_items(&db.lock().unwrap()), 4, "old library live again");
        assert!(!marker(&lib).exists() && !sibling(&lib, "restore").exists());
        drop(db);
        assert_eq!(n_items(&db::open(&lib).unwrap()), 4, "and on disk");
        let _ = std::fs::remove_dir_all(d);
    }

    /// Swap fails and moving back fails too: the old library is opened in place as pre-restore,
    /// a marker is left, and the next start keeps pre-restore. No empty library.db is created.
    #[test]
    fn failed_swap_and_move_back_keeps_pre_restore() {
        let d = tmp("swap-stuck");
        let lib = d.join("library.db");
        let pre = sibling(&lib, "pre-restore");
        let db = Mutex::new(db::open(&lib).unwrap());
        db::fixture(&db.lock().unwrap(), 2);
        let b = backup_at(&db.lock().unwrap(), &d.join("B"), 100, at(1)).unwrap();
        fail_renames_from(&[&sibling(&lib, "restore"), &pre]);
        assert!(restore_backup(&db, &lib, Path::new(&b.path)).is_err());
        fail_renames_from(&[]);
        assert_eq!(
            n_items(&db.lock().unwrap()),
            3,
            "library data still reachable"
        );
        assert!(!lib.exists(), "no empty library.db was created");
        assert!(marker(&lib).exists());
        // a second restore is refused until the stuck one is sorted out
        assert_eq!(
            restore_backup(&db, &lib, Path::new(&b.path))
                .unwrap_err()
                .code,
            Code::Conflict
        );
        assert_eq!(n_items(&db.lock().unwrap()), 3);
        drop(db);
        db::open(&lib).unwrap(); // next start: AppState opens (and creates) library.db
        cleanup_pre_restore(&lib);
        assert!(pre.exists(), "marker keeps pre-restore");
        assert_eq!(n_items(&Connection::open(&pre).unwrap()), 3);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn cleanup_keeps_pre_restore_unless_library_is_healthy() {
        let d = tmp("cleanup");
        let lib = d.join("library.db");
        let pre = sibling(&lib, "pre-restore");
        std::fs::write(&pre, b"old").unwrap();
        cleanup_pre_restore(&lib); // library.db missing
        assert!(pre.exists());
        std::fs::write(&lib, vec![0x42; 8192]).unwrap(); // not a database
        cleanup_pre_restore(&lib);
        assert!(pre.exists());
        std::fs::remove_file(&lib).unwrap();
        drop(db::open(&lib).unwrap());
        cleanup_pre_restore(&lib);
        assert!(!pre.exists(), "healthy library, no marker → deleted");
        let _ = std::fs::remove_dir_all(d);
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
        let t5 = SystemTime::from(at(5));
        assert!(!due(&d.join("b"), t5 + WEEK - Duration::from_secs(1)));
        assert!(due(&d.join("nothing"), t5));
        assert!(due(&d.join("b"), t5 + WEEK));
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
