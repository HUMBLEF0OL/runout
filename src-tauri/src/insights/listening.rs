//! `get_listening_insights` (FR-031) and `get_year_review` (FR-034) over the base view.
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, ToSql};
use serde::Serialize;

use super::{Range, Sourced, base, hours, last_months, pct, sourced};
use crate::error::Result;
use crate::library::Status;

/// API.md §4 `RankRow`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RankRow {
    pub name: String,
    pub by: String,
    pub plays: i64,
    pub hours: f64,
    pub item_id: Option<i64>,
    pub rating: Option<f64>,
    pub status: Option<Status>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MonthHours {
    pub month: String,
    pub hours: f64,
}

/// API.md §4 `PeriodStats & {private_excluded, artists_total}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Totals {
    pub plays: i64,
    pub plays_prev: i64,
    pub ms: i64,
    pub ms_prev: i64,
    pub new_artists: i64,
    pub rated_this_period: i64,
    pub unrated_plays: i64,
    pub private_excluded: i64,
    pub artists_total: i64,
}

/// API.md §4 `ListeningInsights`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ListeningInsights {
    pub top_artists: Vec<RankRow>,
    pub top_albums: Vec<RankRow>,
    pub top_tracks: Vec<RankRow>,
    pub monthly: Vec<MonthHours>,
    /// 7 rows (Mon..Sun) × 24 hours, listening time scaled to 0..1.
    pub heatmap: Vec<Vec<f64>>,
    pub skip_rate: f64,
    pub phone_share: f64,
    pub totals: Totals,
}

/// API.md §4 `YearReview`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct YearReview {
    pub year: i32,
    pub plays: i64,
    pub hours: f64,
    pub explored: i64,
    pub reviews: i64,
    pub busiest_month: String,
    pub top_artist: RankRow,
    pub top_album: RankRow,
}

const TOP: usize = 10;

#[derive(Clone, Copy)]
enum Kind {
    Artist,
    Album,
    Track,
}

/// Top rows by listening time where `filter` (fixed SQL, one bound param `?1`) holds.
fn rank(
    conn: &Connection,
    private: bool,
    kind: Kind,
    filter: &str,
    arg: &dyn ToSql,
    limit: usize,
) -> Result<Vec<RankRow>> {
    // (group key, name, by, record lookup); names are fixed SQL, never user input.
    let (group, name, by, item) = match kind {
        Kind::Artist => (
            "lower(artist_name)",
            "MAX(artist_name)",
            "''",
            "(SELECT id FROM item WHERE type = 'artist' AND display_name = g.name COLLATE NOCASE ORDER BY id LIMIT 1)",
        ),
        Kind::Album => (
            "lower(album_name)",
            "MAX(album_name)",
            "MAX(artist_name)",
            "(SELECT id FROM item WHERE type = 'album' AND display_name = g.name COLLATE NOCASE ORDER BY id LIMIT 1)",
        ),
        Kind::Track => (
            "norm_key",
            "MAX(track_name)",
            "MAX(artist_name)",
            "(SELECT id FROM item WHERE id = g.iid AND type = 'track')",
        ),
    };
    let sql = format!(
        "{}, g AS (SELECT {name} AS name, {by} AS by_, SUM(counts_as_play) AS p, SUM(ms_played) AS ms,
                     MAX(item_id) AS iid
                   FROM base WHERE {filter} GROUP BY {group} HAVING name <> ''
                   ORDER BY ms DESC, name LIMIT {limit}),
              h AS (SELECT g.*, {item} AS item_id FROM g)
         SELECT h.name, h.by_, h.p, h.ms, h.item_id, i.rating, i.status
         FROM h LEFT JOIN item i ON i.id = h.item_id ORDER BY h.ms DESC, h.name",
        base(private)
    );
    let mut st = conn.prepare(&sql)?;
    let rows = st.query_map([arg], |r| {
        Ok(RankRow {
            name: r.get(0)?,
            by: r.get(1)?,
            plays: r.get(2)?,
            hours: hours(r.get(3)?),
            item_id: r.get(4)?,
            rating: r.get(5)?,
            status: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn listening(
    conn: &Connection,
    range: Range,
    include_private: bool,
    now: DateTime<Utc>,
) -> Result<Sourced<ListeningInsights>> {
    let (start, prev) = range.bounds(now).unwrap_or_default();
    let b = base(include_private);
    let in_range = "ended_at_utc >= ?1";
    let top = |k| rank(conn, include_private, k, in_range, &start, TOP);

    let (plays, plays_prev, ms, ms_prev, skipped, events, phone, exported, artists_total): (
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = conn.query_row(
        &format!(
            "{b} SELECT
               COALESCE(SUM(CASE WHEN ended_at_utc >= ?1 THEN counts_as_play END), 0),
               COALESCE(SUM(CASE WHEN ended_at_utc >= ?2 AND ended_at_utc < ?1 THEN counts_as_play END), 0),
               COALESCE(SUM(CASE WHEN ended_at_utc >= ?1 THEN ms_played END), 0),
               COALESCE(SUM(CASE WHEN ended_at_utc >= ?2 AND ended_at_utc < ?1 THEN ms_played END), 0),
               COUNT(CASE WHEN ended_at_utc >= ?1 AND skipped = 1 THEN 1 END),
               COUNT(CASE WHEN ended_at_utc >= ?1 THEN 1 END),
               COUNT(CASE WHEN ended_at_utc >= ?1 AND source = 'export' AND counts_as_play
                           AND (lower(platform) LIKE '%ios%' OR lower(platform) LIKE '%android%') THEN 1 END),
               COUNT(CASE WHEN ended_at_utc >= ?1 AND source = 'export' AND counts_as_play THEN 1 END),
               COUNT(DISTINCT CASE WHEN ended_at_utc >= ?1 AND artist_name <> '' THEN lower(artist_name) END)
             FROM base"
        ),
        [&start, &prev],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
            ))
        },
    )?;
    let count = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [&start], |r| r.get(0))?) };
    let new_artists = count(&format!(
        "{b}, f AS (SELECT MIN(ended_at_utc) AS first FROM base WHERE artist_name <> ''
                   GROUP BY lower(artist_name))
         SELECT COUNT(*) FROM f WHERE first >= ?1"
    ))?;
    // ponytail: no rated_at column; an item rated and last changed in the range counts.
    let rated_this_period =
        count("SELECT COUNT(*) FROM item WHERE rating IS NOT NULL AND modified_at >= ?1")?;
    // Same rule as the `unrated` tab of `list_plays`.
    let unrated_plays = count(&format!(
        "{b} SELECT COUNT(*) FROM base e JOIN item i ON i.id = e.item_id AND i.type = 'track'
         WHERE e.ended_at_utc >= ?1 AND e.counts_as_play AND i.rating IS NULL"
    ))?;
    let private_excluded = if include_private {
        0
    } else {
        count(
            "SELECT COUNT(*) FROM listening_event WHERE private_session = 1
               AND reconciled_with IS NULL AND counts_as_play AND ended_at_utc >= ?1",
        )?
    };

    let months = last_months(now, 12);
    let mut st = conn.prepare(&format!(
        "{b} SELECT substr(local, 1, 7), SUM(ms_played) FROM base WHERE local >= ?1 GROUP BY 1"
    ))?;
    let by_month = st
        .query_map([&months[0]], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<std::collections::HashMap<String, i64>>>()?;
    let monthly = months
        .iter()
        .map(|m| MonthHours {
            month: m.clone(),
            hours: hours(by_month.get(m).copied().unwrap_or(0)),
        })
        .collect();

    let mut heat = vec![vec![0i64; 24]; 7];
    let mut st = conn.prepare(&format!(
        "{b} SELECT (CAST(strftime('%w', local) AS INT) + 6) % 7, CAST(strftime('%H', local) AS INT),
                    SUM(ms_played)
             FROM base WHERE {in_range} GROUP BY 1, 2"
    ))?;
    for r in st.query_map([&start], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get(2)?))
    })? {
        let (d, h, ms) = r?;
        if (0..7).contains(&d) && (0..24).contains(&h) {
            heat[d as usize][h as usize] = ms;
        }
    }
    let max = heat.iter().flatten().copied().max().unwrap_or(0).max(1) as f64;
    let heatmap = heat
        .iter()
        .map(|row| {
            row.iter()
                .map(|&v| (v as f64 * 100.0 / max).round() / 100.0)
                .collect()
        })
        .collect();

    let data = ListeningInsights {
        top_artists: top(Kind::Artist)?,
        top_albums: top(Kind::Album)?,
        top_tracks: top(Kind::Track)?,
        monthly,
        heatmap,
        skip_rate: pct(skipped, events),
        // ponytail: phone = export `platform` naming iOS or Android; recheck the values with WI 4 (T1).
        phone_share: pct(phone, exported),
        totals: Totals {
            plays,
            plays_prev,
            ms,
            ms_prev,
            new_artists,
            rated_this_period,
            unrated_plays,
            private_excluded,
            artists_total,
        },
    };
    sourced(conn, include_private, data)
}

pub fn year_review(
    conn: &Connection,
    year: i32,
    include_private: bool,
) -> Result<Sourced<YearReview>> {
    let y = format!("{year:04}");
    let b = base(include_private);
    let in_year = "substr(local, 1, 4) = ?1";
    let (plays, ms): (i64, i64) = conn.query_row(
        &format!(
            "{b} SELECT COALESCE(SUM(counts_as_play), 0), COALESCE(SUM(ms_played), 0)
             FROM base WHERE {in_year}"
        ),
        [&y],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let busiest_month = conn
        .query_row(
            &format!(
                "{b} SELECT substr(local, 1, 7) FROM base WHERE {in_year}
                 GROUP BY 1 ORDER BY SUM(ms_played) DESC, 1 LIMIT 1"
            ),
            [&y],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_default();
    let items = |col: &str| -> Result<i64> {
        Ok(conn.query_row(
            &format!("SELECT COUNT(*) FROM item WHERE substr({col}, 1, 4) = ?1"),
            [&y],
            |r| r.get(0),
        )?)
    };
    let first = |k| -> Result<RankRow> {
        Ok(rank(conn, include_private, k, in_year, &y, 1)?
            .pop()
            .unwrap_or_default())
    };
    let data = YearReview {
        year,
        plays,
        hours: hours(ms),
        explored: items("explored_at")?,
        reviews: items("reviewed_at")?,
        busiest_month,
        top_artist: first(Kind::Artist)?,
        top_album: first(Kind::Album)?,
    };
    sourced(conn, include_private, data)
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{self, ALBUM, ARTIST, T1};
    use super::*;

    #[test]
    fn listening_week_matches_hand_totals() {
        let (c, _) = fixture::build();
        let s = listening(&c, Range::Week, false, fixture::now()).unwrap();
        let d = &s.data;
        // in range: e1 (200 s, phone), e2 (100 s, skipped), e3 (20 s, skipped, not a play);
        // e4 reconciled, e5 private, e7 in the week before.
        assert_eq!(
            d.totals,
            Totals {
                plays: 2,
                plays_prev: 1,
                ms: 320_000,
                ms_prev: 90_000,
                new_artists: 1, // Quuxley; Zorblax was first heard in 2025
                rated_this_period: 0,
                unrated_plays: 2, // e1, e2 on Song One (unrated)
                private_excluded: 1,
                artists_total: 2,
            }
        );
        assert_eq!(d.skip_rate, 66.7); // 2 of 3 events
        assert_eq!(d.phone_share, 50.0); // e1 of the 2 exported plays
        assert_eq!(d.top_tracks[0].name, "Song One");
        assert_eq!(
            (d.top_tracks[0].plays, d.top_tracks[0].item_id),
            (2, Some(T1))
        );
        assert_eq!(
            (
                d.top_artists[0].name.as_str(),
                d.top_artists[0].item_id,
                d.top_artists[0].status
            ),
            ("Zorblax", Some(ARTIST), Some(Status::InProgress))
        );
        assert_eq!(
            (d.top_albums[0].item_id, d.top_albums[0].rating),
            (Some(ALBUM), Some(4.0))
        );
        assert_eq!(d.top_albums[0].hours, 0.1);
        // heatmap: Sunday 18 Oct 10:00 has the most time
        assert_eq!(d.heatmap.len(), 7);
        assert_eq!(d.heatmap[6][10], 1.0);
        assert_eq!(d.heatmap[5][10], 0.5); // Sat 17 Oct: 100 s of 200 s
        assert_eq!(d.monthly.len(), 12);
        assert_eq!(d.monthly[11].month, "2026-10");
        assert_eq!(
            d.monthly[11].hours,
            hours(200_000 + 100_000 + 20_000 + 90_000)
        );
        assert_eq!(s.sources, vec!["U", "E"]);
    }

    #[test]
    fn private_sessions_count_only_with_the_setting() {
        let (c, _) = fixture::build();
        let t = listening(&c, Range::Week, true, fixture::now())
            .unwrap()
            .data
            .totals;
        assert_eq!((t.plays, t.private_excluded), (3, 0));
    }

    #[test]
    fn year_review_equals_manual_sum() {
        let (c, ids) = fixture::build();
        let y = year_review(&c, 2026, false).unwrap().data;
        // manual sum over the raw rows: 2026, not reconciled, not private
        let (plays, ms): (i64, i64) = c
            .query_row(
                "SELECT SUM(ms_played >= 30000), SUM(ms_played) FROM listening_event
                 WHERE ended_at_utc LIKE '2026%' AND reconciled_with IS NULL AND private_session = 0",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((plays, ms), (4, 460_000)); // e1, e2, e7, e8 count; e3, e9 don't
        assert_eq!(y.plays, plays);
        assert_eq!(y.hours, hours(ms));
        assert_eq!(y.busiest_month, "2026-10");
        assert_eq!(y.top_artist.name, "Zorblax");
        assert_eq!(y.top_album.name, "Album One");
        assert_eq!((y.explored, y.reviews), (1, 1)); // Album One; Song Two was explored in 2025
        assert_eq!(ids.len(), 9);
        let empty = year_review(&c, 2001, false).unwrap().data;
        assert_eq!(
            (empty.plays, empty.busiest_month.as_str(), empty.top_artist),
            (0, "", RankRow::default())
        );
    }
}
