//! `get_collection_insights`: the library itself (source U only).
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;

use super::{Range, Sourced, freshness, last_months};
use crate::error::Result;
use crate::library::Status;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MonthGrowth {
    pub month: String,
    pub added: i64,
    pub finished: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusCount {
    pub status: Status,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RatingCount {
    pub rating: f64,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagCount {
    pub tag: String,
    pub count: i64,
}

/// API.md §4 `CollectionInsights`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CollectionInsights {
    pub backlog_growth: Vec<MonthGrowth>,
    pub completion: Vec<StatusCount>,
    pub rating_spread: Vec<RatingCount>,
    pub tag_trends: Vec<TagCount>,
    pub avg_days_to_explored: Option<i64>,
    pub reviews: i64,
    pub explored_without_review: i64,
}

const STATUSES: [Status; 5] = [
    Status::Planned,
    Status::InProgress,
    Status::Explored,
    Status::NotForMe,
    Status::Abandoned,
];

// ponytail: `range` is accepted per API.md but the page only asks for 12 months, so growth is
// always the last 12 months; scope it when a page asks for another range.
pub fn collection(
    conn: &Connection,
    _range: Range,
    now: DateTime<Utc>,
) -> Result<Sourced<CollectionInsights>> {
    let one = |sql: &str, arg: &dyn rusqlite::ToSql| -> Result<i64> {
        Ok(conn.query_row(sql, [arg], |r| r.get(0))?)
    };
    let backlog_growth = last_months(now, 12)
        .into_iter()
        .map(|m| {
            Ok(MonthGrowth {
                added: one(
                    "SELECT COUNT(*) FROM item WHERE substr(added_at, 1, 7) = ?1",
                    &m,
                )?,
                finished: one(
                    "SELECT COUNT(*) FROM item WHERE substr(explored_at, 1, 7) = ?1",
                    &m,
                )?,
                month: m,
            })
        })
        .collect::<Result<_>>()?;
    let completion = STATUSES
        .iter()
        .map(|s| {
            Ok(StatusCount {
                status: *s,
                count: one("SELECT COUNT(*) FROM item WHERE status = ?1", s)?,
            })
        })
        .collect::<Result<_>>()?;
    let rating_spread = (1..=10)
        .map(|n| {
            let rating = n as f64 / 2.0;
            Ok(RatingCount {
                rating,
                count: one("SELECT COUNT(*) FROM item WHERE rating = ?1", &rating)?,
            })
        })
        .collect::<Result<_>>()?;
    let mut st = conn.prepare(
        "SELECT t.name, COUNT(*) FROM item_tag it JOIN tag t ON t.id = it.tag_id
         GROUP BY t.id ORDER BY 2 DESC, t.name COLLATE NOCASE LIMIT 10",
    )?;
    let tag_trends = st
        .query_map([], |r| {
            Ok(TagCount {
                tag: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let (avg_days_to_explored, reviews, explored_without_review) = conn.query_row(
        "SELECT CAST(ROUND(AVG(julianday(explored_at) - julianday(added_at))) AS INT),
                COUNT(CASE WHEN review IS NOT NULL AND review <> '' THEN 1 END),
                COUNT(CASE WHEN status = 'explored' AND type <> 'artist'
                            AND (review IS NULL OR review = '') THEN 1 END)
         FROM item",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    Ok(Sourced {
        data: CollectionInsights {
            backlog_growth,
            completion,
            rating_spread,
            tag_trends,
            avg_days_to_explored,
            reviews,
            explored_without_review,
        },
        sources: vec!["U"],
        freshness: freshness(conn)?,
    })
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;

    #[test]
    fn collection_matches_hand_counts() {
        let (c, _) = fixture::build();
        let s = collection(&c, Range::Year, fixture::now()).unwrap();
        let d = s.data;
        assert_eq!(s.sources, vec!["U"]);
        let count = |st| d.completion.iter().find(|x| x.status == st).unwrap().count;
        assert_eq!(
            (
                count(Status::Planned),
                count(Status::Explored),
                count(Status::InProgress)
            ),
            (5, 2, 1)
        );
        let r = |v: f64| {
            d.rating_spread
                .iter()
                .find(|x| x.rating == v)
                .unwrap()
                .count
        };
        assert_eq!((r(4.0), r(5.0), r(0.5)), (1, 1, 0));
        assert_eq!(d.rating_spread.len(), 10);
        assert_eq!((d.reviews, d.explored_without_review), (1, 1)); // Song Two has no review
        // Album One: 1 Feb → 10 Mar = 37 days; Song Two: 1 Nov → 1 Dec = 30 days; mean 33.5
        assert_eq!(d.avg_days_to_explored, Some(34));
        let feb = d
            .backlog_growth
            .iter()
            .find(|m| m.month == "2026-02")
            .unwrap();
        assert_eq!((feb.added, feb.finished), (5, 0));
    }
}
