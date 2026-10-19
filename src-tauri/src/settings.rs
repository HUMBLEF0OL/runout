//! App settings (API.md `Settings`), stored as `settings.json` in the app config dir, not in
//! library.db, because `library_path` itself is movable (D15).
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, Result};

pub const FILE: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub library_path: String,
    pub backup_dir: String,
    pub backup_weekly: bool,
    pub size_warn_mb: u32,
    pub artwork_default: bool,
}

impl Settings {
    /// Defaults (TSD §2 locations, §4.10 100 MB warning, §4.3 artwork off).
    pub fn defaults(data_dir: &Path, documents_dir: &Path) -> Self {
        Self {
            library_path: data_dir.join("library.db").to_string_lossy().into_owned(),
            backup_dir: documents_dir
                .join("Runout")
                .join("Backups")
                .to_string_lossy()
                .into_owned(),
            backup_weekly: true,
            size_warn_mb: 100,
            artwork_default: false,
        }
    }
}

/// `update_settings`: `base` with the keys of `patch` laid over it; unknown keys, wrong types or a
/// path the UI must not choose (API.md §1: only Rust's pickers `move_library` / `choose_backup_dir`
/// set `library_path` / `backup_dir`) → `validation`.
pub fn merge(base: &Settings, patch: &Value) -> Result<Settings> {
    for (key, what) in [
        ("library_path", "Use Change location to move the library."),
        ("backup_dir", "Use Change folder to pick where backups go."),
    ] {
        if patch.get(key).is_some() {
            return Err(AppError::validation(what));
        }
    }
    let s = overlay(base, patch)?;
    if s.size_warn_mb == 0 {
        return Err(AppError::validation(
            "The size warning must be at least 1 MB.",
        ));
    }
    Ok(s)
}

/// `base` with the keys of `patch` laid over it; unknown keys or wrong types → `validation`.
fn overlay(base: &Settings, patch: &Value) -> Result<Settings> {
    let Value::Object(patch) = patch else {
        return Err(AppError::validation("Settings must be an object."));
    };
    let mut v = serde_json::to_value(base)?;
    let obj = v.as_object_mut().expect("struct serializes to object");
    for (k, val) in patch {
        if !obj.contains_key(k) {
            return Err(AppError::validation(format!("Unknown setting {k}.")));
        }
        obj.insert(k.clone(), val.clone());
    }
    let s: Settings = serde_json::from_value(v)
        .map_err(|_| AppError::validation("A setting has the wrong type."))?;
    if s.size_warn_mb == 0 {
        return Err(AppError::validation(
            "The size warning must be at least 1 MB.",
        ));
    }
    Ok(s)
}

/// Read `path` over `defaults`, key by key: unknown keys and wrong-typed values are skipped, valid
/// ones kept. Missing file → defaults. A file that isn't a JSON object, or a `library_path` that
/// isn't a non-empty string → error, so start-up never opens a fresh library and forgets a moved
/// one (D15).
pub fn load(path: &Path, defaults: Settings) -> Result<Settings> {
    let bytes = match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(defaults),
        r => r?,
    };
    let bad = || {
        AppError::new(
            crate::error::Code::Integrity,
            "Runout's settings file is damaged, so it can't tell where your library is. Fix or remove settings.json, then start again.",
        )
        .with_detail(path.display().to_string())
    };
    let Ok(Value::Object(file)) = serde_json::from_slice::<Value>(&bytes) else {
        return Err(bad());
    };
    if file
        .get("library_path")
        .is_some_and(|v| !v.as_str().is_some_and(|p| !p.trim().is_empty()))
    {
        return Err(bad());
    }
    let mut s = defaults;
    for (k, v) in file {
        if let Ok(next) = overlay(&s, &Value::Object([(k, v)].into_iter().collect())) {
            s = next;
        }
    }
    Ok(s)
}

/// Write atomically (temp file + rename).
pub fn save(path: &Path, s: &Settings) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_load_save() {
        let d = Settings::defaults(Path::new("data"), Path::new("docs"));
        let m = merge(&d, &json!({"backup_weekly": false, "size_warn_mb": 50})).unwrap();
        assert!(!m.backup_weekly);
        assert_eq!(
            (m.size_warn_mb, m.library_path.as_str()),
            (50, d.library_path.as_str())
        );
        assert!(merge(&d, &json!({"nope": 1})).is_err());
        assert!(merge(&d, &json!({"size_warn_mb": "big"})).is_err());
        assert!(merge(&d, &json!({"size_warn_mb": 0})).is_err());
        // only move_library may change library_path
        // only the Rust pickers may change paths (API.md §1)
        for key in ["library_path", "backup_dir"] {
            let e = merge(&d, &json!({ key: "x" })).unwrap_err();
            assert_eq!(e.code, crate::error::Code::Validation, "{key}");
        }

        let dir = std::env::temp_dir().join(format!("runout-settings-{}", std::process::id()));
        let path = dir.join(FILE);
        assert_eq!(load(&path, d.clone()).unwrap(), d);
        let moved = Settings {
            library_path: "elsewhere.db".into(),
            ..m.clone()
        };
        save(&path, &moved).unwrap();
        assert_eq!(
            load(&path, d.clone()).unwrap(),
            moved,
            "load keeps a moved library_path"
        );
        // unknown key / wrong-typed other field: skipped, library_path kept
        std::fs::write(
            &path,
            br#"{"library_path": "elsewhere.db", "future_key": 1, "size_warn_mb": "big", "backup_weekly": false}"#,
        )
        .unwrap();
        let l = load(&path, d.clone()).unwrap();
        assert_eq!(l.library_path, "elsewhere.db");
        assert_eq!((l.size_warn_mb, l.backup_weekly), (d.size_warn_mb, false));
        // a hand-edited 0 is skipped like any invalid value (else every later update would fail)
        std::fs::write(
            &path,
            br#"{"library_path": "elsewhere.db", "size_warn_mb": 0}"#,
        )
        .unwrap();
        assert_eq!(load(&path, d.clone()).unwrap().size_warn_mb, d.size_warn_mb);
        // unreadable JSON or a bad library_path: error, never silent defaults
        for bad in [
            &b"{corrupt"[..],
            b"[]",
            br#"{"library_path": 5}"#,
            br#"{"library_path": ""}"#,
        ] {
            std::fs::write(&path, bad).unwrap();
            let e = load(&path, d.clone()).unwrap_err();
            assert_eq!(e.code, crate::error::Code::Integrity);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
