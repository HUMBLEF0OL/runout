//! `get_rediscover`: revisit, top rated, stale backlog, incomplete reviews (FR-013, TSD §4.13).
use rusqlite::Connection;
use serde::Serialize;

use super::items::{ItemSummary, summaries};
use crate::error::Result;

/// TSD §4.13: planned, in the backlog and unchanged for this many days.
pub const STALE_BACKLOG_DAYS: i64 = 60;
/// "Top rated" shelf: rating at or above this.
pub const TOP_RATED_MIN: f64 = 4.5;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rediscover {
    pub revisit: Vec<ItemSummary>,
    pub top_rated: Vec<ItemSummary>,
    pub stale_backlog: Vec<ItemSummary>,
    pub incomplete_reviews: Vec<ItemSummary>,
}

pub fn get_rediscover(conn: &Connection) -> Result<Rediscover> {
    Ok(Rediscover {
        revisit: summaries(conn, "WHERE i.revisit = 1 ORDER BY i.id", [])?,
        top_rated: summaries(
            conn,
            "WHERE i.rating >= ?1 ORDER BY i.rating DESC, i.id",
            [TOP_RATED_MIN],
        )?,
        stale_backlog: summaries(
            conn,
            "WHERE i.status = 'planned' AND i.in_backlog = 1
               AND julianday('now') - julianday(i.modified_at) >= ?1
             ORDER BY i.backlog_order, i.id",
            [STALE_BACKLOG_DAYS],
        )?,
        incomplete_reviews: summaries(
            conn,
            "WHERE i.status = 'explored' AND (i.rating IS NULL OR i.review IS NULL OR i.review = '')
             ORDER BY i.id",
            [],
        )?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::library::items::{ItemPatch, update};
    use crate::library::status::set_status;
    use crate::library::testutil::add;
    use crate::library::{ItemType, Status};

    fn ids(v: &[ItemSummary]) -> Vec<i64> {
        v.iter().map(|i| i.id).collect()
    }

    #[test]
    fn stale_backlog_threshold_and_shelves() {
        let mut c = open_in_memory().unwrap();
        let (old, edge, fresh) = (
            add(&mut c, ItemType::Album, "old"),
            add(&mut c, ItemType::Album, "edge"),
            add(&mut c, ItemType::Album, "fresh"),
        );
        let age = |id: i64, days: i64| {
            c.execute(
                "UPDATE item SET modified_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?2) WHERE id = ?1",
                rusqlite::params![id, format!("-{days} days")],
            )
            .unwrap();
        };
        age(old, 90);
        age(edge, STALE_BACKLOG_DAYS);
        age(fresh, STALE_BACKLOG_DAYS - 1);
        assert_eq!(
            ids(&get_rediscover(&c).unwrap().stale_backlog),
            vec![old, edge]
        );

        let p = ItemPatch {
            rating: Some(Some(5.0)),
            revisit: Some(true),
            ..Default::default()
        };
        update(&mut c, fresh, p).unwrap();
        set_status(&mut c, fresh, Status::Explored, None).unwrap();
        let r = get_rediscover(&c).unwrap();
        assert_eq!(ids(&r.revisit), vec![fresh]);
        assert_eq!(ids(&r.top_rated), vec![fresh]);
        assert_eq!(
            ids(&r.incomplete_reviews),
            vec![fresh],
            "rated but no review"
        );
    }
}
