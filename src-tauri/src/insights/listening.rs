//! `get_listening_insights` (FR-031) and `get_year_review` (FR-034) over the base view.
use chrono::{DateTime, Local, NaiveDate, TimeDelta, TimeZone, Utc};
use rusqlite::{Connection, OptionalExtension, ToSql};
use serde::Serialize;

use super::coverage::{self, Model};
use super::{Covered, Range, Sourced, base, hours, last_months, pct, rollup, since, sourced};
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
    /// The class most of the month's days (up to today) have: no data is never shown as silence.
    pub coverage: super::terms::CoverageClass,
}

/// API.md §4 `PeriodStats & {private_excluded, artists_total}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Totals {
    pub plays: i64,
    pub plays_prev: i64,
    pub ms: i64,
    pub ms_prev: i64,
    pub plays_cmp: i64,
    pub ms_cmp: i64,
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
pub(crate) enum Kind {
    Artist,
    Album,
    Track,
}

/// Top rows by listening time over `cte` (`base` or `rollup`) where `filter` (fixed SQL, one
/// bound param `?1`) holds.
pub(crate) fn rank(
    conn: &Connection,
    cte: &str,
    kind: Kind,
    filter: &str,
    arg: &dyn ToSql,
    limit: usize,
) -> Result<Vec<RankRow>> {
    // (group key, name, by, record lookup); names are fixed SQL, never user input. `by` is '' for an
    // all-NULL artist, as the rollup stores it (G-29).
    let (group, name, by, item) = match kind {
        Kind::Artist => (
            "+lower(artist_name)",
            "MAX(artist_name)",
            "''",
            "(SELECT id FROM item WHERE type = 'artist' AND display_name = g.name COLLATE NOCASE ORDER BY id LIMIT 1)",
        ),
        Kind::Album => (
            "+lower(album_name)",
            "MAX(album_name)",
            "IFNULL(MAX(artist_name), '')",
            "(SELECT id FROM item WHERE type = 'album' AND display_name = g.name COLLATE NOCASE ORDER BY id LIMIT 1)",
        ),
        Kind::Track => (
            "+norm_key",
            "MAX(track_name)",
            "IFNULL(MAX(artist_name), '')",
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
        cte
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

/// Today and the range's start / previous start as local dates (TSD §4.14: periods are by local
/// date); `None` for `all`'s empty bounds.
fn local_days<Tz: TimeZone>(
    tz: &Tz,
    now: DateTime<Utc>,
    start: &str,
    prev: &str,
) -> (NaiveDate, Option<NaiveDate>, Option<NaiveDate>) {
    let day = |t: DateTime<Utc>| t.with_timezone(tz).date_naive();
    let parse = |s: &str| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|t| day(t.to_utc()))
    };
    (day(now), parse(start), parse(prev))
}

pub fn listening(
    conn: &Connection,
    range: Range,
    include_private: bool,
    now: DateTime<Utc>,
) -> Result<Covered<ListeningInsights>> {
    let (start, prev) = range.bounds(now).unwrap_or_default();
    let b = base(include_private);
    // Range `all` reads the rollup (G-17): every event is in range, there is no period before.
    let all = super::use_rollup(&start);
    let (src, in_range) = if all {
        (rollup(include_private), "?1 = ''".to_owned())
    } else {
        (b.clone(), since(&start, "ended_at_utc", "?1"))
    };
    let top = |k| rank(conn, &src, k, &in_range, &start, TOP);

    let (plays, ms, skipped, flagged, phone, exported, artists_total): (
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = conn.query_row(
        &if all {
            format!(
                "{src} SELECT COALESCE(SUM(counts_as_play), 0), COALESCE(SUM(ms_played), 0),
                   0, 0, COALESCE(SUM(phone), 0),
                   COALESCE(SUM(exported), 0),
                   COUNT(DISTINCT CASE WHEN artist_name <> '' THEN lower(artist_name) END)
                 FROM base WHERE ?1 = ''"
            )
        } else {
            format!(
            "{b} SELECT COALESCE(SUM(counts_as_play), 0), COALESCE(SUM(ms_played), 0),
               COUNT(CASE WHEN skipped = 1 THEN 1 END),
               COUNT(CASE WHEN skipped IS NOT NULL THEN 1 END),
               COUNT(CASE WHEN source = 'export' AND counts_as_play
                           AND (lower(platform) LIKE '%ios%' OR lower(platform) LIKE '%android%') THEN 1 END),
               COUNT(CASE WHEN source = 'export' AND counts_as_play THEN 1 END),
               COUNT(DISTINCT CASE WHEN artist_name <> '' THEN lower(artist_name) END)
             FROM base WHERE {in_range}"
        )
        },
        [&start],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        },
    )?;
    // G-45: skip rate = skipped ÷ events with a skip flag. For `all`, the 0005 rollup's `events`
    // counts desktop rows too, so count on 0008's partial index instead.
    let (skipped, flagged): (i64, i64) = if all {
        let private = if include_private {
            ""
        } else {
            " AND private_session = 0"
        };
        conn.query_row(
            // unfiltered: base's filters by hand (reconciled out, private per setting), on the index
            &format!(
                "SELECT COALESCE(SUM(skipped), 0), COUNT(*) FROM listening_event
                      WHERE skipped IS NOT NULL AND reconciled_with IS NULL{private}"
            ),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?
    } else {
        (skipped, flagged)
    };
    // FR-048 (G-46): "vs the period before" counts like for like, over the sources that cover both
    // periods (`coverage::Model::compare`); `*_prev` / `*_cmp` stay 0 when the delta is hidden.
    let (today, start_day, prev_day) = local_days(&Local, now, &start, &prev);
    let months = last_months(now, 12);
    let months_from = NaiveDate::parse_from_str(&format!("{}-01", months[0]), "%Y-%m-%d").ok();
    let from = [prev_day, months_from].into_iter().flatten().min();
    let model = Model::load(conn, &Local, from.unwrap_or(NaiveDate::MIN), today, now)?;
    let (cur, vs) = match (start_day, prev_day) {
        (Some(a), Some(p)) => ((a, today), Some([(p, a - TimeDelta::days(1))])),
        _ => ((model.first.unwrap_or(today), today), None),
    };
    let cov = model.compare(cur, vs.as_ref().map(|v| &v[..]), today);
    let (plays_cmp, ms_cmp, plays_prev, ms_prev): (i64, i64, i64, i64) =
        match coverage::compared(&cov, include_private) {
            Some(cmp) => conn.query_row(
                &format!(
                    "{cmp} SELECT COALESCE(SUM(CASE WHEN ended_at_utc >= ?1 THEN counts_as_play END), 0),
                       COALESCE(SUM(CASE WHEN ended_at_utc >= ?1 THEN ms_played END), 0),
                       COALESCE(SUM(CASE WHEN ended_at_utc < ?1 THEN counts_as_play END), 0),
                       COALESCE(SUM(CASE WHEN ended_at_utc < ?1 THEN ms_played END), 0)
                     FROM cmp WHERE ended_at_utc >= ?2"
                ),
                [&start, &prev],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?,
            None => (0, 0, 0, 0),
        };
    let count = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [&start], |r| r.get(0))?) };
    // Artists heard in the range and never before it; for `all` that is every artist.
    let new_artists = if start.is_empty() {
        artists_total
    } else {
        count(&format!(
            // `+` keeps the range on the time index; 0004's index answers NOT EXISTS per artist.
            "{b}, r AS (SELECT DISTINCT +lower(artist_name) AS a FROM base
                        WHERE ended_at_utc >= ?1 AND artist_name <> '')
             SELECT COUNT(*) FROM r WHERE NOT EXISTS (SELECT 1 FROM base o
               WHERE lower(o.artist_name) = r.a AND o.ended_at_utc < ?1)"
        ))?
    };
    // ponytail: no rated_at column; an item rated and last changed in the range counts.
    let rated_this_period =
        count("SELECT COUNT(*) FROM item WHERE rating IS NOT NULL AND modified_at >= ?1")?;
    // Same rule as the `unrated` tab of `list_plays`.
    let unrated_plays = count(&format!(
        "{src} SELECT {} FROM base e CROSS JOIN item i ON i.id = e.item_id AND i.type = 'track'
         WHERE {} AND e.counts_as_play AND i.rating IS NULL",
        if all {
            "COALESCE(SUM(e.counts_as_play), 0)"
        } else {
            "COUNT(*)"
        },
        if all {
            "?1 = ''".to_owned()
        } else {
            since(&start, "e.ended_at_utc", "?1")
        }
    ))?;
    let private_excluded = if include_private {
        0
    } else if all {
        count(
            "SELECT COALESCE(SUM(counts_as_play), 0) FROM listening_rollup WHERE private_session = 1 AND ?1 = ''",
        )?
    } else {
        // unfiltered: counts exactly the private plays base hides (same reconciled rule)
        count(
            "SELECT COUNT(*) FROM listening_event WHERE private_session = 1
               AND reconciled_with IS NULL AND counts_as_play AND ended_at_utc >= ?1",
        )?
    };

    let mut st = conn.prepare(&format!(
        "{b} SELECT substr(local, 1, 7), SUM(ms_played) FROM base
         WHERE local >= ?1 AND ended_at_utc >= date(?1 || '-01', '-1 day') GROUP BY 1"
    ))?;
    let by_month = st
        .query_map([&months[0]], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<std::collections::HashMap<String, i64>>>()?;
    let monthly = months
        .iter()
        .map(|m| {
            let first = NaiveDate::parse_from_str(&format!("{m}-01"), "%Y-%m-%d").unwrap_or(today);
            let last = super::v3::month_end(first).min(today);
            MonthHours {
                month: m.clone(),
                hours: hours(by_month.get(m).copied().unwrap_or(0)),
                coverage: super::v3::majority(first.iter_days().take_while(|d| *d <= last), |d| {
                    model.class(d)
                }),
            }
        })
        .collect();

    let mut heat = vec![vec![0i64; 24]; 7];
    let mut st = conn.prepare(&if all {
        format!(
            "SELECT dow, hour, SUM(ms_played) FROM listening_heat WHERE ?1 = ''{} GROUP BY 1, 2",
            if include_private { "" } else { " AND private_session = 0" }
        )
    } else {
        format!(
        "{b} SELECT (CAST(strftime('%w', local) AS INT) + 6) % 7, CAST(strftime('%H', local) AS INT),
                    SUM(ms_played)
             FROM base WHERE {in_range} AND strftime('%w', local) IS NOT NULL GROUP BY 1, 2"
    )
    })?;
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
        skip_rate: pct(skipped, flagged),
        // ponytail: phone = export `platform` naming iOS or Android; recheck the values with WI 4 (T1).
        phone_share: pct(phone, exported),
        totals: Totals {
            plays,
            plays_prev,
            ms,
            ms_prev,
            plays_cmp,
            ms_cmp,
            new_artists,
            rated_this_period,
            unrated_plays,
            private_excluded,
            artists_total,
        },
    };
    Ok(Covered {
        sourced: sourced(conn, include_private, data)?,
        coverage: cov,
    })
}

pub fn year_review(
    conn: &Connection,
    year: i32,
    include_private: bool,
) -> Result<Sourced<YearReview>> {
    let y = format!("{year:04}");
    let b = base(include_private);
    // The utc bounds (local is at most a day off) let SQLite use the time index.
    let in_year = format!(
        "substr(local, 1, 4) = ?1 AND ended_at_utc >= '{:04}-12-31' AND ended_at_utc < '{:04}-01-02'",
        year - 1,
        year + 1
    );
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
        Ok(rank(conn, &b, k, &in_year, &y, 1)?
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

    /// TSD §4.14: 00:30 on 20 Oct in UTC+5:30 is still 19 Oct in UTC; today is the local 20 Oct.
    #[test]
    fn periods_are_by_local_date() {
        let ist = chrono::FixedOffset::east_opt(5 * 3600 + 1800).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-10-19T19:00:00Z")
            .unwrap()
            .to_utc();
        let (start, prev) = Range::Week.bounds(now).unwrap();
        let d = |s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
        assert_eq!(
            local_days(&ist, now, &start, &prev),
            (d("2026-10-20").unwrap(), d("2026-10-13"), d("2026-10-06"))
        );
        assert_eq!(local_days(&ist, now, "", "").1, None);
    }

    /// Design §4.4: a month the stored data doesn't cover is no data, never 0 h of silence.
    #[test]
    fn months_carry_their_coverage() {
        use super::super::terms::CoverageClass::*;
        let c = crate::db::open_in_memory().unwrap();
        for t in ["2026-09-01T10:00:00Z", "2026-09-30T10:00:00Z"] {
            fixture::put(&c, fixture::ev(t, 60_000, "Song One", None));
        }
        let s = listening(&c, Range::Week, false, fixture::now()).unwrap();
        let m = &s.sourced.data.monthly;
        assert_eq!(
            [m[0].coverage, m[9].coverage, m[10].coverage, m[11].coverage],
            [NoData, NoData, AllDevices, NoData]
        );
    }

    #[test]
    fn listening_week_matches_hand_totals() {
        let (c, _) = fixture::build();
        let s = listening(&c, Range::Week, false, fixture::now()).unwrap();
        let d = &s.sourced.data;
        // in range: e1 (200 s, phone), e2 (100 s, skipped), e3 (20 s, skipped, not a play);
        // e4 reconciled, e5 private, e7 in the week before.
        assert_eq!(
            d.totals,
            Totals {
                plays: 2,
                ms: 320_000,
                // FR-048: the week runs past the export (18 Oct) and the desktop log (e4) starts on
                // 18 Oct, after the week before began: no like-for-like delta (e7 isn't compared)
                plays_prev: 0,
                ms_prev: 0,
                plays_cmp: 0,
                ms_cmp: 0,
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
        assert_eq!(s.sourced.sources, vec!["U", "E"]);
        assert_eq!(s.coverage.desktop_only_since.as_deref(), Some("2026-10-19"));
        assert_eq!(
            s.coverage.no_compare_reason.as_deref(),
            Some(
                "Plays on this PC are only logged from 18 Oct 2026, so there's nothing like for like to compare with."
            )
        );
    }

    /// G-46: once the week runs past the export's end, both weeks count desktop plays only (reconciled
    /// twins included); the totals still count everything.
    #[test]
    fn home_delta_is_desktop_only_past_the_export() {
        use super::super::fixture::{Ev, ev, put};
        use crate::importer::events::Source;
        let c = crate::db::open_in_memory().unwrap();
        let pc = |utc| Ev {
            source: Source::Logger,
            ..ev(utc, 0, "Song One", None)
        };
        let twin = |utc: &'static str, ms: i64, desk: &'static str| {
            let x = put(&c, ev(utc, ms, "Song One", None));
            let l = put(&c, Ev { ms, ..pc(desk) });
            c.execute(
                "UPDATE listening_event SET reconciled_with = ?1 WHERE id = ?2",
                [x, l],
            )
            .unwrap();
        };
        twin("2026-10-01T10:00:00Z", 60_000, "2026-10-01T10:00:30Z"); // the log ran before both weeks
        twin("2026-10-08T10:00:00Z", 120_000, "2026-10-08T10:00:30Z"); // week before
        twin("2026-10-13T10:00:00Z", 100_000, "2026-10-13T10:00:30Z"); // this week, in the export
        put(
            &c,
            Ev {
                platform: "android",
                ..ev("2026-10-09T10:00:00Z", 60_000, "Song One", None)
            },
        );
        put(&c, ev("2026-10-14T10:00:00Z", 50_000, "Song One", None)); // the export's last play
        put(
            &c,
            Ev {
                ms: 200_000,
                ..pc("2026-10-16T10:00:00Z")
            },
        );
        put(
            &c,
            Ev {
                ms: 40_000,
                ..pc("2026-10-17T10:00:00Z")
            },
        );
        let s = listening(&c, Range::Week, false, fixture::now()).unwrap();
        let t = &s.sourced.data.totals;
        // everything this week: export 13, 14 Oct + desktop 16, 17 Oct
        assert_eq!((t.plays, t.ms), (4, 390_000));
        // like for like on this PC: 13 (twin), 16, 17 Oct vs 8 Oct (twin); the phone play on 9 Oct is out
        assert_eq!((t.plays_cmp, t.ms_cmp), (3, 340_000));
        assert_eq!((t.plays_prev, t.ms_prev), (1, 120_000));
        assert_eq!(s.coverage.compared_sources, ["S"]);
        assert_eq!(s.coverage.desktop_only_since.as_deref(), Some("2026-10-15"));
        assert_eq!(s.coverage.no_compare_reason, None);
        // all time has nothing before it
        let all = listening(&c, Range::All, false, fixture::now()).unwrap();
        assert!(all.coverage.no_compare_reason.is_some());
        assert_eq!(all.sourced.data.totals.plays_prev, 0);
    }

    /// G-29: a local time SQLite can't parse is left out of the heatmap, not an error.
    #[test]
    fn heatmap_skips_unparseable_local_time() {
        let (c, _) = fixture::build();
        let before = listening(&c, Range::Week, false, fixture::now()).unwrap();
        c.execute(
            "INSERT INTO listening_event(source, ended_at_utc, ended_at_local, ms_played, track_name,
               artist_name, album_name, norm_key, dedupe_hash)
             VALUES ('export', '2026-10-17T09:00:00Z', 'not a time', 999000, 'Song One', 'Zorblax',
                     'Album One', 'zorblax|song one', 'g29-heat')",
            [],
        )
        .unwrap();
        for range in [Range::Week, Range::All] {
            listening(&c, range, false, fixture::now()).unwrap();
        }
        let after = listening(&c, Range::Week, false, fixture::now()).unwrap();
        assert_eq!(after.sourced.data.heatmap, before.sourced.data.heatmap);
    }

    /// G-45: the skip rate divides by events with a skip flag only; desktop rows have none.
    #[test]
    fn skip_rate_counts_only_flagged_events() {
        let (c, _) = fixture::build();
        c.execute(
            "INSERT INTO listening_event(source, ended_at_utc, ended_at_local, ms_played, track_name,
               artist_name, album_name, norm_key, dedupe_hash)
             VALUES ('logger', '2026-10-17T12:00:00Z', '2026-10-17T12:00:00', 60000, 'Song One',
                     'Zorblax', 'Album One', 'zorblax|song one', 'g45-desktop')",
            [],
        )
        .unwrap();
        let rate = |r| {
            listening(&c, r, false, fixture::now())
                .unwrap()
                .sourced
                .data
                .skip_rate
        };
        // week: e2, e3 skipped of e1–e3 (not 2 of 4 with the desktop row)
        assert_eq!(rate(Range::Week), 66.7);
        // all: e2, e3 of e1, e2, e3, e6, e7, e8, e9 (e4 reconciled, e5 private, desktop unflagged)
        assert_eq!(rate(Range::All), 28.6);
    }

    #[test]
    fn private_sessions_count_only_with_the_setting() {
        let (c, _) = fixture::build();
        let t = listening(&c, Range::Week, true, fixture::now())
            .unwrap()
            .sourced
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
                // unfiltered: test oracle, applies base's filters by hand
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
