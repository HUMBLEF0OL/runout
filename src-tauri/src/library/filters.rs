//! `ItemFilter` → SQL (bound params only) for `list_items`, and saved filters (FR-013).
use rusqlite::types::Value;
use rusqlite::{Connection, params, params_from_iter};
use serde::{Deserialize, Serialize};

use super::items::{ItemSummary, summaries};
use super::{ItemType, Page, Priority, Status};
use crate::error::{AppError, Result};
use crate::text::{clean_line, limit};

pub const DEFAULT_LIMIT: i64 = 50;
/// Upper bound per page (playlist-from-filter reads up to this many).
pub const MAX_LIMIT: i64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    BacklogOrder,
    AddedAt,
    ModifiedAt,
    Rating,
    Name,
}

/// API.md §2 `ItemFilter`. Every field optional; absent fields are left out when stored.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<ItemType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Vec<Status>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_rating: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revisit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favourite: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_backlog: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_needed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Vec<Priority>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Sort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

/// `WHERE …` over `item i` + its params. Only fixed SQL fragments; every value is bound.
fn where_sql(f: &ItemFilter) -> (String, Vec<Value>) {
    let mut w: Vec<String> = vec![];
    let mut p: Vec<Value> = vec![];
    if let Some(t) = f.r#type {
        w.push("i.type = ?".into());
        p.push(t.as_str().to_string().into());
    }
    if let Some(s) = f.status.as_ref().filter(|s| !s.is_empty()) {
        w.push(format!("i.status IN ({})", placeholders(s.len())));
        p.extend(s.iter().map(|s| Value::from(s.as_str().to_string())));
    }
    for tag in f.tags.iter().flatten() {
        w.push(
            "EXISTS (SELECT 1 FROM item_tag it JOIN tag t ON t.id = it.tag_id
               WHERE it.item_id = i.id AND t.name = ?)"
                .into(),
        );
        p.push(clean_line(tag, limit::TAG).into());
    }
    if let Some(r) = f.min_rating {
        w.push("COALESCE(i.rating, 0) >= ?".into());
        p.push(r.into());
    }
    for (col, v) in [
        ("i.revisit", f.revisit),
        ("i.favourite", f.favourite),
        ("i.in_backlog", f.in_backlog),
        ("(i.display_name IS NULL)", f.name_needed),
    ] {
        if let Some(v) = v {
            w.push(format!("{col} = ?"));
            p.push(Value::from(v as i64));
        }
    }
    if let Some(pr) = f.priority.as_ref().filter(|s| !s.is_empty()) {
        w.push(format!("i.priority IN ({})", placeholders(pr.len())));
        p.extend(pr.iter().map(|s| Value::from(s.as_str().to_string())));
    }
    let sql = if w.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", w.join(" AND "))
    };
    (sql, p)
}

/// `ORDER BY …`. Defaults mirror the mock: `added_at`, newest first; nulls sort last ascending.
fn order_sql(f: &ItemFilter) -> String {
    let sort = f.sort.unwrap_or(Sort::AddedAt);
    let desc = f.desc.unwrap_or(matches!(
        sort,
        Sort::AddedAt | Sort::ModifiedAt | Sort::Rating
    ));
    let d = if desc { "DESC" } else { "ASC" };
    let key = match sort {
        Sort::BacklogOrder => format!("(i.backlog_order IS NULL) {d}, i.backlog_order {d}"),
        Sort::AddedAt => format!("i.added_at {d}"),
        Sort::ModifiedAt => format!("i.modified_at {d}"),
        Sort::Rating => format!("COALESCE(i.rating, -1) {d}"),
        Sort::Name => format!("(i.display_name IS NULL) {d}, i.display_name COLLATE NOCASE {d}"),
    };
    format!("ORDER BY {key}, i.id {d}")
}

/// `list_items` (SC-03/04).
pub fn list_items(conn: &Connection, f: &ItemFilter) -> Result<Page<ItemSummary>> {
    let (w, mut p) = where_sql(f);
    let total: i64 = conn.query_row(
        &format!("SELECT count(*) FROM item i {w}"),
        params_from_iter(p.iter()),
        |r| r.get(0),
    )?;
    let offset = f.offset.unwrap_or(0).max(0);
    let lim = f.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    p.push(lim.into());
    p.push(offset.into());
    let rows = summaries(
        conn,
        &format!("{w} {} LIMIT ? OFFSET ?", order_sql(f)),
        params_from_iter(p.iter()),
    )?;
    Ok(Page {
        rows,
        total,
        offset,
    })
}

/// API.md §2 `SavedFilter`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SavedFilter {
    pub id: i64,
    pub name: String,
    pub definition: ItemFilter,
}

pub fn list_saved_filters(conn: &Connection) -> Result<Vec<SavedFilter>> {
    let mut st = conn.prepare("SELECT id, name, definition_json FROM saved_filter ORDER BY id")?;
    let rows = st
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, name, def)| {
            Ok(SavedFilter {
                id,
                name,
                definition: serde_json::from_str(&def)?,
            })
        })
        .collect()
}

/// `save_filter`. Paging is not part of a saved definition.
pub fn save_filter(conn: &Connection, name: &str, definition: ItemFilter) -> Result<SavedFilter> {
    let name = clean_line(name, limit::FILTER);
    if name.is_empty() {
        return Err(AppError::validation("Give the filter a name."));
    }
    // Tags as `tag.name` stores them, so rename/merge (matching ignoring case) can rewrite them.
    let tags = definition.tags.clone().map(|ts| {
        let mut out: Vec<String> = vec![];
        for t in ts.iter().map(|t| clean_line(t, limit::TAG)) {
            if !t.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&t)) {
                out.push(t);
            }
        }
        out
    });
    let definition = ItemFilter {
        offset: None,
        limit: None,
        tags,
        ..definition
    };
    conn.execute(
        "INSERT INTO saved_filter(name, definition_json) VALUES (?1, ?2)",
        params![name, serde_json::to_string(&definition)?],
    )?;
    Ok(SavedFilter {
        id: conn.last_insert_rowid(),
        name,
        definition,
    })
}

pub fn delete_saved_filter(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM saved_filter WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::library::items::{ItemPatch, create, update};
    use crate::library::testutil::{add, new};

    fn names(p: &Page<ItemSummary>) -> Vec<&str> {
        p.rows
            .iter()
            .map(|r| r.display_name.as_deref().unwrap_or("?"))
            .collect()
    }

    fn filter(v: serde_json::Value) -> ItemFilter {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn list_items_filters_sorts_and_pages() {
        let mut c = open_in_memory().unwrap();
        for (name, kind, rating, tags) in [
            ("Bravo", ItemType::Album, Some(4.5), vec!["jazz", "live"]),
            ("alpha", ItemType::Album, None, vec!["jazz"]),
            ("Charlie", ItemType::Track, Some(3.0), vec![]),
        ] {
            let mut n = new(kind, name);
            n.tags = tags.into_iter().map(String::from).collect();
            let id = create(&mut c, n).unwrap().summary.id;
            let p = ItemPatch {
                rating: Some(rating),
                ..Default::default()
            };
            update(&mut c, id, p).unwrap();
        }
        let mut n = new(ItemType::Album, "x");
        n.display_name = None;
        n.spotify_uri = Some("spotify:album:CCCCCCCCCCCCCCCCCCCCCC".into());
        create(&mut c, n).unwrap();

        let all = list_items(&c, &ItemFilter::default()).unwrap();
        assert_eq!((all.total, all.offset, all.rows.len()), (4, 0, 4));
        // default: added_at desc (same second → id desc)
        assert_eq!(names(&all), vec!["?", "Charlie", "alpha", "Bravo"]);

        let by_name = list_items(&c, &filter(serde_json::json!({"sort": "name"}))).unwrap();
        assert_eq!(names(&by_name), vec!["alpha", "Bravo", "Charlie", "?"]);
        let f = filter(serde_json::json!({"type": "album", "tags": ["JAZZ"], "sort": "rating"}));
        assert_eq!(names(&list_items(&c, &f).unwrap()), vec!["Bravo", "alpha"]);
        let f = filter(serde_json::json!({"tags": ["jazz", "live"]}));
        assert_eq!(names(&list_items(&c, &f).unwrap()), vec!["Bravo"]);
        let f = filter(serde_json::json!({"min_rating": 3.5}));
        assert_eq!(names(&list_items(&c, &f).unwrap()), vec!["Bravo"]);
        let f = filter(serde_json::json!({"name_needed": true}));
        assert_eq!(list_items(&c, &f).unwrap().total, 1);
        let f = filter(
            serde_json::json!({"status": ["planned"], "in_backlog": true, "sort": "backlog_order"}),
        );
        assert_eq!(
            names(&list_items(&c, &f).unwrap()),
            vec!["Bravo", "alpha", "Charlie", "?"]
        );
        let f = filter(serde_json::json!({"sort": "name", "offset": 1, "limit": 2}));
        let p = list_items(&c, &f).unwrap();
        assert_eq!(
            (names(&p), p.total, p.offset),
            (vec!["Bravo", "Charlie"], 4, 1)
        );
        // injection attempt is just a value
        let f = filter(serde_json::json!({"tags": ["x' OR 1=1 --"]}));
        assert_eq!(list_items(&c, &f).unwrap().total, 0);
    }

    #[test]
    fn saved_filters_round_trip() {
        let mut c = open_in_memory().unwrap();
        add(&mut c, ItemType::Album, "A");
        let def = filter(serde_json::json!({"status": ["explored"], "min_rating": 4, "limit": 5}));
        let f = save_filter(&c, " Best  of ", def).unwrap();
        assert_eq!(f.name, "Best of");
        let stored: String = c
            .query_row("SELECT definition_json FROM saved_filter", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored, r#"{"status":["explored"],"min_rating":4.0}"#);
        assert_eq!(list_saved_filters(&c).unwrap(), vec![f.clone()]);
        assert!(save_filter(&c, "  ", ItemFilter::default()).is_err());
        delete_saved_filter(&c, f.id).unwrap();
        assert!(list_saved_filters(&c).unwrap().is_empty());
    }

    #[test]
    fn saved_filter_tags_are_cleaned_so_rename_rewrites_them() {
        use crate::library::tags::{list_tags, rename_tag};
        let mut c = open_in_memory().unwrap();
        let mut n = new(ItemType::Album, "A");
        n.tags = vec!["chill".into()];
        create(&mut c, n).unwrap();
        let def = filter(serde_json::json!({"tags": [" chill", "CHILL", " ", "jazz "]}));
        let f = save_filter(&c, "Chill", def).unwrap();
        assert_eq!(f.definition.tags, Some(vec!["chill".into(), "jazz".into()]));
        let tag = list_tags(&c)
            .unwrap()
            .into_iter()
            .find(|t| t.name == "chill")
            .unwrap();
        rename_tag(&mut c, tag.id, "mellow").unwrap();
        let saved = &list_saved_filters(&c).unwrap()[0];
        assert_eq!(
            saved.definition.tags,
            Some(vec!["mellow".into(), "jazz".into()])
        );
    }
}
