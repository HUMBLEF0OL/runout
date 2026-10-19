//! `get_take_insights`: your ratings and backlog set against what you play (TSD §4.9, §4.13).
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use rusqlite::{Connection, params};
use serde::Serialize;

use super::{SRC, Sourced, base, sourced, utc};
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

fn take_line(conn: &Connection, id: i64, line: String) -> Result<Take> {
    Ok(Take {
        item: summary(conn, id)?,
        line,
    })
}

pub fn take(
    conn: &Connection,
    include_private: bool,
    now: DateTime<Utc>,
) -> Result<Sourced<TakeInsights>> {
    // One statement, so the events are scanned once (NFR-04): `per` = counted plays and latest
    // local time per linked record, `stats` = the same per record it counts for (`SRC`).
    // Rows: (list, id, number, text); list 3 = unfinished albums (played, total as text).
    // TSD §4.9: unfinished only for albums whose track list is in the library (`on_album` links).
    let sql = format!(
        "{}{SRC}, per AS (SELECT item_id AS s, SUM(counts_as_play) AS plays,
                     MAX(CASE WHEN counts_as_play THEN local END) AS last
                   FROM base WHERE +item_id IS NOT NULL GROUP BY +item_id),
           stats AS (SELECT src.t AS item_id, SUM(per.plays) AS plays, MAX(per.last) AS last
                     FROM src JOIN per ON per.s = src.s GROUP BY src.t)
         SELECT * FROM (SELECT 0, i.id, s.plays, '' FROM item i JOIN stats s ON s.item_id = i.id
           WHERE i.rating IS NULL AND s.plays >= ?1 ORDER BY s.plays DESC, i.id LIMIT {LIMIT})
         UNION ALL SELECT * FROM (SELECT 1, i.id, 0, i.added_at FROM item i
           LEFT JOIN stats s ON s.item_id = i.id
           WHERE i.in_backlog = 1 AND i.status = 'planned' AND i.type IN ('album', 'track')
             AND COALESCE(s.plays, 0) = 0 ORDER BY i.added_at, i.id LIMIT {LIMIT})
         UNION ALL SELECT * FROM (SELECT 2, i.id, i.rating, s.last FROM item i
           JOIN stats s ON s.item_id = i.id
           WHERE i.rating >= ?2 AND s.last IS NOT NULL AND s.last < ?3 ORDER BY s.last, i.id LIMIT {LIMIT})
         UNION ALL SELECT * FROM (SELECT 3, l.to_id, COUNT(p.s), CAST(COUNT(*) AS TEXT)
           FROM item_link l JOIN item a ON a.id = l.to_id AND a.type = 'album'
           LEFT JOIN per p ON p.s = l.from_id AND p.plays > 0
           WHERE l.kind = 'on_album' GROUP BY l.to_id HAVING COUNT(p.s) < COUNT(*)
           ORDER BY COUNT(p.s) DESC, l.to_id)",
        base(include_private)
    );
    let forgotten = utc(now - TimeDelta::days(FORGOTTEN_DAYS));
    let mut st = conn.prepare(&sql)?;
    let rows = st
        .query_map(params![PLAYED_OFTEN, TOP_RATED_MIN, forgotten], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (mut rated_vs_played, mut never_played_backlog, mut loved_but_forgotten) =
        (Vec::new(), Vec::new(), Vec::new());
    let mut unfinished_albums = Vec::new();
    for (list, id, n, text) in rows {
        match list {
            0 => rated_vs_played.push(take_line(
                conn,
                id,
                format!("Played {n} times. Not rated yet."),
            )?),
            1 => never_played_backlog.push(take_line(
                conn,
                id,
                format!("In your backlog since {}. Never played.", day(&text)),
            )?),
            2 => loved_but_forgotten.push(take_line(
                conn,
                id,
                format!("You rated it {n}. Last played {}.", day(&text)),
            )?),
            _ => unfinished_albums.push(Unfinished {
                item: summary(conn, id)?,
                played: n as i64,
                total: text.parse().unwrap_or_default(),
            }),
        }
    }
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
