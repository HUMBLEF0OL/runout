//! `get_take_insights`: your ratings and backlog set against what you play (TSD §4.9, §4.13).
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rusqlite::Connection;
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
    // `per` = counted plays per linked record, from the 0005 rollup (base rows: reconciled left out,
    // private per setting). The latest local play time is read only for the records that count
    // for a top-rated one ("loved but forgotten"), on the item index (NFR-04: no pass over every
    // linked event).
    // TSD §4.9: unfinished only for albums whose track list is in the library (`on_album` links).
    let private = if include_private {
        ""
    } else {
        " AND private_session = 0"
    };
    let per: HashMap<i64, i64> = conn
        .prepare(&format!(
            "SELECT item_id, SUM(counts_as_play) FROM listening_rollup
             WHERE item_id <> 0 AND events > 0{private} GROUP BY 1"
        ))?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    // `SRC`: each record counts for itself, its album and artists
    let src: Vec<(i64, i64)> = conn
        .prepare(&format!("WITH x AS (SELECT 1){SRC} SELECT s, t FROM src"))?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let rated: HashSet<i64> = conn
        .prepare("SELECT id FROM item WHERE rating >= ?1")?
        .query_map([TOP_RATED_MIN], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let wanted: BTreeSet<i64> = src
        .iter()
        .filter(|(s, t)| rated.contains(t) && per.get(s).is_some_and(|p| *p > 0))
        .map(|x| x.0)
        .collect();
    let mut last: HashMap<i64, String> = HashMap::new();
    if !wanted.is_empty() {
        let ids: Vec<String> = wanted.iter().map(i64::to_string).collect();
        let mut st = conn.prepare(&format!(
            "{} SELECT item_id, MAX(local) FROM base WHERE item_id IN ({}) AND counts_as_play
             GROUP BY 1",
            base(include_private),
            ids.join(", ")
        ))?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            last.insert(r.get(0)?, r.get(1)?);
        }
    }
    // `stats` = plays and the latest play per record it counts for
    let mut stats: HashMap<i64, (i64, Option<String>)> = HashMap::new();
    for (s, t) in src {
        if let Some(plays) = per.get(&s) {
            let e = stats.entry(t).or_default();
            e.0 += plays;
            let l = last.get(&s).filter(|_| rated.contains(&t));
            if l > e.1.as_ref() {
                e.1 = l.cloned();
            }
        }
    }
    struct It {
        id: i64,
        rating: Option<f64>,
        backlog: bool,
        planned: bool,
        album_or_track: bool,
        album: bool,
        added_at: String,
    }
    let items: Vec<It> = conn
        .prepare("SELECT id, rating, in_backlog, status = 'planned', type, added_at FROM item")?
        .query_map([], |r| {
            let kind: String = r.get(4)?;
            Ok(It {
                id: r.get(0)?,
                rating: r.get(1)?,
                backlog: r.get(2)?,
                planned: r.get(3)?,
                album_or_track: kind == "album" || kind == "track",
                album: kind == "album",
                added_at: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let forgotten = utc(now - TimeDelta::days(FORGOTTEN_DAYS));
    // Rows: (list, id, number, text); list 3 = unfinished albums (played, total as text).
    type Row = (i64, i64, f64, String);
    let mut rows: Vec<Row> = vec![];
    let top = |mut v: Vec<Row>, rows: &mut Vec<Row>| {
        v.truncate(LIMIT as usize);
        rows.extend(v);
    };
    // 0: played often, not rated (most played first)
    let mut v: Vec<Row> = items
        .iter()
        .filter(|i| i.rating.is_none())
        .filter_map(|i| stats.get(&i.id).map(|s| (i, s.0)))
        .filter(|(_, plays)| *plays >= PLAYED_OFTEN)
        .map(|(i, plays)| (0, i.id, plays as f64, String::new()))
        .collect();
    v.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.1.cmp(&b.1)));
    top(v, &mut rows);
    // 1: planned in the backlog, never played (oldest first)
    let mut v: Vec<(&It, Row)> = items
        .iter()
        .filter(|i| i.backlog && i.planned && i.album_or_track)
        .filter(|i| stats.get(&i.id).map_or(0, |s| s.0) == 0)
        .map(|i| (i, (1, i.id, 0.0, i.added_at.clone())))
        .collect();
    v.sort_by(|a, b| a.0.added_at.cmp(&b.0.added_at).then(a.0.id.cmp(&b.0.id)));
    top(v.into_iter().map(|x| x.1).collect(), &mut rows);
    // 2: rated high, last played before the forgotten date (longest ago first)
    let mut v: Vec<Row> = items
        .iter()
        .filter_map(|i| {
            let rating = i.rating.filter(|r| *r >= TOP_RATED_MIN)?;
            let last = stats.get(&i.id)?.1.as_ref().filter(|l| **l < forgotten)?;
            Some((2, i.id, rating, last.clone()))
        })
        .collect();
    v.sort_by(|a, b| a.3.cmp(&b.3).then(a.1.cmp(&b.1)));
    top(v, &mut rows);
    // 3: albums with some of their linked tracks unplayed (most played first)
    let albums: HashSet<i64> = items.iter().filter(|i| i.album).map(|i| i.id).collect();
    let mut by_album: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    {
        let mut st =
            conn.prepare("SELECT from_id, to_id FROM item_link WHERE kind = 'on_album'")?;
        let mut links = st.query([])?;
        while let Some(r) = links.next()? {
            let (from, to): (i64, i64) = (r.get(0)?, r.get(1)?);
            if albums.contains(&to) {
                let e = by_album.entry(to).or_default();
                e.0 += per.get(&from).is_some_and(|p| *p > 0) as i64;
                e.1 += 1;
            }
        }
    }
    let mut v: Vec<Row> = by_album
        .into_iter()
        .filter(|(_, (played, total))| played < total)
        .map(|(id, (played, total))| (3, id, played as f64, total.to_string()))
        .collect();
    v.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.1.cmp(&b.1)));
    rows.extend(v);
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
