//! `export_library` (FR-020): the whole library as one JSON file with provenance. Format documented
//! in `docs/EXPORT_FORMAT.md` (a test keeps them in step). `usage_event` is never exported (TSD §4.12).
use std::path::Path;

use rusqlite::Connection;
use rusqlite::types::ValueRef;
use serde_json::{Map, Value, json};

use crate::error::Result;

pub const FORMAT_VERSION: i64 = 1;

/// Every row of `sql` as a JSON object keyed by column name; `json_cols` hold JSON text to inline.
fn rows(conn: &Connection, sql: &str, json_cols: &[&str]) -> Result<Vec<Value>> {
    let mut st = conn.prepare(sql)?;
    let names: Vec<String> = st.column_names().into_iter().map(String::from).collect();
    let mut out = Vec::new();
    let mut q = st.query([])?;
    while let Some(r) = q.next()? {
        let mut m = Map::new();
        for (i, name) in names.iter().enumerate() {
            let v = match r.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(f) => json!(f),
                ValueRef::Text(t) if json_cols.contains(&name.as_str()) => {
                    serde_json::from_slice(t)?
                }
                ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
                ValueRef::Blob(_) => Value::Null, // no blob columns in library.db
            };
            m.insert(name.clone(), v);
        }
        out.push(Value::Object(m));
    }
    Ok(out)
}

/// The export document (see `docs/EXPORT_FORMAT.md`).
pub fn build(conn: &Connection) -> Result<Value> {
    let items = rows(
        conn,
        "SELECT i.*,
           (SELECT json_group_array(t.name) FROM item_tag it JOIN tag t ON t.id = it.tag_id
             WHERE it.item_id = i.id) AS tags,
           (SELECT json_group_object(p.field, p.source) FROM field_provenance p
             WHERE p.item_id = i.id) AS provenance
         FROM item i ORDER BY i.id",
        &["tags", "provenance"],
    )?;
    Ok(json!({
        "format_version": FORMAT_VERSION,
        "exported_at": crate::db::now(),
        "items": items,
        "tags": rows(conn, "SELECT id, name, description FROM tag ORDER BY id", &[])?,
        "links": rows(conn, "SELECT from_id, to_id, kind FROM item_link ORDER BY from_id, to_id, kind", &[])?,
        "history": rows(conn, "SELECT id, item_id, from_status, to_status, at, note FROM status_history ORDER BY id", &[])?,
        "saved_filters": rows(conn, "SELECT id, name, definition_json AS definition FROM saved_filter ORDER BY id", &["definition"])?,
    }))
}

/// Write the export to `path` (temp file + rename); returns the number of items.
pub fn export_library(conn: &Connection, path: &Path) -> Result<usize> {
    let doc = build(conn)?;
    let n = doc["items"].as_array().map_or(0, Vec::len);
    let bytes = serde_json::to_vec_pretty(&doc)?;
    crate::backup::write_then_rename(&path.with_extension("json.tmp"), path, |tmp| {
        Ok(std::fs::write(tmp, &bytes)?)
    })?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn keys(v: &Value) -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    }

    /// `docs/EXPORT_FORMAT.md`'s example must have the same keys as a real export.
    #[test]
    fn doc_matches_a_real_export() {
        let c = db::open_in_memory().unwrap();
        db::fixture(&c, 5);
        let dir = std::env::temp_dir().join(format!("runout-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("runout-library.json");
        assert_eq!(export_library(&c, &path).unwrap(), 6); // 5 + the artist
        let real: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(dir);

        let md = include_str!("../../docs/EXPORT_FORMAT.md");
        let example = md
            .split("```json")
            .nth(1)
            .unwrap()
            .split("```")
            .next()
            .unwrap();
        let doc: Value = serde_json::from_str(example).expect("example is valid JSON");
        assert_eq!(keys(&doc), keys(&real), "top-level keys");
        for k in ["items", "tags", "links", "history", "saved_filters"] {
            assert_eq!(keys(&doc[k][0]), keys(&real[k][0]), "{k}[] keys");
        }
        assert_eq!(real["format_version"], FORMAT_VERSION);
    }

    #[test]
    fn failed_export_leaves_no_tmp() {
        let c = db::open_in_memory().unwrap();
        let dir = std::env::temp_dir().join(format!("runout-export-fail-{}", std::process::id()));
        let path = dir.join("out");
        std::fs::create_dir_all(&path).unwrap(); // target is a directory: the rename fails
        assert!(export_library(&c, &path).is_err());
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn export_carries_provenance_and_skips_usage() {
        let c = db::open_in_memory().unwrap();
        db::fixture(&c, 3);
        let doc = build(&c).unwrap();
        let it = &doc["items"][1];
        assert_eq!(
            it["provenance"],
            json!({"display_name": "promoted", "rating": "user"})
        );
        assert_eq!(it["tags"], json!(["late night"]));
        assert_eq!(doc["saved_filters"][0]["definition"]["min_rating"], 4);
        assert_eq!(doc["items"][0]["provenance"], json!({}));
        let text = doc.to_string();
        assert!(!text.contains("app_open") && !text.contains("usage"));
    }
}
