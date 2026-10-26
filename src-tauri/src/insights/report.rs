//! `get_period_report` (API.md §4.1, design §5 Report tab and §5.1 "the period in a few lines"):
//! every Report card for one period of the page header, compared like for like (FR-048).
use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, Months, NaiveDate, TimeDelta, TimeZone};
use rusqlite::{ToSql, params_from_iter};
use serde::Serialize;

use super::coverage::{self, Coverage, Model};
use super::listening::{Kind, RankRow, rank};
use super::terms::{
    self, Compare, CoverageClass, Fingerprint, FirstHeard, FpParts, Span, TagShare, fingerprint_of,
    median, streaks,
};
use super::v3::{
    Agg, Ctx, Grain, PeriodQuery, Resolved, Src, aggs, artist_item, artist_name, between,
    by_artist, covered, daily, day, majority, minute, month, months, period_coverage, played,
    resolve, show,
};
use super::{Covered, base, pct, rollup};
use crate::error::Result;
use crate::importer::events::Source;
use crate::library::Status;

/// §5.1 rule 1: a change in listening time of at least this share fires a line.
pub const LINE_TIME_CHANGE: f64 = 0.10;
/// Rule 2: a new artist in the top this many.
pub const LINE_NEW_TOP: usize = 5;
/// Rule 3: an artist leaves the top this many …
pub const LINE_LEFT_TOP: usize = 10;
/// … after at least this many months in it.
pub const LINE_LEFT_MONTHS: usize = 6;
/// Rule 4: the busiest hour holds at least this share (%) of the time.
pub const LINE_BUSY_HOUR_PCT: f64 = 30.0;
/// Rule 5: a streak of at least this many days.
pub const LINE_STREAK_DAYS: i64 = 14;
/// At most this many lines.
pub const LINES_MAX: usize = 4;
/// A day-by-day timeline for custom ranges up to this many days (else month by month).
pub const DAILY_MAX_DAYS: i64 = 92;
/// Streak lookback before the period's end.
const STREAK_BACK_DAYS: i64 = 365;
/// Most played rows, and how deep the comparison's ranking goes ("NEW" = not in it).
const TOP: usize = 10;
const MOVE_DEPTH: usize = 50;

/// API.md §4.1 `Compared`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Compared {
    pub value: f64,
    pub now: Option<f64>,
    pub before: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PeriodInfo {
    pub grain: Grain,
    pub vs: Compare,
    pub from: String,
    pub to: String,
    pub vs_from: Option<String>,
    pub vs_to: Option<String>,
    pub prev_at: Option<String>,
    pub next_at: Option<String>,
    pub days: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tiles {
    pub ms: Compared,
    pub plays: Compared,
    pub artists: Compared,
    pub new_artists: Compared,
    pub days_listened: Compared,
    pub streak: i64,
    pub finished_pct: Option<Compared>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Point {
    pub key: String,
    pub ms: i64,
    pub before_ms: Option<i64>,
    pub coverage: CoverageClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Phrase {
    pub text: String,
    pub em: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    TimeChange,
    NewInTop5,
    LeftTop10,
    BusiestHour,
    Streak,
    Record,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sentence {
    pub rule: Rule,
    pub parts: Vec<Phrase>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Clock {
    pub hours: Vec<f64>,
    pub baseline: Vec<f64>,
    pub weekdays: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopNew {
    pub name: String,
    pub item_id: Option<i64>,
    pub status: Option<Status>,
    pub first_played_local: String,
    pub first_track: String,
    pub ms: i64,
    pub plays: i64,
    pub rank: usize,
    pub share_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RatioCards {
    pub tracks: Compared,
    pub albums: Compared,
    pub artists: Compared,
    pub tracks_per_artist: Compared,
    pub albums_per_artist: Compared,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fingerprints {
    pub now: Fingerprint,
    pub average: Option<Fingerprint>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tags {
    pub rows: Vec<TagShare>,
    pub covered_pct: f64,
}

/// API.md §4.1 `TopRow`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopRow {
    #[serde(flatten)]
    pub row: RankRow,
    pub r#move: Option<i64>,
    pub is_new: bool,
    pub spark: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MostPlayed {
    pub artists: Vec<TopRow>,
    pub albums: Vec<TopRow>,
    pub tracks: Vec<TopRow>,
}

/// API.md §4.1 `PeriodReport`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PeriodReport {
    pub period: PeriodInfo,
    pub private_excluded: i64,
    pub tiles: Tiles,
    pub timeline: Vec<Point>,
    pub lines: Vec<Sentence>,
    pub clock: Clock,
    pub top_new: Option<TopNew>,
    pub also_new: Vec<String>,
    pub also_new_more: usize,
    pub ratio: RatioCards,
    pub fingerprint: Fingerprints,
    pub tags: Tags,
    pub most_played: MostPlayed,
}

fn em(s: impl Into<String>) -> Phrase {
    Phrase {
        text: s.into(),
        em: true,
    }
}
fn tx(s: impl Into<String>) -> Phrase {
    Phrase {
        text: s.into(),
        em: false,
    }
}

/// "9 h 12 min", "45 min".
pub fn hm(ms: i64) -> String {
    let m = (ms + 30_000) / 60_000;
    match (m / 60, m % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// How a line names the comparison ("than August", "than your usual month").
fn vs_label(grain: Grain, vs: Compare, single: Option<Span>) -> String {
    let unit = match grain {
        Grain::Week => "week",
        Grain::Month => "month",
        Grain::Year => "year",
        _ => "stretch",
    };
    match (vs, grain, single) {
        (Compare::Usual, ..) => format!("your usual {unit}"),
        (Compare::Prev, Grain::Month, Some(s)) => s.0.format("%B").to_string(),
        (Compare::LastYear, Grain::Month, Some(s)) => s.0.format("%B %Y").to_string(),
        (_, Grain::Year, Some(s)) => s.0.format("%Y").to_string(),
        (Compare::Prev, ..) => format!("the {unit} before"),
        _ => "the same time last year".into(),
    }
}

/// Inputs of the §5.1 rules, computed by the report.
#[derive(Debug, Clone, Default)]
pub struct LineFacts {
    /// Listening time now and before, like for like (`None` = no comparison).
    pub ms_now_before: Option<(f64, f64)>,
    pub vs_label: String,
    pub on_pc: bool,
    /// The period's top artists by time, display names, newest-first heard flag and first date.
    pub top: Vec<(String, Option<NaiveDate>)>,
    /// An artist that left the top 10 and how many months it was in it before.
    pub left: Option<(String, usize)>,
    /// Busiest hour and its share (%).
    pub busiest: Option<(usize, f64)>,
    pub days_listened: i64,
    pub covered_days: i64,
    pub streak: i64,
    /// The biggest day ever (all-device dates), when it falls in the period.
    pub record_day: Option<(NaiveDate, i64)>,
}

/// "The period in a few lines" (design §5.1): fixed rule order, each with its threshold, ≤ 4.
pub fn lines(f: &LineFacts) -> Vec<Sentence> {
    let mut out = vec![];
    let on_pc = if f.on_pc { " on this PC" } else { "" };
    if let Some((now, before)) = f.ms_now_before
        && before > 0.0
        && (now / before - 1.0).abs() >= LINE_TIME_CHANGE
    {
        let p = now / before - 1.0;
        let word = if p > 0.0 { "more" } else { "less" };
        out.push(Sentence {
            rule: Rule::TimeChange,
            parts: vec![
                tx("You listened "),
                em(format!("{}% {word}", (p.abs() * 100.0).round())),
                tx(format!("{on_pc} than {}.", f.vs_label)),
            ],
        });
    }
    if let Some((i, (name, Some(at)))) = f
        .top
        .iter()
        .take(LINE_NEW_TOP)
        .enumerate()
        .find(|(_, a)| a.1.is_some())
    {
        out.push(Sentence {
            rule: Rule::NewInTop5,
            parts: vec![
                em(name.clone()),
                tx(" is new and already your "),
                em(format!("#{}", i + 1)),
                tx(" artist. You first played them on "),
                em(show(*at)),
                tx("."),
            ],
        });
    }
    if let Some((name, n)) = &f.left
        && *n >= LINE_LEFT_MONTHS
    {
        out.push(Sentence {
            rule: Rule::LeftTop10,
            parts: vec![
                em(name.clone()),
                tx(" left your top 10 after "),
                em(format!("{n} months")),
                tx(" in it."),
            ],
        });
    }
    if let Some((h, share)) = f.busiest
        && share >= LINE_BUSY_HOUR_PCT
    {
        out.push(Sentence {
            rule: Rule::BusiestHour,
            parts: vec![
                tx("Your busiest hour was "),
                em(format!("{h:02}:00")),
                tx(": "),
                em(format!("{}%", share.round())),
                tx(" of your time."),
            ],
        });
    }
    if f.streak >= LINE_STREAK_DAYS {
        out.push(Sentence {
            rule: Rule::Streak,
            parts: vec![
                tx("You listened on "),
                em(format!("{} of {} days", f.days_listened, f.covered_days)),
                tx(format!("{on_pc}. Your current streak is ")),
                em(format!("{} days", f.streak)),
                tx("."),
            ],
        });
    }
    if let Some((d, ms)) = f.record_day {
        out.push(Sentence {
            rule: Rule::Record,
            parts: vec![
                em(show(d)),
                tx(" was your biggest day ever: "),
                em(hm(ms)),
                tx("."),
            ],
        });
    }
    out.truncate(LINES_MAX);
    out
}

pub(crate) fn round0(x: f64) -> f64 {
    x.round()
}
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// A `Compared` from the period's value, and like for like now and the comparison windows
/// (`before` = their median: one window, or usual's 12).
pub(crate) fn compared(
    value: f64,
    like: Option<(f64, Vec<f64>)>,
    round: fn(f64) -> f64,
) -> Compared {
    match like {
        None => Compared {
            value,
            now: None,
            before: None,
        },
        Some((now, vs)) => Compared {
            value,
            now: Some(now),
            before: median(&vs).map(round),
        },
    }
}

/// % per bucket, one decimal.
fn shares(v: &[i64]) -> Vec<f64> {
    let total: i64 = v.iter().sum();
    v.iter().map(|x| pct(*x, total)).collect()
}

/// Listening time per local hour and weekday (Mon = 0) in `s`.
fn clock_ms<Tz: TimeZone>(cx: &Ctx<Tz>, s: Span, all_time: bool) -> Result<([i64; 24], [i64; 7])> {
    let (mut h, mut w) = ([0i64; 24], [0i64; 7]);
    if all_time {
        let mut st = cx.conn.prepare(&format!(
            "SELECT hour, dow, SUM(ms_played) FROM listening_heat WHERE hour >= 0{} GROUP BY 1, 2",
            cx.private_filter("private_session")
        ))?;
        for r in st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })? {
            let (hour, dow, ms) = r?;
            if (0..24).contains(&hour) && (0..7).contains(&dow) {
                h[hour as usize] += ms;
                w[dow as usize] += ms;
            }
        }
        return Ok((h, w));
    }
    // by date and hour text (no strftime per row, NFR-04); the weekday comes from the date
    let mut st = cx.conn.prepare(&format!(
        "{} SELECT substr(local, 1, 10), substr(local, 12, 2), SUM(ms_played) FROM ev WHERE {} GROUP BY 1, 2",
        Src::All.cte(cx.include_private),
        between(s)
    ))?;
    for r in st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (d, hour, ms) = r?;
        if let (Some(d), Ok(hour)) = (super::v3::parse_day(&d), hour.parse::<usize>())
            && hour < 24
        {
            h[hour] += ms;
            w[d.weekday().num_days_from_monday() as usize] += ms;
        }
    }
    Ok((h, w))
}

/// Plays per window of tracks (`norm_key`) played before the window started (fingerprint Repeat).
/// Windows don't overlap. One grouped pass per (window, track), then one probe per track for a
/// play before the first window (NFR-04: per track, not per play).
pub(crate) fn repeat_plays<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    src: Src,
    spans: &[Span],
) -> Result<Vec<i64>> {
    let mut out = vec![0; spans.len()];
    if spans.is_empty() {
        return Ok(out);
    }
    let values = spans
        .iter()
        .enumerate()
        .map(|(i, s)| format!("({i}, '{}', '{}')", day(s.0), day(s.1)))
        .collect::<Vec<_>>()
        .join(", ");
    let lo = spans.iter().map(|s| s.0).min().expect("non-empty");
    let hi = spans.iter().map(|s| s.1).max().expect("non-empty");
    let mut st = cx.conn.prepare(&format!(
        "{}, sp(i, a, b) AS (VALUES {values})
         SELECT sp.i, ev.norm_key, COUNT(*) FROM ev JOIN sp ON substr(ev.local, 1, 10) BETWEEN sp.a AND sp.b
         WHERE {} AND ev.counts_as_play AND ev.norm_key <> '' GROUP BY 1, 2",
        src.cte(cx.include_private),
        between((lo, hi))
    ))?;
    let mut by_key: HashMap<String, Vec<(usize, i64)>> = HashMap::new();
    for r in st.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (i, k, n) = r?;
        by_key.entry(k).or_default().push((i as usize, n));
    }
    let mut before = cx.conn.prepare(&format!(
        "SELECT EXISTS(SELECT 1 FROM listening_event WHERE norm_key = ?1 AND ended_at_utc < ?2
           AND counts_as_play AND reconciled_with IS NULL{})",
        cx.private_filter("private_session")
    ))?;
    let lo_s = day(lo);
    for (k, mut v) in by_key {
        v.sort_by_key(|(i, _)| spans[*i].0);
        let mut known: bool = before.query_row([&k, &lo_s], |r| r.get(0))?;
        for (i, n) in v {
            if known {
                out[i] += n;
            }
            known = true;
        }
    }
    Ok(out)
}

/// Group key and name columns of one kind (as `listening::rank` groups).
fn kind_sql(kind: Kind) -> (&'static str, &'static str, &'static str) {
    match kind {
        Kind::Artist => ("lower(artist_name)", "MAX(artist_name)", "''"),
        Kind::Album => (
            "lower(album_name)",
            "MAX(album_name)",
            "IFNULL(MAX(artist_name), '')",
        ),
        Kind::Track => (
            "norm_key",
            "MAX(track_name)",
            "IFNULL(MAX(artist_name), '')",
        ),
    }
}

/// A row's identity across rankings.
fn row_key(kind: Kind, name: &str, by: &str) -> String {
    match kind {
        Kind::Track => format!(
            "{}\u{1f}{}",
            name.to_ascii_lowercase(),
            by.to_ascii_lowercase()
        ),
        _ => name.to_ascii_lowercase(),
    }
}

/// `(ms, key)` rows before ranking.
type Ranked = Vec<(i64, String)>;

/// Like-for-like rankings (top `MOVE_DEPTH` keys by time) of `cur` and `prev` in one pass.
fn rankings<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    cov: &Coverage,
    kind: Kind,
    cur: Option<Span>,
    prev: Span,
) -> Result<(Vec<String>, Vec<String>)> {
    let (group, name, by) = kind_sql(kind);
    // no `cur`: only the comparison window is ranked (the caller has the current order)
    let (c, p) = (cur.map_or("0".into(), between), between(prev));
    // the hull of both windows bounds the scan on the time index (an OR of two ranges scans all)
    let hull = cur.map_or(prev, |c| (c.0.min(prev.0), c.1.max(prev.1)));
    let sql = format!(
        "{} SELECT {name}, {by}, SUM(CASE WHEN {c} THEN ms_played END), SUM(CASE WHEN {p} THEN ms_played END)
         FROM ev WHERE {} AND (({c}) OR ({p})) GROUP BY +{group} HAVING {name} <> ''",
        Src::Like(cov).cte(cx.include_private),
        between(hull)
    );
    let mut st = cx.conn.prepare(&sql)?;
    let (mut a, mut b): (Ranked, Ranked) = (vec![], vec![]);
    for r in st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<i64>>(2)?,
            r.get::<_, Option<i64>>(3)?,
        ))
    })? {
        let (n, by, x, y) = r?;
        let k = row_key(kind, &n, &by);
        if let Some(x) = x.filter(|x| *x > 0) {
            a.push((x, k.clone()));
        }
        if let Some(y) = y.filter(|y| *y > 0) {
            b.push((y, k));
        }
    }
    let top = |mut v: Vec<(i64, String)>| {
        v.sort_by(|x, y| y.0.cmp(&x.0).then_with(|| x.1.cmp(&y.1)));
        v.into_iter().take(MOVE_DEPTH).map(|x| x.1).collect()
    };
    Ok((top(a), top(b)))
}

/// Hours per month for the rows' names over `months` (the last 12), like `listening::rank` groups.
fn sparks<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    kind: Kind,
    rows: &[RankRow],
    ms: &[NaiveDate],
) -> Result<Vec<Vec<f64>>> {
    let mut out = vec![vec![0.0; ms.len()]; rows.len()];
    if rows.is_empty() || ms.is_empty() {
        return Ok(out);
    }
    let span = (ms[0], super::v3::month_end(*ms.last().expect("non-empty")));
    let names: Vec<String> = rows.iter().map(|r| r.name.to_ascii_lowercase()).collect();
    let marks = vec!["?"; names.len()].join(", ");
    let sql = match kind {
        Kind::Artist => format!(
            "SELECT artist, '', month, SUM(ms_played) FROM listening_month_artist
             WHERE month BETWEEN '{}' AND '{}' AND events > 0 AND artist IN ({marks}){} GROUP BY 1, 3",
            month(span.0),
            month(span.1),
            cx.private_filter("private_session")
        ),
        _ => {
            let (col, by) = match kind {
                Kind::Album => ("lower(album_name)", "''"),
                _ => ("lower(track_name)", "lower(IFNULL(artist_name, ''))"),
            };
            format!(
                "{} SELECT {col}, {by}, substr(local, 1, 7), SUM(ms_played) FROM ev
                 WHERE {} AND {col} IN ({marks}) GROUP BY 1, 2, 3",
                Src::All.cte(cx.include_private),
                between(span)
            )
        }
    };
    let idx: HashMap<String, usize> = ms.iter().enumerate().map(|(i, m)| (month(*m), i)).collect();
    let want: HashMap<String, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (row_key(kind, &r.name, &r.by), i))
        .collect();
    let mut st = cx.conn.prepare(&sql)?;
    let params: Vec<&dyn ToSql> = names.iter().map(|n| n as &dyn ToSql).collect();
    for r in st.query_map(params_from_iter(params), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })? {
        let (n, by, m, v) = r?;
        let key = match kind {
            Kind::Track => row_key(kind, &n, &by),
            _ => n,
        };
        if let (Some(&i), Some(&j)) = (want.get(&key), idx.get(&m)) {
            out[i][j] += v as f64;
        }
    }
    for v in out.iter_mut().flatten() {
        *v = super::hours(*v as i64);
    }
    Ok(out)
}

/// The rows of one Most played tab with their moves and sparklines.
#[allow(clippy::too_many_arguments)]
fn most_played<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    r: &Resolved,
    cov: &Coverage,
    kind: Kind,
    all_time: bool,
    prev: Option<Span>,
    spark_months: &[NaiveDate],
) -> Result<Vec<TopRow>> {
    let rows = if all_time {
        rank(
            cx.conn,
            &rollup(cx.include_private),
            kind,
            "?1 = ''",
            &"",
            TOP,
        )?
    } else {
        let filter = format!("{} AND ?1 = ''", between(r.cur));
        rank(cx.conn, &base(cx.include_private), kind, &filter, &"", TOP)?
    };
    // Like for like equal to the base view (`is_base`): the displayed rows are the current order.
    let base_like = Src::Like(cov).is_base();
    let ranked = match prev {
        Some(p) if cov.no_compare_reason.is_none() => {
            let (now, before) = rankings(cx, cov, kind, (!base_like).then_some(r.cur), p)?;
            let now = if base_like {
                rows.iter().map(|x| row_key(kind, &x.name, &x.by)).collect()
            } else {
                now
            };
            Some((now, before))
        }
        _ => None,
    };
    let sparks = sparks(cx, kind, &rows, spark_months)?;
    Ok(rows
        .into_iter()
        .zip(sparks)
        .map(|(row, spark)| {
            let k = row_key(kind, &row.name, &row.by);
            let (mv, is_new) = match &ranked {
                None => (None, false),
                Some((now, before)) => {
                    let at = |v: &Vec<String>| v.iter().position(|x| *x == k).map(|i| i as i64);
                    match (at(now), at(before)) {
                        (Some(n), Some(b)) => (Some(b - n), false),
                        (_, None) => (None, true),
                        _ => (None, false),
                    }
                }
            };
            TopRow {
                row,
                r#move: mv,
                is_new,
                spark,
            }
        })
        .collect())
}

/// An artist that left the top 10: in it each of the ≥ 6 months before the period (consecutive,
/// ending the month before) and not in the period's top 10. The longest such run wins.
fn left_top10<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    start: NaiveDate,
    top_now: &HashSet<String>,
) -> Result<Option<(String, usize)>> {
    let to = start.with_day(1).expect("day 1") - TimeDelta::days(1);
    let from = (to.with_day(1).expect("day 1")) - Months::new(23);
    // each month's top 10 in SQL (a window), so only those rows load
    let mut st = cx.conn.prepare(&format!(
        "SELECT month, artist FROM (SELECT month, artist, ROW_NUMBER() OVER (PARTITION BY month
           ORDER BY ms DESC, artist) AS k FROM (SELECT month, artist, SUM(ms_played) AS ms
           FROM listening_month_artist WHERE month BETWEEN '{}' AND '{}' AND events > 0 AND artist <> ''{}
           GROUP BY 1, 2)) WHERE k <= {LINE_LEFT_TOP}",
        month(from),
        month(to),
        cx.private_filter("private_session")
    ))?;
    let mut by: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (m, a) = r?;
        by.entry(m).or_default().insert(a);
    }
    let tops: Vec<HashSet<String>> = months(from, to)
        .into_iter()
        .rev()
        .map(|m| by.remove(&month(m)).unwrap_or_default())
        .collect();
    let Some(last) = tops.first() else {
        return Ok(None);
    };
    let best = last
        .iter()
        .filter(|a| !top_now.contains(*a))
        .map(|a| (tops.iter().take_while(|t| t.contains(a)).count(), a))
        .max_by(|x, y| x.0.cmp(&y.0).then_with(|| y.1.cmp(x.1)));
    match best {
        Some((n, a)) if n >= LINE_LEFT_MONTHS => Ok(Some((artist_name(cx.conn, a)?.0, n))),
        _ => Ok(None),
    }
}

/// The biggest day ever over all-device dates, `(date, ms)`.
fn biggest_day<Tz: TimeZone>(cx: &Ctx<Tz>, model: &Model) -> Result<Option<(NaiveDate, i64)>> {
    let Some((a, b)) = model.export else {
        return Ok(None);
    };
    let row: Option<(String, i64)> = rusqlite::OptionalExtension::optional(cx.conn.query_row(
        &format!(
            "SELECT day, SUM(ms_played) AS ms FROM listening_day WHERE day BETWEEN '{}' AND '{}'
             AND events > 0{} GROUP BY day ORDER BY ms DESC, day LIMIT 1",
            day(a),
            day(b),
            cx.private_filter("private_session")
        ),
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ))?;
    Ok(row.and_then(|(d, ms)| super::v3::parse_day(&d).map(|d| (d, ms))))
}

/// New artists (first play in all stored data in the window): the period, and like for like
/// (counted when that first play is from a compared source) now and in the comparison windows.
pub(crate) fn new_artists_compared(
    heard: &[FirstHeard],
    cov: &Coverage,
    cur: Span,
    vs: &[Span],
) -> Compared {
    let allowed = |f: &FirstHeard| match cov.compared_sources.as_slice() {
        ["S"] => f.source == Source::Logger,
        ["E"] => f.source == Source::Export,
        _ => true,
    };
    let count = |s: Span, like: bool| {
        heard
            .iter()
            .filter(|f| (s.0..=s.1).contains(&f.local.date()) && (!like || allowed(f)))
            .count() as f64
    };
    compared(
        count(cur, false),
        cov.no_compare_reason.is_none().then(|| {
            (
                count(cur, true),
                vs.iter().map(|s| count(*s, true)).collect(),
            )
        }),
        round0,
    )
}

/// `get_period_report`.
pub fn report<Tz: TimeZone>(cx: &Ctx<Tz>, q: &PeriodQuery) -> Result<Covered<PeriodReport>> {
    let mut __t = std::time::Instant::now();
    let today = cx.today();
    let first = coverage::first_day(cx.conn)?;
    let r = resolve(q, today, first)?;
    let (model, cov) = period_coverage(cx, &r, STREAK_BACK_DAYS + 31)?;
    let hidden = cov.no_compare_reason.is_some();
    let on_pc = cov.desktop_only_since.is_some();
    let end = r.cur.1.min(today);
    let shown = (r.cur.0, end);
    let vs: Vec<Span> = r.vs.clone().unwrap_or_default();
    let all_time = r.grain == Grain::All;

    super::v3::PROFLAP(&mut __t, "// Basic counts: the per");
    // Basic counts: the period, then like for like (FR-048).
    let like = Src::Like(&cov);
    let (all, like_aggs): (Agg, Option<(Agg, Vec<Agg>)>) = if all_time {
        (super::v3::aggs_all(cx)?, None)
    } else if hidden {
        (aggs(cx, Src::All, &[r.cur])?[0], None)
    } else {
        let spans: Vec<Span> = std::iter::once(r.cur).chain(vs.iter().copied()).collect();
        let v = aggs(cx, like, &spans)?;
        let all = if like.is_base() {
            v[0]
        } else {
            aggs(cx, Src::All, &[r.cur])?[0]
        };
        (all, Some((v[0], v[1..].to_vec())))
    };
    let pick = |f: fn(&Agg) -> f64| {
        like_aggs
            .as_ref()
            .map(|(n, b)| (f(n), b.iter().map(f).collect()))
    };
    let c = |f: fn(&Agg) -> f64, round: fn(f64) -> f64| compared(f(&all), pick(f), round);

    super::v3::PROFLAP(&mut __t, "// New artists (first pl");
    // New artists (first play in all stored data in the window).
    let heard = terms::first_heard(cx.conn, cx.include_private)?;
    let new_artists = new_artists_compared(&heard, &cov, r.cur, &vs);

    super::v3::PROFLAP(&mut __t, "// Days listened, streak");
    // Days listened, streak and covered days: this PC only past the export (design §4.4).
    let basis = if on_pc { Src::Desktop } else { Src::All };
    let class = |d: NaiveDate| {
        if !on_pc {
            model.class(d)
        } else if model.ran(d) {
            CoverageClass::ThisPc
        } else {
            CoverageClass::NoData
        }
    };
    let look = end - TimeDelta::days(STREAK_BACK_DAYS);
    let basis_days = daily(cx, basis, (look.min(r.cur.0), end))?;
    let played_days = played(&basis_days);
    let streak = if end >= r.cur.0 {
        streaks(look, end, &played_days, class).1
    } else {
        0
    };
    let covered_days = shown
        .0
        .iter_days()
        .take_while(|d| *d <= shown.1)
        .filter(|d| class(*d) != CoverageClass::NoData)
        .count() as i64;
    let days_listened_now = played_days.range(shown.0..=shown.1.max(shown.0)).count() as i64;
    let mut days_listened = c(|a| a.days as f64, round0);
    if on_pc {
        days_listened.value = days_listened_now as f64;
    }
    let finished_pct = all.finished_pct().map(|v| {
        let e = cov.compared_sources.contains(&"E");
        let like = like_aggs.as_ref().filter(|_| e).and_then(|(n, b)| {
            Some((
                n.finished_pct()?,
                b.iter().filter_map(Agg::finished_pct).collect(),
            ))
        });
        compared(v, like, round1)
    });

    super::v3::PROFLAP(&mut __t, "// Timeline: day by day");
    // Timeline: day by day for weeks, months and short custom ranges, else month by month.
    let by_day = matches!(r.grain, Grain::Week | Grain::Month)
        || (r.grain == Grain::Custom && (r.cur.1 - r.cur.0).num_days() < DAILY_MAX_DAYS);
    let tl_src = if hidden { Src::All } else { like };
    let cur_days = if end >= r.cur.0 {
        daily(cx, tl_src, shown)?
    } else {
        BTreeMap::new()
    };
    let vs_days = match (vs.iter().map(|s| s.0).min(), vs.iter().map(|s| s.1).max()) {
        (Some(a), Some(b)) if !hidden => daily(cx, like, (a, b))?,
        _ => BTreeMap::new(),
    };
    let sum = |m: &BTreeMap<NaiveDate, (i64, i64)>, a: NaiveDate, b: NaiveDate| -> i64 {
        m.range(a..=b.max(a))
            .filter(|(d, _)| **d <= b)
            .map(|(_, v)| v.0)
            .sum()
    };
    let mean = |xs: Vec<i64>| {
        (!xs.is_empty()).then(|| (xs.iter().sum::<i64>() as f64 / xs.len() as f64).round() as i64)
    };
    let timeline: Vec<Point> = if end < r.cur.0 {
        vec![]
    } else if by_day {
        shown
            .0
            .iter_days()
            .take_while(|d| *d <= end)
            .enumerate()
            .map(|(i, d)| Point {
                key: day(d),
                ms: cur_days.get(&d).map_or(0, |v| v.0),
                before_ms: if hidden {
                    None
                } else {
                    mean(
                        vs.iter()
                            .map(|s| s.0 + TimeDelta::days(i as i64))
                            .zip(vs.iter())
                            .filter(|(x, s)| *x <= s.1)
                            .map(|(x, _)| vs_days.get(&x).map_or(0, |v| v.0))
                            .collect(),
                    )
                },
                coverage: model.class(d),
            })
            .collect()
    } else {
        months(shown.0, end)
            .into_iter()
            .enumerate()
            .map(|(i, m)| {
                let (a, b) = (m.max(shown.0), super::v3::month_end(m).min(end));
                Point {
                    key: month(m),
                    ms: sum(&cur_days, a, b),
                    before_ms: if hidden {
                        None
                    } else {
                        mean(
                            vs.iter()
                                .filter_map(|s| {
                                    let vm = months(s.0, s.1).get(i).copied()?;
                                    Some(sum(
                                        &vs_days,
                                        vm.max(s.0),
                                        super::v3::month_end(vm).min(s.1),
                                    ))
                                })
                                .collect(),
                        )
                    },
                    coverage: majority(a.iter_days().take_while(|d| *d <= b), |d| model.class(d)),
                }
            })
            .collect()
    };

    super::v3::PROFLAP(&mut __t, "// When you listened:");
    // When you listened: the period vs the last 6 months.
    let (hours, weekdays) = clock_ms(cx, r.cur, all_time)?;
    let base_span = (end - Months::new(6) + TimeDelta::days(1), end);
    let (baseline, _) = clock_ms(cx, base_span, false)?;
    let busiest = hours
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(&a.0)))
        .filter(|(_, ms)| **ms > 0)
        .map(|(h, ms)| (h, pct(*ms, hours.iter().sum())));

    super::v3::PROFLAP(&mut __t, "// Artists of the period");
    // Artists of the period, new ones first heard in it.
    let artists = by_artist(cx, Src::All, r.cur, first)?;
    let mut ranked: Vec<(&String, &(i64, i64))> = artists.iter().collect();
    ranked.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.0.cmp(b.0)));
    let new_here: HashMap<&str, &FirstHeard> = heard
        .iter()
        .filter(|f| (r.cur.0..=r.cur.1).contains(&f.local.date()))
        .map(|f| (f.artist.as_str(), f))
        .collect();
    let new_ranked: Vec<(usize, &String, &(i64, i64))> = ranked
        .iter()
        .enumerate()
        .filter(|(_, (k, _))| new_here.contains_key(k.as_str()))
        .map(|(i, (k, v))| (i + 1, *k, *v))
        .collect();
    let top_new = match new_ranked.first() {
        Some((rank, k, (ms, plays))) => {
            let f = new_here[k.as_str()];
            let (item_id, status) = artist_item(cx.conn, &f.name)?;
            Some(TopNew {
                name: f.name.clone(),
                item_id,
                status,
                first_played_local: minute(&f.local.format("%Y-%m-%dT%H:%M:%S").to_string()),
                first_track: f.track.clone(),
                ms: *ms,
                plays: *plays,
                rank: *rank,
                share_pct: pct(*ms, all.ms),
            })
        }
        None => None,
    };
    let also_new: Vec<String> = new_ranked
        .iter()
        .skip(1)
        .take(3)
        .map(|(_, k, _)| new_here[k.as_str()].name.clone())
        .collect();
    let also_new_more = new_ranked.len().saturating_sub(1 + also_new.len());

    super::v3::PROFLAP(&mut __t, "// Fingerprint now");
    // Fingerprint now (basis as days listened) and the 12-month average.
    let basis_artists;
    let (fp_artists, fp_agg) = if on_pc {
        basis_artists = by_artist(cx, basis, r.cur, first)?;
        (&basis_artists, aggs(cx, basis, &[r.cur])?[0])
    } else {
        (&artists, all)
    };
    let new_keys: HashSet<&str> = new_here.keys().copied().collect();
    let parts = FpParts {
        ms: fp_agg.ms,
        artist_ms: fp_artists.values().map(|v| v.0).collect(),
        new_ms: fp_artists
            .iter()
            .filter(|(k, _)| new_keys.contains(k.as_str()))
            .map(|(_, v)| v.0)
            .sum(),
        days_listened: days_listened_now,
        covered_days,
        plays: fp_agg.plays,
        repeat_plays: if first.is_some_and(|f| f < r.cur.0) {
            repeat_plays(cx, basis, &[r.cur])?[0]
        } else {
            0
        },
    };
    super::v3::PROFLAP(&mut __t, "let average = average_fi");
    let average = average_fingerprint(cx, &model, &heard, r.cur.0, end)?;

    super::v3::PROFLAP(&mut __t, "// Lines (design §5.1).");
    // Lines (design §5.1).
    let top_names: Vec<(String, Option<NaiveDate>)> = ranked
        .iter()
        .take(LINE_NEW_TOP)
        .map(|(k, _)| match new_here.get(k.as_str()) {
            Some(f) => Ok((f.name.clone(), Some(f.local.date()))),
            None => Ok((artist_name(cx.conn, k)?.0, None)),
        })
        .collect::<Result<_>>()?;
    let top10: HashSet<String> = ranked
        .iter()
        .take(LINE_LEFT_TOP)
        .map(|(k, _)| (*k).clone())
        .collect();
    let record_day = biggest_day(cx, &model)?.filter(|(d, _)| (shown.0..=shown.1).contains(d));
    let ms_c = c(|a| a.ms as f64, round0);
    let facts = LineFacts {
        ms_now_before: ms_c.now.zip(ms_c.before),
        vs_label: vs_label(r.grain, r.vs_kind, r.single_vs()),
        on_pc,
        top: top_names,
        left: if all_time || ranked.is_empty() {
            None
        } else {
            left_top10(cx, r.cur.0, &top10)?
        },
        busiest,
        days_listened: days_listened_now,
        covered_days,
        streak,
        record_day,
    };

    super::v3::PROFLAP(&mut __t, "let (tag_rows, tag_cov) ");
    let (tag_rows, tag_cov) = if all_time {
        top_tags_all(cx)?
    } else {
        terms::top_tags(cx.conn, cx.include_private, r.cur.0, r.cur.1, 6)?
    };
    super::v3::PROFLAP(&mut __t, "let prev = r.single_vs()");
    let prev = r.single_vs().or_else(|| vs.first().copied());
    let spark_months = months(month_start_minus(end, 11), end);
    let most = |k| most_played(cx, &r, &cov, k, all_time, prev, &spark_months);
    super::v3::PROFLAP(&mut __t, "let private_excluded = i");
    let private_excluded = if cx.include_private {
        0
    } else {
        cx.conn.query_row(
            &format!(
                "SELECT IFNULL(SUM(plays), 0) FROM listening_day WHERE private_session = 1
                 AND day BETWEEN '{}' AND '{}'",
                day(r.cur.0),
                day(r.cur.1)
            ),
            [],
            |row| row.get(0),
        )?
    };
    let (prev_at, next_at) = r.steps(today, first);
    let single = r.single_vs();
    let data = PeriodReport {
        period: PeriodInfo {
            grain: r.grain,
            vs: r.vs_kind,
            from: day(r.cur.0),
            to: day(r.cur.1),
            vs_from: single.map(|s| day(s.0)),
            vs_to: single.map(|s| day(s.1)),
            prev_at,
            next_at,
            days: covered_days,
        },
        private_excluded,
        tiles: Tiles {
            ms: ms_c,
            plays: c(|a| a.plays as f64, round0),
            artists: c(|a| a.artists as f64, round0),
            new_artists,
            days_listened,
            streak,
            finished_pct,
        },
        timeline,
        lines: lines(&facts),
        clock: Clock {
            hours: shares(&hours),
            baseline: shares(&baseline),
            weekdays: shares(&weekdays),
        },
        top_new,
        also_new,
        also_new_more,
        ratio: RatioCards {
            tracks: c(|a| a.tracks as f64, round0),
            albums: c(|a| a.albums as f64, round0),
            artists: c(|a| a.artists as f64, round0),
            tracks_per_artist: c(Agg::tracks_per_artist, round1),
            albums_per_artist: c(Agg::albums_per_artist, round1),
        },
        fingerprint: Fingerprints {
            now: fingerprint_of(&parts),
            average,
        },
        tags: Tags {
            rows: tag_rows,
            covered_pct: tag_cov,
        },
        most_played: MostPlayed {
            artists: most(Kind::Artist)?,
            albums: most(Kind::Album)?,
            tracks: most(Kind::Track)?,
        },
    };
    super::v3::PROFLAP(&mut __t, "covered(cx, data, cov, t");
    covered(cx, data, cov, true)
}

/// `terms::top_tags` over all time from the 0005 rollup (per record, then its links and tags).
fn top_tags_all<Tz: TimeZone>(cx: &Ctx<Tz>) -> Result<(Vec<TagShare>, f64)> {
    let p = cx.private_filter("private_session");
    let rows = format!(
        "WITH r AS (SELECT item_id AS iid, SUM(ms_played) AS ms FROM listening_rollup
                    WHERE events > 0 AND item_id <> 0{p} GROUP BY 1){},
              rt AS (SELECT r.iid, r.ms, t.name FROM r JOIN src ON src.s = r.iid
                     JOIN item_tag it ON it.item_id = src.t JOIN tag t ON t.id = it.tag_id GROUP BY r.iid, t.id)",
        super::SRC
    );
    let total: i64 = cx.conn.query_row(
        &format!("SELECT IFNULL(SUM(ms_played), 0) FROM listening_rollup WHERE events > 0{p}"),
        [],
        |r| r.get(0),
    )?;
    let tagged: i64 = cx.conn.query_row(
        &format!("{rows} SELECT IFNULL(SUM(ms), 0) FROM (SELECT DISTINCT iid, ms FROM rt)"),
        [],
        |r| r.get(0),
    )?;
    let mut st = cx.conn.prepare(&format!(
        "{rows} SELECT name, SUM(ms) AS ms FROM rt GROUP BY name ORDER BY ms DESC, name LIMIT 6"
    ))?;
    let out = st
        .query_map([], |r| {
            Ok(TagShare {
                tag: r.get(0)?,
                share_pct: pct(r.get(1)?, tagged),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok((out, pct(tagged, total)))
}

/// First day of the month `n` months before `d`'s.
fn month_start_minus(d: NaiveDate, n: u32) -> NaiveDate {
    super::v3::month_start(d) - Months::new(n)
}

/// Your own 12-month average: the mean of each measure over the 12 calendar months before the
/// period's first month (months without listening left out); `None` when there are none.
fn average_fingerprint<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    model: &Model,
    heard: &[FirstHeard],
    start: NaiveDate,
    today: NaiveDate,
) -> Result<Option<Fingerprint>> {
    let to = super::v3::month_start(start) - TimeDelta::days(1);
    let from = month_start_minus(to, 11);
    if from > today {
        return Ok(None);
    }
    let ms = months(from, to);
    let spans: Vec<Span> = ms.iter().map(|m| (*m, super::v3::month_end(*m))).collect();
    let mut st = cx.conn.prepare(&format!(
        "SELECT month, artist, SUM(ms_played) FROM listening_month_artist
         WHERE month BETWEEN '{}' AND '{}' AND events > 0 AND artist <> ''{} GROUP BY 1, 2",
        month(from),
        month(to),
        cx.private_filter("private_session")
    ))?;
    let mut artists: HashMap<String, Vec<(String, i64)>> = HashMap::new();
    for r in st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (m, a, v) = r?;
        artists.entry(m).or_default().push((a, v));
    }
    let days = daily(cx, Src::All, (from, to))?;
    let first_month: HashMap<&str, String> = heard
        .iter()
        .map(|f| (f.artist.as_str(), f.local.format("%Y-%m").to_string()))
        .collect();
    let repeats = repeat_plays(cx, Src::All, &spans)?;
    let mut fps = vec![];
    for (i, (a, b)) in spans.iter().enumerate() {
        let m = month(*a);
        let rows = artists.remove(&m).unwrap_or_default();
        let in_month = days.range(*a..=*b);
        let (total, plays, listened) = in_month.fold((0, 0, 0), |(t, p, l), (_, v)| {
            (t + v.0, p + v.1, l + (v.1 > 0) as i64)
        });
        if total == 0 {
            continue;
        }
        fps.push(fingerprint_of(&FpParts {
            ms: total,
            new_ms: rows
                .iter()
                .filter(|(k, _)| first_month.get(k.as_str()) == Some(&m))
                .map(|r| r.1)
                .sum(),
            artist_ms: rows.iter().map(|r| r.1).collect(),
            days_listened: listened,
            covered_days: a
                .iter_days()
                .take_while(|d| d <= b)
                .filter(|d| model.class(*d) != CoverageClass::NoData)
                .count() as i64,
            plays,
            repeat_plays: repeats[i],
        }));
    }
    if fps.is_empty() {
        return Ok(None);
    }
    let n = fps.len() as f64;
    let avg = |f: fn(&Fingerprint) -> f64| (fps.iter().map(f).sum::<f64>() / n).round();
    Ok(Some(Fingerprint {
        regularity: avg(|f| f.regularity),
        discovery: avg(|f| f.discovery),
        focus: avg(|f| f.focus),
        variety: avg(|f| f.variety),
        repeat: avg(|f| f.repeat),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn facts() -> LineFacts {
        LineFacts {
            ms_now_before: Some((110.0, 100.0)),
            vs_label: "September".into(),
            top: vec![
                ("Aster".into(), None),
                ("Birch".into(), Some(d("2026-10-04"))),
            ],
            left: Some(("Cedar".into(), 6)),
            busiest: Some((22, 30.0)),
            days_listened: 26,
            covered_days: 30,
            streak: 14,
            record_day: Some((d("2026-10-12"), 33_120_000)),
            ..Default::default()
        }
    }

    fn text(s: &Sentence) -> String {
        s.parts.iter().map(|p| p.text.as_str()).collect()
    }

    #[test]
    fn lines_fire_at_their_thresholds_in_rule_order() {
        let all = lines(&LineFacts {
            ms_now_before: None,
            ..facts()
        });
        // without the time change, rules 2–5 fill the four lines; the record (6) is cut
        assert_eq!(
            all.iter().map(|s| s.rule).collect::<Vec<_>>(),
            vec![
                Rule::NewInTop5,
                Rule::LeftTop10,
                Rule::BusiestHour,
                Rule::Streak
            ]
        );
        assert_eq!(
            text(&all[0]),
            "Birch is new and already your #2 artist. You first played them on 4 Oct 2026."
        );
        assert_eq!(
            text(&all[1]),
            "Cedar left your top 10 after 6 months in it."
        );
        assert_eq!(
            text(&all[2]),
            "Your busiest hour was 22:00: 30% of your time."
        );
        assert_eq!(
            text(&all[3]),
            "You listened on 26 of 30 days. Your current streak is 14 days."
        );
        assert!(all[0].parts[0].em && !all[0].parts[1].em);
        let l = lines(&facts());
        assert_eq!(l.len(), LINES_MAX);
        assert_eq!(text(&l[0]), "You listened 10% more than September.");
        // each rule just under its threshold: nothing but the record
        let quiet = lines(&LineFacts {
            ms_now_before: Some((109.0, 100.0)),
            top: vec![("Aster".into(), None); 5]
                .into_iter()
                .chain([("Late".into(), Some(d("2026-10-04")))])
                .collect(),
            left: Some(("Cedar".into(), 5)),
            busiest: Some((22, 29.9)),
            streak: 13,
            on_pc: true,
            ..facts()
        });
        assert_eq!(
            quiet.iter().map(|s| s.rule).collect::<Vec<_>>(),
            vec![Rule::Record]
        );
        assert_eq!(
            text(&quiet[0]),
            "12 Oct 2026 was your biggest day ever: 9 h 12 min."
        );
        // less, and on this PC
        let less = lines(&LineFacts {
            ms_now_before: Some((80.0, 100.0)),
            on_pc: true,
            ..facts()
        });
        assert_eq!(
            text(&less[0]),
            "You listened 20% less on this PC than September."
        );
        // no comparison to divide by
        assert!(
            lines(&LineFacts {
                ms_now_before: Some((5.0, 0.0)),
                ..facts()
            })[0]
                .rule
                != Rule::TimeChange
        );
    }

    #[test]
    fn labels_and_durations() {
        let s = Some((d("2026-09-01"), d("2026-09-30")));
        assert_eq!(vs_label(Grain::Month, Compare::Prev, s), "September");
        assert_eq!(
            vs_label(Grain::Month, Compare::LastYear, s),
            "September 2026"
        );
        assert_eq!(
            vs_label(Grain::Week, Compare::Usual, None),
            "your usual week"
        );
        assert_eq!(
            vs_label(Grain::Custom, Compare::Prev, s),
            "the stretch before"
        );
        assert_eq!(
            vs_label(Grain::Week, Compare::LastYear, s),
            "the same time last year"
        );
        assert_eq!(
            vs_label(
                Grain::Year,
                Compare::Prev,
                Some((d("2025-01-01"), d("2025-12-31")))
            ),
            "2025"
        );
        assert_eq!(
            (hm(45 * 60_000), hm(2 * 3_600_000), hm(33_120_000)),
            ("45 min".into(), "2 h".into(), "9 h 12 min".into())
        );
        assert_eq!(
            compared(5.0, Some((4.0, vec![1.0, 3.0, 10.0, 2.0])), round0),
            Compared {
                value: 5.0,
                now: Some(4.0),
                before: Some(3.0)
            } // median of 1, 2, 3, 10 = 2.5 → 3
        );
    }
}
