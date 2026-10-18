//! `get_dashboard`, `get_counts` (SC-01/02, FR-015).
use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Serialize;

use super::items::{ItemSummary, summaries};
use crate::error::Result;
use crate::links::{Unresolved, list_unresolved};

/// API.md §3 `get_counts` (rail counts).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Counts {
    pub library: i64,
    pub albums: i64,
    pub tracks: i64,
    pub artists: i64,
    pub backlog: i64,
    pub in_progress: i64,
    pub revisit: i64,
    pub name_needed: i64,
}

pub fn get_counts(conn: &Connection) -> Result<Counts> {
    Ok(conn.query_row(
        "SELECT count(*), total(type = 'album'), total(type = 'track'), total(type = 'artist'),
           total(in_backlog), total(status = 'in_progress'), total(revisit), total(display_name IS NULL)
         FROM item",
        [],
        |r| {
            let n = |i: usize| -> rusqlite::Result<i64> { Ok(r.get::<_, f64>(i)? as i64) };
            Ok(Counts {
                library: r.get(0)?,
                albums: n(1)?,
                tracks: n(2)?,
                artists: n(3)?,
                backlog: n(4)?,
                in_progress: n(5)?,
                revisit: n(6)?,
                name_needed: n(7)?,
            })
        },
    )?)
}

/// API.md §3 `get_dashboard`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dashboard {
    pub counts: Counts,
    pub in_progress: Vec<ItemSummary>,
    pub name_needed: Vec<ItemSummary>,
    pub unresolved: Vec<Unresolved>,
    pub planned: i64,
    pub explored_this_month: i64,
    pub not_for_me_this_month: i64,
    pub explored_no_review: i64,
    /// In-progress item id → when it last entered `in_progress` (falls back to `added_at`).
    pub started_at: BTreeMap<i64, String>,
}

pub fn get_dashboard(conn: &Connection) -> Result<Dashboard> {
    let (planned, explored_this_month, not_for_me_this_month, explored_no_review) = conn
        .query_row(
            "SELECT total(in_backlog = 1 AND status = 'planned'),
               total(explored_at >= strftime('%Y-%m-01T00:00:00Z', 'now')),
               total(status = 'not_for_me' AND explored_at >= strftime('%Y-%m-01T00:00:00Z', 'now')),
               total(status = 'explored' AND type <> 'artist' AND COALESCE(review, '') = '')
             FROM item",
            [],
            |r| {
                let n = |i: usize| -> rusqlite::Result<i64> { Ok(r.get::<_, f64>(i)? as i64) };
                Ok((n(0)?, n(1)?, n(2)?, n(3)?))
            },
        )?;
    let mut st = conn.prepare(
        "SELECT i.id, COALESCE((SELECT max(h.at) FROM status_history h
             WHERE h.item_id = i.id AND h.to_status = 'in_progress'), i.added_at)
         FROM item i WHERE i.status = 'in_progress'",
    )?;
    let started_at = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Dashboard {
        counts: get_counts(conn)?,
        in_progress: summaries(
            conn,
            "WHERE i.status = 'in_progress' ORDER BY i.modified_at DESC, i.id DESC",
            [],
        )?,
        name_needed: summaries(conn, "WHERE i.display_name IS NULL ORDER BY i.id", [])?,
        unresolved: list_unresolved(conn)?,
        planned,
        explored_this_month,
        not_for_me_this_month,
        explored_no_review,
        started_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::library::items::insert_name_needed;
    use crate::library::status::set_status;
    use crate::library::testutil::add;
    use crate::library::{ItemType, Status};

    #[test]
    fn counts_and_dashboard_tiles() {
        let mut c = open_in_memory().unwrap();
        let a = add(&mut c, ItemType::Album, "A");
        let t = add(&mut c, ItemType::Track, "T");
        let r = add(&mut c, ItemType::Artist, "R");
        insert_name_needed(
            &c,
            ItemType::Album,
            "spotify:album:DDDDDDDDDDDDDDDDDDDDDD",
            "phone",
        )
        .unwrap();
        set_status(&mut c, a, Status::InProgress, None).unwrap();
        set_status(&mut c, t, Status::Explored, None).unwrap();
        set_status(&mut c, r, Status::NotForMe, None).unwrap();

        let n = get_counts(&c).unwrap();
        assert_eq!(
            n,
            Counts {
                library: 4,
                albums: 2,
                tracks: 1,
                artists: 1,
                backlog: 2,
                in_progress: 1,
                revisit: 0,
                name_needed: 1
            }
        );
        let d = get_dashboard(&c).unwrap();
        assert_eq!(
            d.in_progress.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![a]
        );
        assert_eq!(d.name_needed.len(), 1);
        assert_eq!(d.name_needed[0].added_via, "phone");
        assert_eq!(
            (
                d.planned,
                d.explored_this_month,
                d.not_for_me_this_month,
                d.explored_no_review
            ),
            (1, 2, 1, 1)
        );
        assert!(d.started_at.contains_key(&a));
        let v = serde_json::to_value(&d).unwrap();
        assert!(v["started_at"][a.to_string()].is_string());
        assert!(v.get("recent_activity").is_none());
    }
}
