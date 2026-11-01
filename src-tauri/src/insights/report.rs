//! `get_period_report` (API.md §4.1, design §5 Report tab and §5.1 "the period in a few lines"):
//! every Report card for one period of the page header, compared like for like (FR-048).
use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, Months, NaiveDate, TimeDelta, TimeZone};
use rusqlite::{Connection, ToSql, params_from_iter};
use serde::Serialize;

use super::coverage::{self, Coverage, Model};
use super::listening::{Kind, RankRow, rank};
use super::terms::{
    self, Compare, CoverageClass, Fingerprint, FirstHeard, FpParts, Span, TagShare, fingerprint_of,
    median, streaks,
};
use super::v3::{
    Agg, AggAcc, ArtistAcc, Aside, Ctx, Grain, IdMap, PeriodQuery, Resolved, Src, artist_item,
    artist_name, between, by_artist, by_artist_rollup, covered, daily, day, majority, minute,
    month, months, period_coverage, played, resolve, show,
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
    let mut acc = ClockAcc::new(s);
    super::v3::scan_spans(cx, &[s], |p| acc.add(p))?;
    Ok(acc.finish())
}

/// `terms::top_tags` of a period as a `scan` figure: base-view time per record (`None` = none).
#[derive(Clone)]
struct TagAcc {
    span: Span,
    bounds: (u32, u32),
    per: HashMap<Option<i64>, i64>,
}

impl TagAcc {
    fn new(s: Span) -> Self {
        TagAcc {
            span: s,
            bounds: (super::v3::num(s.0), super::v3::num(s.1)),
            per: HashMap::new(),
        }
    }

    fn add(&mut self, p: &super::v3::Pass) -> Result<()> {
        if p.base && self.bounds.0 <= p.day && p.day <= self.bounds.1 {
            *self.per.entry(p.item_id()?).or_default() += p.ms;
        }
        Ok(())
    }

    fn merge(&mut self, o: TagAcc) {
        for (k, v) in o.per {
            *self.per.entry(k).or_default() += v;
        }
    }
}

/// `clock_ms` over the events as a `scan` figure: base-view time per local date and hour (the
/// hour from the time text, no strftime per event, NFR-04); the weekday comes from the date.
#[derive(Clone)]
struct ClockAcc {
    span: Span,
    bounds: (u32, u32),
    by_day: IdMap<u32, [i64; 24]>,
}

impl ClockAcc {
    fn new(s: Span) -> Self {
        ClockAcc {
            span: s,
            bounds: (super::v3::num(s.0), super::v3::num(s.1)),
            by_day: IdMap::default(),
        }
    }

    fn add(&mut self, p: &super::v3::Pass) -> Result<()> {
        if !p.base || p.day < self.bounds.0 || p.day > self.bounds.1 {
            return Ok(());
        }
        if let Some(Ok(hour)) = p.local()?.get(11..13).map(str::parse::<usize>)
            && hour < 24
        {
            self.by_day.entry(p.day).or_insert([0; 24])[hour] += p.ms;
        }
        Ok(())
    }

    fn merge(&mut self, o: ClockAcc) {
        for (d, hs) in o.by_day {
            let e = self.by_day.entry(d).or_insert([0; 24]);
            for (a, b) in e.iter_mut().zip(hs) {
                *a += b;
            }
        }
    }

    fn finish(self) -> ([i64; 24], [i64; 7]) {
        let (mut h, mut w) = ([0i64; 24], [0i64; 7]);
        for (d, hs) in self.by_day {
            let Some(d) = NaiveDate::from_ymd_opt((d / 10_000) as i32, d / 100 % 100, d % 100)
            else {
                continue;
            };
            for (hour, ms) in hs.iter().enumerate() {
                h[hour] += ms;
                w[d.weekday().num_days_from_monday() as usize] += ms;
            }
        }
        (h, w)
    }
}

/// Plays per window of tracks (`norm_key`) played before the window started (fingerprint Repeat).
/// Windows don't overlap. `RepeatAcc` over its own pass.
#[cfg(test)]
pub(crate) fn repeat_plays<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    src: Src,
    spans: &[Span],
) -> Result<Vec<i64>> {
    let mut acc = RepeatAcc::new(src, spans);
    super::v3::scan_spans(cx, spans, |p| acc.add(p))?;
    acc.into_parts().count(cx.conn, cx.include_private)
}

/// `repeat_plays` as a `scan` figure: the windows' plays summed per track here (NFR-04: SQL's
/// GROUP BY would sort every play), then one probe per track for a play before the first window
/// (per track, not per play).
#[derive(Clone)]
pub(crate) struct RepeatAcc<'c> {
    src: Src<'c>,
    spans: Vec<Span>,
    bounds: Vec<(u32, u32)>,
    /// quick hash of the track key (retried on a collision) → the key and its plays per window
    by_key: IdMap<u64, (String, Vec<i64>)>,
}

impl<'c> RepeatAcc<'c> {
    pub(crate) fn new(src: Src<'c>, spans: &[Span]) -> Self {
        RepeatAcc {
            src,
            spans: spans.to_vec(),
            bounds: spans
                .iter()
                .map(|s| (super::v3::num(s.0), super::v3::num(s.1)))
                .collect(),
            by_key: IdMap::default(),
        }
    }

    pub(crate) fn add(&mut self, p: &super::v3::Pass) -> Result<()> {
        if !p.play || !self.src.has(p) {
            return Ok(());
        }
        let d = p.day;
        let Some(i) = self.bounds.iter().position(|(a, b)| *a <= d && d <= *b) else {
            return Ok(());
        };
        let Some(k) = p.norm_key()?.filter(|k| !k.is_empty()) else {
            return Ok(());
        };
        self.plays_of(k)[i] += 1;
        Ok(())
    }

    /// The plays per window of track `k`, added when new.
    fn plays_of(&mut self, k: &str) -> &mut Vec<i64> {
        let mut h = super::v3::quick_hash(&[Some(k)]);
        loop {
            match self.by_key.get(&h) {
                Some(e) if e.0 == k => break,
                Some(_) => h = h.wrapping_add(0x9e37_79b9_7f4a_7c15),
                None => {
                    self.by_key
                        .insert(h, (k.to_owned(), vec![0; self.spans.len()]));
                    break;
                }
            }
        }
        &mut self.by_key.get_mut(&h).expect("found or inserted").1
    }

    /// Adds another part's events (`scan_parts`).
    pub(crate) fn merge(&mut self, o: RepeatAcc) {
        for (k, v) in o.by_key.into_values() {
            for (a, b) in self.plays_of(&k).iter_mut().zip(v) {
                *a += b;
            }
        }
    }

    /// Its windows and tracks, owned (for `RepeatParts::count` on another connection).
    pub(crate) fn into_parts(self) -> RepeatParts {
        RepeatParts {
            spans: self.spans,
            tracks: self.by_key.into_values().collect(),
        }
    }
}

/// `RepeatAcc`'s windows and each track's plays per window, owned.
pub(crate) struct RepeatParts {
    spans: Vec<Span>,
    tracks: Vec<(String, Vec<i64>)>,
}

impl RepeatParts {
    /// Repeat plays per window: plays of tracks heard before (an earlier window or any play before
    /// the first one).
    pub(crate) fn count(self, conn: &Connection, include_private: bool) -> Result<Vec<i64>> {
        let mut out = vec![0; self.spans.len()];
        let Some(lo) = self.spans.iter().map(|s| s.0).min() else {
            return Ok(out);
        };
        let mut order: Vec<usize> = (0..self.spans.len()).collect();
        order.sort_by_key(|i| self.spans[*i].0);
        let mut before = conn.prepare(&format!(
            // unfiltered: base filters inline (reconciled out, private per setting); one seek on 0011's index
            "SELECT EXISTS(SELECT 1 FROM listening_event INDEXED BY listening_event_norm_time
               WHERE norm_key = ?1 AND ended_at_utc < ?2 AND counts_as_play AND reconciled_with IS NULL{})",
            if include_private {
                ""
            } else {
                " AND private_session = 0"
            }
        ))?;
        let lo_s = day(lo);
        // in key order, so the seeks walk the index once (NFR-04: one per track)
        let mut tracks = self.tracks;
        tracks.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        for (k, v) in tracks {
            let mut known: bool = before.query_row([&k, &lo_s], |r| r.get(0))?;
            for &i in &order {
                if v[i] > 0 {
                    if known {
                        out[i] += v[i];
                    }
                    known = true;
                }
            }
        }
        Ok(out)
    }
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

/// One Most played group while it's summed: what `listening::rank` / `rankings` compute in SQL.
#[derive(Default)]
struct Group {
    name: String,
    by: String,
    plays: i64,
    ms: i64,
    iid: Option<i64>,
}

impl Group {
    /// `MAX(name)`, `MAX(by)`, `MAX(item_id)` and the sums, as SQL aggregates them (BINARY order).
    fn add(&mut self, name: Option<&str>, by: Option<&str>, plays: i64, ms: i64, iid: Option<i64>) {
        if let Some(n) = name.filter(|n| *n > self.name.as_str()) {
            self.name = n.to_owned();
        }
        if let Some(b) = by.filter(|b| *b > self.by.as_str()) {
            self.by = b.to_owned();
        }
        self.plays += plays;
        self.ms += ms;
        self.iid = self.iid.max(iid);
    }
}

/// One distinct set of names, track key and record among a Most played pass's events, summed per
/// window: [shown (base, period), like for like in the period, before] as (events, plays, ms), and
/// ms per sparkline month (base). Most events repeat one, so the per-kind groups are built from
/// these (NFR-04: one hash per event instead of one per kind and window).
#[derive(Clone)]
struct Combo {
    artist: Option<String>,
    album: Option<String>,
    track: Option<String>,
    norm: Option<String>,
    iid: Option<i64>,
    sums: [(i64, i64, i64); 3],
    spark: Vec<i64>,
}

/// Most played of a period (not all time) for the three kinds as a `scan` figure: the period, the
/// compared window and the sparkline months in one pass (NFR-04: a GROUP BY sort per kind and
/// window otherwise). Same rows, moves and sparks as `most_played`: rows and sparks over the base
/// view, moves over like for like (`Src::Like`, which may hold reconciled desktop rows).
#[derive(Clone)]
struct MostAcc<'c> {
    like: Src<'c>,
    base_like: bool,
    ranked: Option<Span>,
    cur: Span,
    bounds: (u32, u32),
    pv: Option<(u32, u32)>,
    spark: Option<Span>,
    month_ix: IdMap<u32, usize>,
    n_spark: usize,
    combos: IdMap<u64, Combo>,
}

impl<'c> MostAcc<'c> {
    fn new(
        r: &Resolved,
        cov: &'c Coverage,
        prev: Option<Span>,
        spark_months: &[NaiveDate],
    ) -> Self {
        let ranked = prev.filter(|_| cov.no_compare_reason.is_none());
        let num = super::v3::num;
        MostAcc {
            like: Src::Like(cov),
            base_like: Src::Like(cov).is_base(),
            ranked,
            cur: r.cur,
            bounds: (num(r.cur.0), num(r.cur.1)),
            pv: ranked.map(|p| (num(p.0), num(p.1))),
            spark: spark_months
                .first()
                .zip(spark_months.last())
                .map(|(a, b)| (*a, super::v3::month_end(*b))),
            month_ix: spark_months
                .iter()
                .enumerate()
                .map(|(i, m)| (m.year() as u32 * 100 + m.month(), i))
                .collect(),
            n_spark: spark_months.len(),
            combos: IdMap::default(),
        }
    }

    /// The windows it reads.
    fn spans(&self) -> Vec<Span> {
        std::iter::once(self.cur)
            .chain(self.spark)
            .chain(self.ranked)
            .collect()
    }

    fn add(&mut self, p: &super::v3::Pass) -> Result<()> {
        let d = p.day;
        let in_like = self.like.has(p);
        let in_cur = self.bounds.0 <= d && d <= self.bounds.1;
        let si = self.month_ix.get(&(d / 100)).copied().filter(|_| p.base);
        let to = [
            p.base && in_cur,
            !self.base_like && in_like && in_cur && self.ranked.is_some(),
            in_like && self.pv.is_some_and(|w| w.0 <= d && d <= w.1),
        ];
        if !to.iter().any(|x| *x) && si.is_none() {
            return Ok(());
        }
        let (artist, album, track, norm, iid) = (
            p.artist()?,
            p.album()?,
            p.track()?,
            p.norm_key()?,
            p.item_id()?,
        );
        let n = self.n_spark;
        let c = self.combo([artist, album, track, norm], iid);
        for (on, sum) in to.iter().zip(&mut c.sums) {
            if *on {
                sum.0 += 1;
                sum.1 += p.play as i64;
                sum.2 += p.ms;
            }
        }
        if let Some(i) = si {
            if c.spark.is_empty() {
                c.spark = vec![0; n];
            }
            c.spark[i] += p.ms;
        }
        Ok(())
    }

    /// The combo of these names, track key and record, added when new.
    fn combo(
        &mut self,
        [artist, album, track, norm]: [Option<&str>; 4],
        iid: Option<i64>,
    ) -> &mut Combo {
        let mut key =
            super::v3::quick_hash(&[artist, album, track, norm]) ^ iid.unwrap_or(-1) as u64;
        // a quick hash, checked against the texts (a collision moves on to the next key)
        loop {
            match self.combos.get(&key) {
                Some(c)
                    if c.iid == iid
                        && c.artist.as_deref() == artist
                        && c.album.as_deref() == album
                        && c.track.as_deref() == track
                        && c.norm.as_deref() == norm =>
                {
                    break;
                }
                Some(_) => key = key.wrapping_add(0x9e37_79b9_7f4a_7c15),
                None => {
                    self.combos.insert(
                        key,
                        Combo {
                            artist: artist.map(str::to_owned),
                            album: album.map(str::to_owned),
                            track: track.map(str::to_owned),
                            norm: norm.map(str::to_owned),
                            iid,
                            sums: Default::default(),
                            spark: vec![],
                        },
                    );
                    break;
                }
            }
        }
        self.combos.get_mut(&key).expect("found or inserted")
    }

    /// Adds another part's events (`scan_parts`).
    fn merge(&mut self, o: MostAcc) {
        let n = self.n_spark;
        for c in o.combos.into_values() {
            let m = self.combo(
                [
                    c.artist.as_deref(),
                    c.album.as_deref(),
                    c.track.as_deref(),
                    c.norm.as_deref(),
                ],
                c.iid,
            );
            for (a, b) in m.sums.iter_mut().zip(c.sums) {
                (a.0, a.1, a.2) = (a.0 + b.0, a.1 + b.1, a.2 + b.2);
            }
            if !c.spark.is_empty() {
                if m.spark.is_empty() {
                    m.spark = vec![0; n];
                }
                for (a, b) in m.spark.iter_mut().zip(c.spark) {
                    *a += b;
                }
            }
        }
    }
}

/// Most played of a period (not all time): `MostAcc` over its own pass.
#[cfg(test)]
fn most_played_pass<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    r: &Resolved,
    cov: &Coverage,
    prev: Option<Span>,
    spark_months: &[NaiveDate],
) -> Result<[Vec<TopRow>; 3]> {
    let mut acc = MostAcc::new(r, cov, prev, spark_months);
    super::v3::scan_spans(cx, &acc.spans(), |p| acc.add(p))?;
    most_rows(cx, acc, spark_months)
}

/// The Most played rows of a finished `MostAcc`.
fn most_rows<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    acc: MostAcc,
    spark_months: &[NaiveDate],
) -> Result<[Vec<TopRow>; 3]> {
    let MostAcc {
        ranked,
        base_like,
        combos,
        ..
    } = acc;
    // [kind] → group key (as SQL groups: lower(artist), lower(album), norm_key; hashed) → group:
    // the shown rows, like for like in the period (when that isn't the base view) and before
    let mut shown: [IdMap<Option<u64>, Group>; 3] = Default::default();
    let mut now_like: [IdMap<Option<u64>, Group>; 3] = Default::default();
    let mut before: [IdMap<Option<u64>, Group>; 3] = Default::default();
    // [kind] → spark key (as `sparks` matches rows: lower(artist), lower(album), lower(track) and
    // lower(artist); hashed) → ms per spark month
    let mut sp: [IdMap<u64, Vec<i64>>; 3] = Default::default();
    let mut buf = Vec::new();
    let fold = super::v3::folded_hash;
    for c in combos.into_values() {
        let (artist, album, track) = (c.artist.as_deref(), c.album.as_deref(), c.track.as_deref());
        let keys = [
            artist.map(|a| fold(&mut buf, &[a])),
            album.map(|a| fold(&mut buf, &[a])),
            c.norm.as_deref().map(super::v3::exact_hash),
        ];
        let names = [artist, album, track];
        let bys = [None, artist, artist];
        for k in 0..3 {
            for (sum, map) in c
                .sums
                .iter()
                .zip([&mut shown[k], &mut now_like[k], &mut before[k]])
            {
                if sum.0 > 0 {
                    map.entry(keys[k])
                        .or_default()
                        .add(names[k], bys[k], sum.1, sum.2, c.iid);
                }
            }
        }
        if !c.spark.is_empty() {
            let (a, b, t) = (
                artist.unwrap_or(""),
                album.unwrap_or(""),
                track.unwrap_or(""),
            );
            let sk = [
                fold(&mut buf, &[a]),
                fold(&mut buf, &[b]),
                fold(&mut buf, &[t, a]),
            ];
            for (k, key) in sk.into_iter().enumerate() {
                let v = sp[k]
                    .entry(key)
                    .or_insert_with(|| vec![0; spark_months.len()]);
                for (x, ms) in v.iter_mut().zip(&c.spark) {
                    *x += ms;
                }
            }
        }
    }
    let kinds = [Kind::Artist, Kind::Album, Kind::Track];
    let by_name = [
        super::v3::items_by_name(cx.conn, "artist")?,
        super::v3::items_by_name(cx.conn, "album")?,
    ];
    let mut item_info = cx
        .conn
        .prepare("SELECT type, rating, status FROM item WHERE id = ?1")?;
    // `rankings`: keys with time in the window, by time then key, the first MOVE_DEPTH
    let ranking = |kind: Kind, g: &IdMap<Option<u64>, Group>| -> Vec<String> {
        let mut v: Vec<(i64, String)> = g
            .values()
            .filter(|g| !g.name.is_empty() && g.ms > 0)
            .map(|g| (g.ms, row_key(kind, &g.name, &g.by)))
            .collect();
        v.sort_by(|x, y| y.0.cmp(&x.0).then_with(|| x.1.cmp(&y.1)));
        v.into_iter().take(MOVE_DEPTH).map(|x| x.1).collect()
    };
    let mut out: [Vec<TopRow>; 3] = Default::default();
    for k in 0..3 {
        let kind = kinds[k];
        let mut groups: Vec<&Group> = shown[k].values().filter(|g| !g.name.is_empty()).collect();
        groups.sort_by(|a, b| b.ms.cmp(&a.ms).then_with(|| a.name.cmp(&b.name)));
        groups.truncate(TOP);
        let keys: Vec<String> = groups
            .iter()
            .map(|g| row_key(kind, &g.name, &g.by))
            .collect();
        let moves: Option<(Vec<String>, Vec<String>)> = ranked.map(|_| {
            let now = if base_like {
                keys.clone()
            } else {
                ranking(kind, &now_like[k])
            };
            (now, ranking(kind, &before[k]))
        });
        for (g, key) in groups.into_iter().zip(&keys) {
            let id = match kind {
                Kind::Track => g.iid,
                _ => by_name[k].get(&g.name.to_ascii_lowercase()).map(|x| x.0),
            };
            // `rank`: a track's id only when it is a track record
            let info: Option<(Option<f64>, Status)> = match id {
                Some(id) => rusqlite::OptionalExtension::optional(
                    item_info
                        .query_row([id], |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?))),
                )?
                .filter(|(t, _, _)| !matches!(kind, Kind::Track) || t == "track")
                .map(|(_, rating, status)| (rating, status)),
                None => None,
            };
            let (mv, is_new) = match &moves {
                None => (None, false),
                Some((now, before)) => {
                    let at = |v: &[String]| v.iter().position(|x| x == key).map(|i| i as i64);
                    match (at(now), at(before)) {
                        (Some(n), Some(b)) => (Some(b - n), false),
                        (_, None) => (None, true),
                        _ => (None, false),
                    }
                }
            };
            let spark_key = match kind {
                Kind::Track => fold(&mut buf, &[&g.name, &g.by]),
                _ => fold(&mut buf, &[&g.name]),
            };
            let spark = sp[k].get(&spark_key).map_or_else(
                || vec![0.0; spark_months.len()],
                |v| v.iter().map(|ms| super::hours(*ms)).collect(),
            );
            out[k].push(TopRow {
                row: RankRow {
                    name: g.name.clone(),
                    by: g.by.clone(),
                    plays: g.plays,
                    hours: super::hours(g.ms),
                    item_id: info.as_ref().and(id),
                    rating: info.as_ref().and_then(|i| i.0),
                    status: info.map(|i| i.1),
                },
                r#move: mv,
                is_new,
                spark,
            });
        }
    }
    Ok(out)
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
    // each month's top 10 by time, then name (as SQL's ROW_NUMBER over `ms DESC, artist`)
    let mut by: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    let rows = super::v3::month_artists(cx, &month(from), &month(to))?;
    for chunk in rows.chunk_by(|x, y| x.0 == y.0) {
        let mut v: Vec<&super::v3::MonthArtist> = chunk.iter().collect();
        v.sort_by(|x, y| y.2.cmp(&x.2).then_with(|| x.1.cmp(&y.1)));
        by.insert(
            chunk[0].0.clone(),
            v.into_iter()
                .take(LINE_LEFT_TOP)
                .map(|x| x.1.clone())
                .collect(),
        );
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

/// Each artist key's first desktop play's local date, reconciled twins included (the desktop log's
/// own record, as `coverage::compared` counts it for `['S']`); empty unless that is the comparison.
pub(crate) fn pc_first_plays<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    cov: &Coverage,
) -> Result<HashMap<String, NaiveDate>> {
    if cov.compared_sources != ["S"] {
        return Ok(HashMap::new());
    }
    let mut st = cx.conn.prepare(&format!(
        // MIN's bare column: `local` of the row with the first end time
        "{} SELECT lower(artist_name), MIN(ended_at_utc), local FROM ev
         WHERE counts_as_play AND IFNULL(artist_name, '') <> '' GROUP BY 1",
        Src::Desktop.cte(cx.include_private)
    ))?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(2)?)))?;
    let mut out = HashMap::new();
    for r in rows {
        let (a, local) = r?;
        if let Some(d) = super::v3::parse_day(&local) {
            out.insert(a, d);
        }
    }
    Ok(out)
}

/// New artists (first play in all stored data in the window): the period, and like for like
/// now and in the comparison windows: desktop only (`['S']`) counts one when the desktop log also
/// played it in that window (`pc_first`, from `pc_first_plays`: an export twin doesn't hide it);
/// export only when that first play is from the export.
pub(crate) fn new_artists_compared(
    heard: &[FirstHeard],
    pc_first: &HashMap<String, NaiveDate>,
    cov: &Coverage,
    cur: Span,
    vs: &[Span],
) -> Compared {
    let allowed = |f: &FirstHeard, s: Span| match cov.compared_sources.as_slice() {
        ["S"] => pc_first
            .get(&f.artist)
            .is_some_and(|d| (s.0..=s.1).contains(d)),
        ["E"] => f.source == Source::Export,
        _ => true,
    };
    let count = |s: Span, like: bool| {
        heard
            .iter()
            .filter(|f| (s.0..=s.1).contains(&f.local.date()) && (!like || allowed(f, s)))
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

/// Every `scan` figure of a period report, read in one pass (`scan_parts`).
#[derive(Clone)]
struct Figures<'c> {
    /// like for like over the period and compared windows; the period's own (when like for like
    /// isn't the base view); the period on this PC
    aggs: [Option<AggAcc<'c>>; 3],
    /// the period's artists (when not from the rollup); on this PC
    artists: [Option<ArtistAcc<'c>>; 2],
    clock_cur: Option<ClockAcc>,
    clock_base: ClockAcc,
    /// Repeat: period and average together; or the period, and the average apart
    reps: [Option<RepeatAcc<'c>>; 3],
    tags: Option<TagAcc>,
    most: Option<MostAcc<'c>>,
}

impl Figures<'_> {
    fn spans(&self) -> Vec<Span> {
        let mut spans: Vec<Span> = vec![self.clock_base.span];
        for a in self.aggs.iter().flatten() {
            spans.extend(&a.spans);
        }
        spans.extend(self.artists.iter().flatten().map(|a| a.span));
        spans.extend(self.clock_cur.as_ref().map(|c| c.span));
        for a in self.reps.iter().flatten() {
            spans.extend(&a.spans);
        }
        spans.extend(self.tags.as_ref().map(|t| t.span));
        spans.extend(self.most.iter().flat_map(|m| m.spans()));
        spans
    }

    fn add(&mut self, p: &super::v3::Pass) -> Result<()> {
        for a in self.aggs.iter_mut().flatten() {
            a.add(p)?;
        }
        for a in self.artists.iter_mut().flatten() {
            a.add(p)?;
        }
        if let Some(c) = &mut self.clock_cur {
            c.add(p)?;
        }
        self.clock_base.add(p)?;
        for a in self.reps.iter_mut().flatten() {
            a.add(p)?;
        }
        if let Some(t) = &mut self.tags {
            t.add(p)?;
        }
        if let Some(m) = &mut self.most {
            m.add(p)?;
        }
        Ok(())
    }

    fn merge(&mut self, o: Self) {
        for (a, b) in self.aggs.iter_mut().zip(o.aggs) {
            if let (Some(a), Some(b)) = (a, b) {
                a.merge(b);
            }
        }
        for (a, b) in self.artists.iter_mut().zip(o.artists) {
            if let (Some(a), Some(b)) = (a, b) {
                a.merge(b);
            }
        }
        if let (Some(a), Some(b)) = (&mut self.clock_cur, o.clock_cur) {
            a.merge(b);
        }
        self.clock_base.merge(o.clock_base);
        for (a, b) in self.reps.iter_mut().zip(o.reps) {
            if let (Some(a), Some(b)) = (a, b) {
                a.merge(b);
            }
        }
        if let (Some(a), Some(b)) = (&mut self.tags, o.tags) {
            a.merge(b);
        }
        if let (Some(a), Some(b)) = (&mut self.most, o.most) {
            a.merge(b);
        }
    }
}

/// `get_period_report`.
pub fn report<Tz: TimeZone + Send + 'static>(
    cx: &Ctx<Tz>,
    q: &PeriodQuery,
) -> Result<Covered<PeriodReport>> {
    let today = cx.today();
    let first = coverage::first_day(cx.conn)?;
    let r = resolve(q, today, first)?;
    let (model, cov) = period_coverage(cx, &r, STREAK_BACK_DAYS + 31)?;
    let hidden = cov.no_compare_reason.is_some();
    let all_time = r.grain == Grain::All;
    // Design §4.4: past the export, a bounded period counts this PC ("on this PC"); all time
    // counts all-device dates only (the export's first to last date).
    let on_pc = cov.desktop_only_since.is_some() && !all_time;
    let end = r.cur.1.min(today);
    let shown = (r.cur.0, end);
    let dev_span = model
        .export
        .filter(|_| all_time && cov.desktop_only_since.is_some())
        .map(|(a, b)| (a.max(shown.0), b.min(end)));
    // days listened, streak and the fingerprint read these dates
    let (fig, fig_src) = match dev_span {
        Some(s) => (s, Src::All),
        None => (shown, if on_pc { Src::Desktop } else { Src::All }),
    };
    let vs: Vec<Span> = r.vs.clone().unwrap_or_default();
    let like = Src::Like(&cov);
    let basis = if on_pc { Src::Desktop } else { Src::All };
    let prev = r.single_vs().or_else(|| vs.first().copied());
    let spark_months = months(month_start_minus(end, 11), end);
    // All time reads rollups only for these: each on its own connection while the events are
    // scanned below (NFR-04).
    let side = all_time.then(|| {
        let most = |k: Kind| {
            let (r, cov, sm) = (r.clone(), cov.clone(), spark_months.clone());
            Aside::run_cx(cx, move |cx| most_played(cx, &r, &cov, k, true, prev, &sm))
        };
        (
            Aside::run_cx(cx, super::v3::aggs_all),
            Aside::run_cx(cx, move |cx| by_artist(cx, Src::All, r.cur, first)),
            Aside::run_cx(cx, top_tags_all),
            [most(Kind::Artist), most(Kind::Album), most(Kind::Track)],
        )
    });

    // One pass over the events the figures below read (NFR-04: the period, the compared windows,
    // the last 6 months, the sparkline months and the average's months read once together, not
    // once per figure). Each figure keeps to its own windows and view (`Src::has`).
    let cur_vs: Vec<Span> = std::iter::once(r.cur).chain(vs.iter().copied()).collect();
    let agg_like = (!all_time && !hidden).then(|| AggAcc::new(like, &cur_vs));
    let agg_all =
        (!all_time && (hidden || !like.is_base())).then(|| AggAcc::new(Src::All, &[r.cur]));
    let fig_own = on_pc || dev_span.is_some();
    let agg_pc = fig_own.then(|| AggAcc::new(fig_src, &[fig]));
    let artists_all =
        (!by_artist_rollup(cx, Src::All, r.cur, first)).then(|| ArtistAcc::new(Src::All, r.cur));
    let artists_pc = fig_own.then(|| ArtistAcc::new(fig_src, fig));
    let clock_cur = (!all_time).then(|| ClockAcc::new(r.cur));
    let clock_base = ClockAcc::new((end - Months::new(6) + TimeDelta::days(1), end));
    // Repeat now and in the average's months: one figure from the first of those months to the
    // period's end when both count every event (the days between are a window read only to know
    // which tracks were heard before the period).
    let avg_spans = average_spans(r.cur.0, end);
    let cur_repeat = first.is_some_and(|f| f < r.cur.0);
    let rep_merged = match avg_spans.as_deref() {
        Some(ms) if cur_repeat && !on_pc => {
            let mut spans = ms.to_vec();
            let gap = ms[ms.len() - 1].1 + TimeDelta::days(1);
            if gap < r.cur.0 {
                spans.push((gap, r.cur.0 - TimeDelta::days(1)));
            }
            spans.push(r.cur);
            Some(RepeatAcc::new(Src::All, &spans))
        }
        _ => None,
    };
    let merged = rep_merged.is_some();
    let rep_cur = (cur_repeat && !merged).then(|| RepeatAcc::new(basis, &[r.cur]));
    let rep_avg = avg_spans
        .as_deref()
        .filter(|_| !merged)
        .map(|ms| RepeatAcc::new(basis, ms));
    let tags = (!all_time).then(|| TagAcc::new(r.cur));
    let most = (!all_time).then(|| MostAcc::new(&r, &cov, prev, &spark_months));
    let figs = Figures {
        aggs: [agg_like, agg_all, agg_pc],
        artists: [artists_all, artists_pc],
        clock_cur,
        clock_base,
        reps: [rep_merged, rep_cur, rep_avg],
        tags,
        most,
    };
    let spans = figs.spans();
    let mut parts = super::v3::scan_parts(cx, &spans, figs, &|f, p| f.add(p))?.into_iter();
    let mut figs = parts.next().expect("one part at least");
    for o in parts {
        figs.merge(o);
    }
    let Figures {
        aggs: [agg_like, agg_all, agg_pc],
        artists: [artists_all, artists_pc],
        clock_cur,
        clock_base,
        reps: [rep_merged, rep_cur, rep_avg],
        tags: tags_acc,
        most: most_acc,
    } = figs;
    // Repeat now and in the average's months: one seek per track, on another connection while the
    // figures below are read (NFR-04).
    let (rep_merged, rep_cur, rep_avg) = (
        rep_merged.map(RepeatAcc::into_parts),
        rep_cur.map(RepeatAcc::into_parts),
        rep_avg.map(RepeatAcc::into_parts),
    );
    let n_avg = avg_spans.as_ref().map_or(0, Vec::len);
    let rep_job = move |conn: &Connection, include_private: bool| -> Result<(i64, Vec<i64>)> {
        let mut now = 0;
        let avg = match (rep_merged, rep_avg) {
            (Some(p), _) => {
                let v = p.count(conn, include_private)?;
                now = v[v.len() - 1];
                v[..n_avg].to_vec()
            }
            (None, avg) => {
                if let Some(p) = rep_cur {
                    now = p.count(conn, include_private)?[0];
                }
                match avg {
                    Some(p) => p.count(conn, include_private)?,
                    None => vec![],
                }
            }
        };
        Ok((now, avg))
    };
    let repeats = super::v3::Aside::run(cx, rep_job);

    // Basic counts: the period, then like for like (FR-048).
    // like for like: the same pass gives the timeline's days (`daily` over `like`)
    let mut like_days = None;
    let cur_all = agg_all.map(|a| a.finish().0[0]);
    let (side_aggs, side_artists, side_tags, side_most) = match side {
        Some((a, b, c, d)) => (Some(a), Some(b), Some(c), Some(d)),
        None => (None, None, None, None),
    };
    let (all, like_aggs): (Agg, Option<(Agg, Vec<Agg>)>) = if let Some(a) = side_aggs {
        (a.join()?, None)
    } else if let Some(acc) = agg_like {
        let (v, days) = acc.finish();
        like_days = Some(days);
        (cur_all.unwrap_or(v[0]), Some((v[0], v[1..].to_vec())))
    } else {
        (cur_all.unwrap_or_default(), None)
    };
    let pick = |f: fn(&Agg) -> f64| {
        like_aggs
            .as_ref()
            .map(|(n, b)| (f(n), b.iter().map(f).collect()))
    };
    let c = |f: fn(&Agg) -> f64, round: fn(f64) -> f64| compared(f(&all), pick(f), round);

    // New artists (first play in all stored data in the window).
    let heard = terms::first_heard(cx.conn, cx.include_private)?;
    let pc_first = pc_first_plays(cx, &cov)?;
    let new_artists = new_artists_compared(&heard, &pc_first, &cov, r.cur, &vs);

    // Days listened, streak and covered days: this PC only past the export (design §4.4).
    let class = |d: NaiveDate| {
        if dev_span.is_some() {
            match model.class(d) {
                CoverageClass::AllDevices => CoverageClass::AllDevices,
                _ => CoverageClass::NoData,
            }
        } else if !on_pc {
            model.class(d)
        } else if model.ran(d) {
            CoverageClass::ThisPc
        } else {
            CoverageClass::NoData
        }
    };
    let look = fig.1 - TimeDelta::days(STREAK_BACK_DAYS);
    let basis_days = daily(cx, basis, (look.min(fig.0), fig.1))?;
    let played_days = played(&basis_days);
    let streak = if fig.1 >= fig.0 {
        streaks(look, fig.1, &played_days, class).1
    } else {
        0
    };
    let covered_days = fig
        .0
        .iter_days()
        .take_while(|d| *d <= fig.1)
        .filter(|d| class(*d) != CoverageClass::NoData)
        .count() as i64;
    let days_listened_now = played_days.range(fig.0..=fig.1.max(fig.0)).count() as i64;
    let mut days_listened = c(|a| a.days as f64, round0);
    if fig_own {
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

    // Timeline: day by day for weeks, months and short custom ranges, else month by month.
    let by_day = matches!(r.grain, Grain::Week | Grain::Month)
        || (r.grain == Grain::Custom && (r.cur.1 - r.cur.0).num_days() < DAILY_MAX_DAYS);
    let tl_src = if hidden { Src::All } else { like };
    let cur_days = match &like_days {
        _ if end < r.cur.0 => BTreeMap::new(),
        Some(d) => d.range(shown.0..=shown.1).map(|(k, v)| (*k, *v)).collect(),
        None => daily(cx, tl_src, shown)?,
    };
    // read only at the compared windows' dates, which hold no current day
    let vs_days = match (vs.is_empty(), like_days) {
        (false, Some(d)) => d,
        _ => BTreeMap::new(),
    };
    let sum = |m: &BTreeMap<NaiveDate, (i64, i64)>, a: NaiveDate, b: NaiveDate| -> i64 {
        m.range(a..=b.max(a))
            .filter(|(d, _)| **d <= b)
            .map(|(_, v)| v.0)
            .sum()
    };
    // a comparison date its sources don't cover is no data: left out of the ghost line, never 0
    // (design §4.4); desktop only counts the dates the log ran
    let vs_has = |x: NaiveDate| {
        if cov.compared_sources == ["S"] {
            model.ran(x)
        } else {
            model.class(x) != CoverageClass::NoData
        }
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
                            .filter(|(x, s)| *x <= s.1 && vs_has(*x))
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
                                    let (a, b) = (vm.max(s.0), super::v3::month_end(vm).min(s.1));
                                    // most of the month no data: no point
                                    let n = a.iter_days().take_while(|d| *d <= b);
                                    let (has, all) =
                                        n.fold((0, 0), |(h, t), d| (h + vs_has(d) as i64, t + 1));
                                    (2 * has >= all).then(|| sum(&vs_days, a, b))
                                })
                                .collect(),
                        )
                    },
                    coverage: majority(a.iter_days().take_while(|d| *d <= b), |d| model.class(d)),
                }
            })
            .collect()
    };

    // When you listened: the period vs the last 6 months.
    let (hours, weekdays) = match clock_cur {
        Some(c) => c.finish(),
        None => clock_ms(cx, r.cur, all_time)?,
    };
    let (baseline, _) = clock_base.finish();
    let busiest = hours
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(&a.0)))
        .filter(|(_, ms)| **ms > 0)
        .map(|(h, ms)| (h, pct(*ms, hours.iter().sum())));

    // Artists of the period, new ones first heard in it.
    let artists = match (artists_all, side_artists) {
        (Some(a), _) => a.out,
        (None, Some(a)) => a.join()?,
        (None, None) => by_artist(cx, Src::All, r.cur, first)?,
    };
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

    // Fingerprint now (basis as days listened) and the 12-month average.
    let basis_artists;
    let (fp_artists, fp_agg) = match (artists_pc, agg_pc) {
        (Some(a), Some(g)) => {
            basis_artists = a.out;
            (&basis_artists, g.finish().0[0])
        }
        _ => (&artists, all),
    };
    let new_keys: HashSet<&str> = new_here.keys().copied().collect();
    let mut parts = FpParts {
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
        repeat_plays: 0,
    };
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

    let (tag_rows, tag_cov) = match (tags_acc, side_tags) {
        (Some(t), _) => terms::tag_shares(cx.conn, t.per, 6)?,
        (None, Some(t)) => t.join()?,
        (None, None) => top_tags_all(cx)?,
    };
    let most = |k| most_played(cx, &r, &cov, k, all_time, prev, &spark_months);
    let [artists, albums, tracks] = match (most_acc, side_most) {
        (Some(acc), _) => most_rows(cx, acc, &spark_months)?,
        (None, Some([a, b, c])) => [a.join()?, b.join()?, c.join()?],
        (None, None) => [most(Kind::Artist)?, most(Kind::Album)?, most(Kind::Track)?],
    };
    // the repeat counts ran beside everything above (NFR-04)
    let (repeat_plays, avg_repeats) = repeats.join()?;
    parts.repeat_plays = repeat_plays;
    let average = match avg_spans.as_deref() {
        Some(ms) => average_fingerprint(cx, &model, on_pc, &heard, ms, &avg_repeats)?,
        None => None,
    };
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
            artists,
            albums,
            tracks,
        },
    };
    covered(cx, data, cov, true)
}

/// `terms::top_tags` over all time from the 0005 rollup (per record, then its links and tags).
fn top_tags_all<Tz: TimeZone>(cx: &Ctx<Tz>) -> Result<(Vec<TagShare>, f64)> {
    let mut st = cx.conn.prepare(&format!(
        "SELECT NULLIF(item_id, 0), ms_played FROM listening_rollup WHERE events > 0{}",
        cx.private_filter("private_session")
    ))?;
    let mut per: HashMap<Option<i64>, i64> = HashMap::new();
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        *per.entry(r.get(0)?).or_default() += r.get::<_, i64>(1)?;
    }
    drop(rows);
    terms::tag_shares(cx.conn, per, 6)
}

/// First day of the month `n` months before `d`'s.
fn month_start_minus(d: NaiveDate, n: u32) -> NaiveDate {
    super::v3::month_start(d) - Months::new(n)
}

/// The 12 calendar months before the period's first month, as spans; `None` when they start after
/// `end` (the period's last day up to today).
fn average_spans(start: NaiveDate, end: NaiveDate) -> Option<Vec<Span>> {
    let to = super::v3::month_start(start) - TimeDelta::days(1);
    let from = month_start_minus(to, 11);
    (from <= end).then(|| {
        months(from, to)
            .into_iter()
            .map(|m| (m, super::v3::month_end(m)))
            .collect()
    })
}

/// Your own 12-month average: the mean of each measure over the 12 calendar months before the
/// period's first month (`spans`, from `average_spans`; months without listening left out), with
/// their Repeat plays (`repeats`, per span); `None` when there are none. `on_pc` (the period is
/// counted on this PC): the months count desktop plays and the dates the log ran, like for like
/// with now (FR-048); months the log didn't cover have no time and are left out.
fn average_fingerprint<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    model: &Model,
    on_pc: bool,
    heard: &[FirstHeard],
    spans: &[Span],
    repeats: &[i64],
) -> Result<Option<Fingerprint>> {
    let (from, to) = (spans[0].0, spans[spans.len() - 1].1);
    let mut artists: HashMap<String, Vec<(String, i64)>> = HashMap::new();
    if on_pc {
        for (m, a, v, _) in super::v3::desktop_month_artists(cx, (from, to))?.0 {
            artists.entry(m).or_default().push((a, v));
        }
    } else {
        let mut st = cx.conn.prepare(&format!(
            "SELECT month, artist, SUM(ms_played) FROM listening_month_artist
             WHERE month BETWEEN '{}' AND '{}' AND events > 0 AND artist <> ''{} GROUP BY 1, 2",
            month(from),
            month(to),
            cx.private_filter("private_session")
        ))?;
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
    }
    let src = if on_pc { Src::Desktop } else { Src::All };
    let days = daily(cx, src, (from, to))?;
    let first_month: HashMap<&str, String> = heard
        .iter()
        .map(|f| (f.artist.as_str(), f.local.format("%Y-%m").to_string()))
        .collect();
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
                .filter(|d| {
                    if on_pc {
                        model.ran(*d)
                    } else {
                        model.class(*d) != CoverageClass::NoData
                    }
                })
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

    /// The one-pass Most played equals the per-kind SQL path (rows, moves, sparks) for every grain
    /// and comparison, with and without private sessions, on the insights fixture (library records,
    /// a reconciled twin, a private play, a play with no record, export-only comparisons).
    #[test]
    fn most_played_pass_equals_the_sql_path() {
        let (c, _) = super::super::fixture::build();
        let (mut compared, mut moves, mut like) = (0, 0, 0);
        for include_private in [false, true] {
            let cx = Ctx {
                conn: &c,
                include_private,
                tz: &chrono::Utc,
                now: super::super::fixture::now(),
            };
            let today = cx.today();
            let first = coverage::first_day(&c).unwrap();
            for q in [
                r#"{"grain": "week", "vs": "prev", "at": "2026-10-12"}"#,
                r#"{"grain": "month", "vs": "prev", "at": null}"#,
                r#"{"grain": "month", "vs": "prev", "at": "2026-09"}"#,
                r#"{"grain": "month", "vs": "last_year", "at": null}"#,
                r#"{"grain": "month", "vs": "usual", "at": null}"#,
                r#"{"grain": "year", "vs": "prev", "at": null}"#,
                r#"{"grain": "custom", "vs": "prev", "at": null, "from": "2026-09-01", "to": "2026-10-19"}"#,
            ] {
                let q: PeriodQuery = serde_json::from_str(q).unwrap();
                let r = resolve(&q, today, first).unwrap();
                let (_, cov) = period_coverage(&cx, &r, 0).unwrap();
                let end = r.cur.1.min(today);
                let prev = r
                    .single_vs()
                    .or_else(|| r.vs.as_ref().and_then(|v| v.first().copied()));
                let spark = months(month_start_minus(end, 11), end);
                let pass = most_played_pass(&cx, &r, &cov, prev, &spark).unwrap();
                like += usize::from(!Src::Like(&cov).is_base() && cov.no_compare_reason.is_none());
                let sql = [Kind::Artist, Kind::Album, Kind::Track]
                    .map(|k| most_played(&cx, &r, &cov, k, false, prev, &spark).unwrap());
                assert_eq!(pass, sql, "{q:?} private {include_private}");
                compared += pass.iter().map(Vec::len).sum::<usize>();
                moves += pass
                    .iter()
                    .flatten()
                    .filter(|t| t.r#move.is_some() || t.is_new)
                    .count();
            }
        }
        assert!(
            compared > 20 && moves > 0 && like > 0,
            "the fixture exercises rows, moves and export-only rankings"
        );
    }
}
