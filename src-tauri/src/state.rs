//! `AppState`, managed by Tauri: the library connection, resolved paths and current settings.
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{AppHandle, Manager};

use crate::backup;
use crate::error::Result;
use crate::settings::{self, Settings};

/// Folders from Tauri's path API (TSD §2), never hard-coded.
#[derive(Debug, Clone)]
pub struct Paths {
    /// `%APPDATA%\app.runout.desktop` (settings.json).
    pub config_dir: PathBuf,
    /// `%APPDATA%\app.runout.desktop` (default library.db).
    pub data_dir: PathBuf,
    /// `%LOCALAPPDATA%\app.runout.desktop` (spotify_cache.db, EBWebView).
    pub local_data_dir: PathBuf,
}

impl Paths {
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join(settings::FILE)
    }
}

pub struct AppState {
    /// library.db. Lock briefly; never hold across an `.await` (see `links::retry_unresolved`).
    pub db: Mutex<Connection>,
    pub paths: Paths,
    pub settings: Mutex<Settings>,
}

impl AppState {
    /// Resolve paths, load settings, open + migrate library.db.
    pub fn init(app: &AppHandle) -> Result<Self> {
        let p = app.path();
        let paths = Paths {
            config_dir: p.app_config_dir()?,
            data_dir: p.app_data_dir()?,
            local_data_dir: p.app_local_data_dir()?,
        };
        // Backups never default under app data: the uninstaller's "delete my library" removes it (TSD §4.11).
        let docs = p.document_dir().or_else(|_| p.home_dir())?;
        let settings = settings::load(
            &paths.settings_file(),
            Settings::defaults(&paths.data_dir, &docs),
        )?;
        recover_library(std::path::Path::new(&settings.library_path));
        let db = crate::db::open(std::path::Path::new(&settings.library_path))?;
        Ok(Self {
            db: Mutex::new(db),
            paths,
            settings: Mutex::new(settings),
        })
    }
}

/// Start-up recovery (TSD §4.10), before library.db is opened: a restore that failed half-way leaves
/// a marker (`backup::marker`). While it exists, `library.db.pre-restore` IS the user's library
/// (library.db may be the half-restored file), so it goes back in place whatever library.db holds,
/// then the marker is removed. Returns `true` when it put pre-restore back.
pub fn recover_library(library: &std::path::Path) -> bool {
    let (pre, marker) = (backup::pre_restore(library), backup::marker(library));
    if !marker.exists() {
        return false;
    }
    let moved = pre.exists();
    if moved && backup::move_db(&pre, library).is_err() {
        return false; // marker stays: next start tries again
    }
    let _ = std::fs::remove_file(marker);
    moved
}

/// Library file name inside the folder `move_library` picks.
pub const LIBRARY_FILE: &str = "library.db";

impl AppState {
    /// `move_library` (D15): copy library.db into `dir` with `VACUUM INTO` under the db lock, open the copy,
    /// check it holds the same items, save `settings.library_path`, then swap connections and delete the old
    /// file. Any failure before the swap leaves the old library live and removes the partial copy.
    pub fn move_library(&self, dir: &std::path::Path) -> Result<String> {
        use crate::error::AppError;
        let to = dir.join(LIBRARY_FILE);
        let mut conn = self.db.lock().unwrap();
        let from = PathBuf::from(&self.settings.lock().unwrap().library_path);
        if backup::marker(&from).exists() {
            return Err(AppError::new(
                crate::error::Code::Conflict,
                "A restore didn't finish. Restart Runout before moving the library.",
            ));
        }
        if to == from {
            return Ok(from.to_string_lossy().into_owned());
        }
        if to.exists() {
            return Err(AppError::validation(
                "That folder already has a library.db. Choose an empty folder.",
            ));
        }
        let count =
            |c: &Connection| c.query_row("SELECT count(*) FROM item", [], |r| r.get::<_, i64>(0));
        let new = (|| -> Result<Connection> {
            std::fs::create_dir_all(dir)?;
            conn.execute("VACUUM INTO ?1", [to.to_string_lossy()])?;
            let new = crate::db::open(&to)?;
            if count(&new)? != count(&conn)? {
                return Err(AppError::new(
                    crate::error::Code::Integrity,
                    "The moved library didn't match.",
                ));
            }
            let mut s = self.settings.lock().unwrap().clone();
            s.library_path = to.to_string_lossy().into_owned();
            settings::save(&self.paths.settings_file(), &s)?;
            *self.settings.lock().unwrap() = s;
            Ok(new)
        })();
        let new = match new {
            Ok(c) => c,
            Err(e) => {
                for side in ["", "-wal", "-shm"] {
                    let _ = std::fs::remove_file(format!("{}{side}", to.display()));
                }
                return Err(e);
            }
        };
        drop(std::mem::replace(&mut *conn, new)); // closes the old file
        for side in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{side}", from.display()));
        }
        // TSD §4.10 keeps pre-restore until the next successful start: it travels with the library.
        let pre = backup::pre_restore(&from);
        if pre.exists() && backup::move_db(&pre, &backup::pre_restore(&to)).is_err() {
            // rename fails across drives: copy it over and remove the old one only once the copy exists
            // (a failed copy leaves it at the old place rather than losing the only safety copy)
            if std::fs::copy(&pre, backup::pre_restore(&to)).is_ok() {
                let _ = backup::remove_pre_restore(&from);
            }
        }
        Ok(to.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recover_library_puts_pre_restore_back_only_after_a_failed_restore() {
        let d = std::env::temp_dir().join(format!("runout-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let lib = d.join(LIBRARY_FILE);
        let (pre, marker) = (
            d.join("library.db.pre-restore"),
            d.join("library.db.restore-failed"),
        );
        let good = crate::db::open(&pre).unwrap();
        crate::db::fixture(&good, 2);
        drop(good);
        assert!(!recover_library(&lib), "no marker → nothing to do");
        std::fs::write(&marker, b"").unwrap();
        assert!(
            recover_library(&lib),
            "missing library + marker → recovered"
        );
        assert!(lib.exists() && !pre.exists() && !marker.exists());
        let n: i64 = crate::db::open(&lib)
            .unwrap()
            .query_row("SELECT count(*) FROM item", [], |r| r.get(0))
            .unwrap();
        assert!(n > 0);
        // damaged library + marker → recovered; healthy library + marker → left alone
        std::fs::rename(&lib, &pre).unwrap();
        std::fs::write(&lib, b"not a database").unwrap();
        std::fs::write(&marker, b"").unwrap();
        assert!(recover_library(&lib));
        // healthy (half-restored) library + marker → pre-restore still wins
        for side in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{side}", lib.display()));
        }
        drop(crate::db::open(&lib).unwrap()); // empty but passes integrity_check
        let good = crate::db::open(&pre).unwrap();
        crate::db::fixture(&good, 2);
        let want: i64 = good
            .query_row("SELECT count(*) FROM item", [], |r| r.get(0))
            .unwrap();
        drop(good);
        std::fs::write(&marker, b"").unwrap();
        assert!(recover_library(&lib), "marker → pre-restore goes back");
        assert!(!pre.exists() && !marker.exists());
        let n: i64 = crate::db::open(&lib)
            .unwrap()
            .query_row("SELECT count(*) FROM item", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, want);
        // marker without pre-restore → marker cleared
        std::fs::write(&marker, b"").unwrap();
        assert!(!recover_library(&lib) && !marker.exists());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn move_library_copies_switches_and_refuses_occupied_folder() {
        let d = std::env::temp_dir().join(format!("runout-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let old = d.join("a").join(LIBRARY_FILE);
        let db = crate::db::open(&old).unwrap();
        crate::db::fixture(&db, 3);
        let paths = Paths {
            config_dir: d.join("cfg"),
            data_dir: d.join("a"),
            local_data_dir: d.join("local"),
        };
        let mut s = Settings::defaults(&paths.data_dir, &d);
        s.library_path = old.to_string_lossy().into_owned();
        let st = AppState {
            db: Mutex::new(db),
            paths,
            settings: Mutex::new(s),
        };
        let n = |st: &AppState| -> i64 {
            st.db
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM item", [], |r| r.get(0))
                .unwrap()
        };
        let before = n(&st);
        // a failed restore's marker → refused, nothing moves
        let marker = backup::marker(&old);
        std::fs::write(&marker, b"").unwrap();
        let e = st.move_library(&d.join("b")).unwrap_err();
        assert_eq!(e.code, crate::error::Code::Conflict);
        assert!(old.exists() && !d.join("b").exists());
        std::fs::remove_file(&marker).unwrap();
        // a leftover pre-restore moves with the library (kept until next successful start, TSD §4.10)
        std::fs::write(backup::pre_restore(&old), b"old").unwrap();
        let new = st.move_library(&d.join("b")).unwrap();
        assert!(!backup::pre_restore(&old).exists(), "not left behind");
        assert_eq!(
            std::fs::read(backup::pre_restore(std::path::Path::new(&new))).unwrap(),
            b"old",
            "pre-restore moved beside the new library"
        );
        assert_eq!(PathBuf::from(&new), d.join("b").join(LIBRARY_FILE));
        assert_eq!(n(&st), before, "live connection is the moved file");
        assert!(!old.exists(), "old file removed after success");
        let saved = settings::load(&st.paths.settings_file(), Settings::defaults(&d, &d)).unwrap();
        assert_eq!(saved.library_path, new);
        // occupied target → validation, nothing changes
        std::fs::create_dir_all(d.join("c")).unwrap();
        std::fs::write(d.join("c").join(LIBRARY_FILE), b"x").unwrap();
        let e = st.move_library(&d.join("c")).unwrap_err();
        assert_eq!(e.code, crate::error::Code::Validation);
        assert_eq!(st.settings.lock().unwrap().library_path, new);
        drop(st);
        let _ = std::fs::remove_dir_all(d);
    }
}
