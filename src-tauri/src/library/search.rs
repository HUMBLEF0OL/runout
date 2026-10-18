//! FTS5 search over `item_fts` (name, review, notes, recommended_by, tags, linked artists) with
//! a plain-text snippet and the field that matched (NFR-04).
use rusqlite::{Connection, params};
use serde::Serialize;

use super::ItemType;
use super::items::{ItemSummary, summary};
use crate::error::Result;
use crate::text::{clean_line, limit};

/// API.md §3 `search` row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchHit {
    pub item: ItemSummary,
    pub snippet: String,
    pub field: String,
}

/// `item_fts` columns in order → the `field` names the UI shows (same as the mock).
const FIELDS: [&str; 6] = [
    "name",
    "review",
    "notes",
    "recommended_by",
    "tags",
    "artist",
];
const MAX_TOKENS: usize = 16;
pub const DEFAULT_LIMIT: i64 = 50;

/// User text → a safe FTS5 expression: each word becomes a quoted prefix term (`"word"*`), all
/// required. User syntax (quotes, `:`, `NEAR`, `-`, `*`) is never interpreted. `None` = no words.
pub fn fts_query(q: &str) -> Option<String> {
    let q = clean_line(q, limit::QUERY);
    let terms: Vec<String> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .take(MAX_TOKENS)
        .map(|t| format!("\"{t}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// `search(q, limit?, type?)`, best match first.
pub fn search(
    conn: &Connection,
    q: &str,
    limit: Option<i64>,
    kind: Option<ItemType>,
) -> Result<Vec<SearchHit>> {
    let Some(expr) = fts_query(q) else {
        return Ok(vec![]);
    };
    // First column with a highlighted match → field + snippet column.
    let col = format!(
        "CASE {} ELSE 0 END",
        (0..FIELDS.len())
            .map(|c| format!(
                "WHEN instr(highlight(item_fts, {c}, char(1), ''), char(1)) > 0 THEN {c}"
            ))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let sql = format!(
        "SELECT item_fts.rowid, {col}, snippet(item_fts, {col}, '', '', '…', 12)
         FROM item_fts JOIN item i ON i.id = item_fts.rowid
         WHERE item_fts MATCH ?1 AND (?2 IS NULL OR i.type = ?2)
         ORDER BY rank LIMIT ?3"
    );
    let lim = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, 200);
    let mut st = conn.prepare(&sql)?;
    let rows = st
        .query_map(params![expr, kind, lim], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, c, snippet)| {
            Ok(SearchHit {
                item: summary(conn, id)?,
                snippet,
                field: FIELDS[c as usize].to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::library::items::{ItemPatch, create, update};
    use crate::library::testutil::{add, new};

    #[test]
    fn query_is_sanitized() {
        assert_eq!(fts_query("night son"), Some(r#""night"* "son"*"#.into()));
        assert_eq!(
            fts_query(r#"" AND artists:x NEAR( -*"#),
            Some(r#""AND"* "artists"* "x"* "NEAR"*"#.into())
        );
        assert_eq!(fts_query("  \"*:() "), None);
    }

    #[test]
    fn finds_a_word_only_in_a_review_with_snippet_and_field() {
        let mut c = open_in_memory().unwrap();
        let id = add(&mut c, ItemType::Album, "Night Songs");
        add(&mut c, ItemType::Album, "Other");
        let p = ItemPatch {
            review: Some(Some("The second side is a luminous slow burn.".into())),
            ..Default::default()
        };
        update(&mut c, id, p).unwrap();
        let hits = search(&c, "lumin", None, None).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].item.id, hits[0].field.as_str()), (id, "review"));
        assert!(hits[0].snippet.contains("luminous"), "{}", hits[0].snippet);
        assert!(!hits[0].snippet.contains('\u{1}'));
        // garbage syntax never errors
        for q in ["\"", "a:b", "NEAR(", "*", "-x", "OR", "a\"b"] {
            search(&c, q, None, None).unwrap();
        }
    }

    #[test]
    fn matches_tags_linked_artists_and_filters_by_type() {
        let mut c = open_in_memory().unwrap();
        let mut n = new(ItemType::Album, "Mordechai");
        n.artist_name = Some("Khruangbin".into());
        n.tags = vec!["psych".into()];
        let album = create(&mut c, n).unwrap().summary.id;
        let hits = search(&c, "khruang", None, None).unwrap();
        // the artist item (name) and the album (linked artist)
        assert_eq!(hits.len(), 2);
        let album_hit = hits.iter().find(|h| h.item.id == album).unwrap();
        assert_eq!(album_hit.field, "artist");
        let only_albums = search(&c, "khruang", None, Some(ItemType::Album)).unwrap();
        assert_eq!(only_albums.len(), 1);
        assert_eq!(only_albums[0].item.id, album);
        assert_eq!(search(&c, "psych", None, None).unwrap()[0].field, "tags");
        assert_eq!(search(&c, "khruang", Some(1), None).unwrap().len(), 1);
    }
}
