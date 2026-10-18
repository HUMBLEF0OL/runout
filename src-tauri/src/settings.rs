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

/// `update_settings`: `base` with the keys of `patch` laid over it; unknown keys, wrong types or
/// `library_path` (only `move_library` changes it) → `validation`.
pub fn merge(base: &Settings, patch: &Value) -> Result<Settings> {
    if patch.get("library_path").is_some() {
        return Err(AppError::validation(
            "Use Move library to change where the library lives.",
        ));
    }
    overlay(base, patch)
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
    serde_json::from_value(v).map_err(|_| AppError::validation("A setting has the wrong type."))
}

/// Read `path` over `defaults`. Missing or unreadable file → defaults (never blocks start-up).
pub fn load(path: &Path, defaults: Settings) -> Settings {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| overlay(&defaults, &v).ok())
        .unwrap_or(defaults)
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
        // only move_library may change library_path
        let e = merge(&d, &json!({"library_path": "x.db"})).unwrap_err();
        assert_eq!(e.code, crate::error::Code::Validation);

        let dir = std::env::temp_dir().join(format!("runout-settings-{}", std::process::id()));
        let path = dir.join(FILE);
        assert_eq!(load(&path, d.clone()), d);
        let moved = Settings {
            library_path: "elsewhere.db".into(),
            ..m.clone()
        };
        save(&path, &moved).unwrap();
        assert_eq!(
            load(&path, d.clone()),
            moved,
            "load keeps a moved library_path"
        );
        std::fs::write(&path, b"{corrupt").unwrap();
        assert_eq!(load(&path, d.clone()), d);
        let _ = std::fs::remove_dir_all(dir);
    }
}
