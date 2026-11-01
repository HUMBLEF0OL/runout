//! Tags: list, rename, merge (each item tagged once), delete (SC-13b). Names are unique ignoring case.
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::filters::ItemFilter;
use crate::error::{AppError, Result};
use crate::text::{clean_line, limit};

/// API.md §2 `Tag`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub item_count: i64,
}

const TAG_SQL: &str = "SELECT t.id, t.name, t.description,
  (SELECT count(*) FROM item_tag it WHERE it.tag_id = t.id) FROM tag t";

fn tag_row(r: &rusqlite::Row) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        item_count: r.get(3)?,
    })
}

pub fn list_tags(conn: &Connection) -> Result<Vec<Tag>> {
    let mut st = conn.prepare(&format!("{TAG_SQL} ORDER BY t.name COLLATE NOCASE"))?;
    let rows = st.query_map([], tag_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn get(conn: &Connection, id: i64) -> Result<Tag> {
    conn.query_row(&format!("{TAG_SQL} WHERE t.id = ?1"), [id], tag_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("Tag not found."))
}

/// Replace the item's tag set, creating tags as needed (matched ignoring case, first spelling wins).
pub(crate) fn set_item_tags(conn: &Connection, item: i64, names: &[String]) -> Result<()> {
    conn.execute("DELETE FROM item_tag WHERE item_id = ?1", [item])?;
    for name in names.iter().map(|n| clean_line(n, limit::TAG)) {
        if name.is_empty() {
            continue;
        }
        conn.execute("INSERT OR IGNORE INTO tag(name) VALUES (?1)", [&name])?;
        conn.execute(
            "INSERT OR IGNORE INTO item_tag(item_id, tag_id) SELECT ?1, id FROM tag WHERE name = ?2",
            params![item, name],
        )?;
    }
    Ok(())
}

/// Point saved filters' `tags` at `to` instead of `from` (ignoring case, like `tag.name`), deduped.
fn rewrite_saved_filters(conn: &Connection, from: &str, to: &str) -> Result<()> {
    let mut st = conn.prepare("SELECT id, definition_json FROM saved_filter")?;
    let rows = st
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (fid, def) in rows {
        let mut f: ItemFilter = serde_json::from_str(&def)?;
        let Some(tags) = f.tags.as_mut() else {
            continue;
        };
        if !tags.iter().any(|t| t.eq_ignore_ascii_case(from)) {
            continue;
        }
        let mut out: Vec<String> = vec![];
        for t in tags.drain(..) {
            let t = if t.eq_ignore_ascii_case(from) {
                to.to_string()
            } else {
                t
            };
            if !out.iter().any(|o| o.eq_ignore_ascii_case(&t)) {
                out.push(t);
            }
        }
        *tags = out;
        conn.execute(
            "UPDATE saved_filter SET definition_json = ?2 WHERE id = ?1",
            params![fid, serde_json::to_string(&f)?],
        )?;
    }
    Ok(())
}

/// Rename (in one transaction with the saved filters that name it); another tag with the same
/// name (ignoring case) → `validation` (use merge instead).
pub fn rename_tag(conn: &mut Connection, id: i64, name: &str) -> Result<Tag> {
    let name = clean_line(name, limit::TAG);
    if name.is_empty() {
        return Err(AppError::validation("A tag needs a name."));
    }
    let old = get(conn, id)?.name;
    let tx = conn.transaction()?;
    let clash: Option<i64> = tx
        .query_row(
            "SELECT id FROM tag WHERE name = ?1 AND id <> ?2",
            params![name, id],
            |r| r.get(0),
        )
        .optional()?;
    if clash.is_some() {
        return Err(AppError::validation(
            "There's already a tag with that name. Merge them instead.",
        ));
    }
    tx.execute("UPDATE tag SET name = ?2 WHERE id = ?1", params![id, name])?;
    rewrite_saved_filters(&tx, &old, &name)?;
    tx.commit()?;
    get(conn, id)
}

/// Move every item from `from` to `into` (an item with both keeps one), then drop `from`.
/// Saved filters naming `from` now name `into`.
pub fn merge_tags(conn: &mut Connection, from: i64, into: i64) -> Result<Tag> {
    if from == into {
        return Err(AppError::validation("Pick two different tags."));
    }
    let from_name = get(conn, from)?.name;
    let into_name = get(conn, into)?.name;
    let tx = conn.transaction()?;
    rewrite_saved_filters(&tx, &from_name, &into_name)?;
    tx.execute(
        "INSERT OR IGNORE INTO item_tag(item_id, tag_id) SELECT item_id, ?2 FROM item_tag WHERE tag_id = ?1",
        params![from, into],
    )?;
    tx.execute("DELETE FROM tag WHERE id = ?1", [from])?;
    tx.commit()?;
    get(conn, into)
}

/// Untags every item (cascade). Deleting a missing tag is fine.
pub fn delete_tag(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM tag WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::error::Code;
    use crate::library::ItemType;
    use crate::library::items::create;
    use crate::library::testutil::new;

    fn tagged(c: &mut Connection, name: &str, tags: &[&str]) -> i64 {
        let mut n = new(ItemType::Album, name);
        n.tags = tags.iter().map(|t| t.to_string()).collect();
        create(c, n).unwrap().summary.id
    }

    fn id_of(c: &Connection, name: &str) -> i64 {
        c.query_row("SELECT id FROM tag WHERE name = ?1", [name], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn merge_keeps_every_item_tagged_once() {
        let mut c = open_in_memory().unwrap();
        let both = tagged(&mut c, "both", &["chill", "mellow"]);
        let a = tagged(&mut c, "a", &["chill"]);
        let b = tagged(&mut c, "b", &["mellow"]);
        let (from, into) = (id_of(&c, "chill"), id_of(&c, "mellow"));
        let t = merge_tags(&mut c, from, into).unwrap();
        assert_eq!((t.name.as_str(), t.item_count), ("mellow", 3));
        for id in [both, a, b] {
            let n: i64 = c
                .query_row(
                    "SELECT count(*) FROM item_tag WHERE item_id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "item {id} tagged once");
        }
        assert_eq!(list_tags(&c).unwrap().len(), 1);
        assert_eq!(
            merge_tags(&mut c, into, into).unwrap_err().code,
            Code::Validation
        );
    }

    #[test]
    fn saved_filters_follow_rename_and_merge() {
        use crate::library::filters::{ItemFilter, list_items, list_saved_filters, save_filter};
        let mut c = open_in_memory().unwrap();
        tagged(&mut c, "a", &["chill"]);
        tagged(&mut c, "b", &["calm"]);
        let s = |v: &[&str]| v.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        let by_tags = |tags: &[&str]| ItemFilter {
            tags: Some(s(tags)),
            ..Default::default()
        };
        save_filter(&c, "chill", by_tags(&["chill"])).unwrap();
        save_filter(&c, "both", by_tags(&["CHILL", "calm"])).unwrap();
        let hits = |c: &Connection| -> Vec<(Vec<String>, i64)> {
            list_saved_filters(c)
                .unwrap()
                .into_iter()
                .map(|f| {
                    let n = list_items(c, &f.definition).unwrap().total;
                    (f.definition.tags.unwrap(), n)
                })
                .collect()
        };
        let chill = id_of(&c, "chill");
        rename_tag(&mut c, chill, "mellow").unwrap();
        assert_eq!(
            hits(&c),
            vec![(s(&["mellow"]), 1), (s(&["mellow", "calm"]), 0)]
        );
        let (mellow, calm) = (id_of(&c, "mellow"), id_of(&c, "calm"));
        merge_tags(&mut c, mellow, calm).unwrap();
        assert_eq!(hits(&c), vec![(s(&["calm"]), 2), (s(&["calm"]), 2)]);
    }

    #[test]
    fn rename_is_case_insensitive_unique_and_delete_untags() {
        let mut c = open_in_memory().unwrap();
        tagged(&mut c, "x", &["jazz", "rock"]);
        let jazz = id_of(&c, "jazz");
        assert_eq!(
            rename_tag(&mut c, jazz, "ROCK").unwrap_err().code,
            Code::Validation
        );
        assert_eq!(rename_tag(&mut c, jazz, "Jazz").unwrap().name, "Jazz"); // own case change is fine
        assert_eq!(
            rename_tag(&mut c, jazz, "  ").unwrap_err().code,
            Code::Validation
        );
        assert_eq!(
            rename_tag(&mut c, 99, "x").unwrap_err().code,
            Code::NotFound
        );
        delete_tag(&c, jazz).unwrap();
        let tags = list_tags(&c).unwrap();
        assert_eq!(
            tags.iter()
                .map(|t| (t.name.as_str(), t.item_count))
                .collect::<Vec<_>>(),
            vec![("rock", 1)]
        );
    }
}
