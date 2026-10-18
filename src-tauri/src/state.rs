//! `AppState`, managed by Tauri: the library connection, resolved paths and current settings.
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{AppHandle, Manager};

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
        let docs = p.document_dir().unwrap_or_else(|_| paths.data_dir.clone());
        let settings = settings::load(
            &paths.settings_file(),
            Settings::defaults(&paths.data_dir, &docs),
        );
        let db = crate::db::open(std::path::Path::new(&settings.library_path))?;
        Ok(Self {
            db: Mutex::new(db),
            paths,
            settings: Mutex::new(settings),
        })
    }
}
