//! `get_take_insights`: your ratings and backlog set against what you play (TSD §4.9, §4.13).
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use rusqlite::{Connection, ToSql};
use serde::Serialize;

use super::{ATTR, Sourced, base, sourced, utc};
use crate::error::Result;
use crate::library::items::{ItemSummary, summary};
use crate::library::rediscover::TOP_RATED_MIN;

/// "Played but not rated": at least this many counted plays.
pub const PLAYED_OFTEN: i64 = 3;
/// "Loved but forgotten": rated ≥ `TOP_RATED_MIN`, no counted play for this many days.
pub const FORGOTTEN_DAYS: i64 = 90;
const LIMIT: i64 = 10;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Take {
    pub item: ItemSummary,
    pub line: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unfinished {
    pub item: ItemSummary,
    pub played: i64,
    pub total: i64,
}

/// API.md §4 `TakeInsights`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TakeInsights {
    pub rated_vs_played: Vec<Take>,
    pub never_played_backlog: Vec<Take>,
    pub loved_but_forgotten: Vec<Take>,
    pub unfinished_albums: Vec<Unfinished>,
    /// Albums with no linked track, so "unfinished" can't be checked (G-8).
    pub unfinished_unknown: i64,
}

/// "18 May" from a stored timestamp.
fn day(ts: &str) -> String {
    NaiveDate::parse_from_str(ts.get(..10).unwrap_or(ts), "%Y-%m-%d")
        .map(|d| d.format("%-d %b").to_string())
        .unwrap_or_default()
}

/// `sql` (after the `stats` CTE) selects (id, number, text) rows; `line` turns one into the sentence shown.
fn takes(
    conn: &Connection,
    stats: &str,
    sql: &str,
    args: &[&dyn ToSql],
    line: impl Fn(f64, &str) -> String,
) -> Result<Vec<Take>> {
    let mut st = conn.prepare(&format!("{stats} {sql} LIMIT {LIMIT}"))?;
    let rows = st
        .query_map(args, |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, n, s)| {
            Ok(Take {
                item: summary(conn, id)?,
                line: line(n, &s),
            })
        })
        .collect()
}

pub fn take(
    conn: &Connection,
    include_private: bool,
    now: DateTime<Utc>,
) -> Result<Sourced<TakeInsights>> {
    // stats(item_id, plays, last): counted plays per record and the local time of the latest.
    let stats = format!(
        "{}{ATTR}, stats AS (SELECT a.item_id, SUM(b.counts_as_play) AS plays,
                     MAX(CASE WHEN b.counts_as_play THEN b.local END) AS last
                   FROM attr a JOIN base b ON b.id = a.event_id GROUP BY a.item_id)",
        base(include_private)
    );
    let rated_vs_played = takes(
        conn,
        &stats,
        "SELECT i.id, s.plays, '' FROM item i JOIN stats s ON s.item_id = i.id
         WHERE i.rating IS NULL AND s.plays >= ?1 ORDER BY s.plays DESC, i.id",
        &[&PLAYED_OFTEN],
        |n, _| format!("Played {n} times. Not rated yet."),
    )?;
    let never_played_backlog = takes(
        conn,
        &stats,
        "SELECT i.id, 0, i.added_at FROM item i LEFT JOIN stats s ON s.item_id = i.id
         WHERE i.in_backlog = 1 AND i.status = 'planned' AND i.type IN ('album', 'track')
           AND COALESCE(s.plays, 0) = 0 ORDER BY i.added_at, i.id",
        &[],
        |_, added| format!("In your backlog since {}. Never played.", day(added)),
    )?;
    let loved_but_forgotten = takes(
        conn,
        &stats,
        "SELECT i.id, i.rating, s.last FROM item i JOIN stats s ON s.item_id = i.id
         WHERE i.rating >= ?1 AND s.last IS NOT NULL AND s.last < ?2 ORDER BY s.last, i.id",
        &[&TOP_RATED_MIN, &utc(now - TimeDelta::days(FORGOTTEN_DAYS))],
        |r, last| format!("You rated it {r}. Last played {}.", day(last)),
    )?;

    // TSD §4.9: only albums whose track list is in the library (tracks linked `on_album`).
    let mut st = conn.prepare(&format!(
        "{}, played AS (SELECT DISTINCT item_id FROM base WHERE counts_as_play AND item_id IS NOT NULL)
         SELECT l.to_id, COUNT(*), COUNT(p.item_id) FROM item_link l
           JOIN item a ON a.id = l.to_id AND a.type = 'album'
           LEFT JOIN played p ON p.item_id = l.from_id
         WHERE l.kind = 'on_album' GROUP BY l.to_id HAVING COUNT(p.item_id) < COUNT(*)
         ORDER BY COUNT(p.item_id) DESC, l.to_id",
        base(include_private)
    ))?;
    let rows = st
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let unfinished_albums = rows
        .into_iter()
        .map(|(id, total, played)| {
            Ok(Unfinished {
                item: summary(conn, id)?,
                played,
                total,
            })
        })
        .collect::<Result<_>>()?;
    let unfinished_unknown = conn.query_row(
        "SELECT COUNT(*) FROM item a WHERE a.type = 'album'
           AND NOT EXISTS (SELECT 1 FROM item_link l WHERE l.to_id = a.id AND l.kind = 'on_album')",
        [],
        |r| r.get(0),
    )?;
    let data = TakeInsights {
        rated_vs_played,
        never_played_backlog,
        loved_but_forgotten,
        unfinished_albums,
        unfinished_unknown,
    };
    sourced(conn, include_private, data)
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{self, ALBUM, ARTIST, LONELY_ALBUM, T1, T2, T3};
    use super::*;

    fn ids(t: &[Take]) -> Vec<i64> {
        t.iter().map(|t| t.item.id).collect()
    }

    #[test]
    fn take_matches_hand_counts() {
        let (c, _) = fixture::build();
        let d = take(&c, false, fixture::now()).unwrap().data;
        // Album One: Song One (e1, e2, e7) and Song Two (e6) played, Song Three not.
        assert_eq!(d.unfinished_albums.len(), 1);
        let u = &d.unfinished_albums[0];
        assert_eq!((u.item.id, u.played, u.total), (ALBUM, 2, 3));
        assert_eq!(d.unfinished_unknown, 1); // Lonely Album
        // Zorblax: Song One ×3 + Song Two ×1 (each event once); Song One ×3. Album One is rated.
        assert_eq!(ids(&d.rated_vs_played), vec![ARTIST, T1]);
        assert_eq!(d.rated_vs_played[1].line, "Played 3 times. Not rated yet.");
        assert_eq!(ids(&d.never_played_backlog), vec![T3, LONELY_ALBUM]);
        assert_eq!(
            d.never_played_backlog[1].line,
            "In your backlog since 25 Sep. Never played."
        );
        assert_eq!(ids(&d.loved_but_forgotten), vec![T2]);
        assert_eq!(
            d.loved_but_forgotten[0].line,
            "You rated it 5. Last played 10 Jan."
        );
    }
}
