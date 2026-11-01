//! Insights v3 commands, shared parts (API.md §4.1, M10 WI 5): the page header's `PeriodQuery`,
//! its resolution into a period and comparison windows, the `ev` event CTE (all events or the
//! like-for-like ones FR-048 picks, `coverage::compared`), and the aggregates every tab reads.
//! Sources U, S, E only: `listening_event`, its 0008 rollups and `item*`; no cache DB handle.
//!
//! Speed (NFR-04, design §6): whole-month spans over the base view read the 0008 rollups
//! (`listening_day`, `listening_month_artist`); everything else is one grouped SQL pass. Dates put
//! into SQL text are formatted `NaiveDate`s, never user input.
use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Datelike, NaiveDate, TimeDelta, TimeZone, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::coverage::{self, Coverage, Model};
use super::terms::{Compare, CoverageClass, Period, Span, compare_with};
use super::{Covered, base, sourced};
use crate::error::{AppError, Result};
use crate::library::Status;

/// API.md §4.1 `Grain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Grain {
    Week,
    Month,
    Year,
    All,
    Custom,
}

/// API.md §4.1 `PeriodQuery` (the page header).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeriodQuery {
    pub grain: Grain,
    pub vs: Compare,
    pub at: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

/// What every v3 read needs: the library connection, the private-session setting, the local zone
/// (`chrono::Local` in the app, `Utc` in tests) and now.
pub struct Ctx<'a, Tz: TimeZone> {
    pub conn: &'a Connection,
    pub include_private: bool,
    pub tz: &'a Tz,
    pub now: DateTime<Utc>,
}

impl<Tz: TimeZone> Ctx<'_, Tz> {
    pub fn today(&self) -> NaiveDate {
        self.now.with_timezone(self.tz).date_naive()
    }

    /// `Model::load` for the local dates `from..=to`.
    pub fn model(&self, from: NaiveDate, to: NaiveDate) -> Result<Model> {
        Model::load(self.conn, self.tz, from, to, self.now)
    }

    /// `private_session = 0` unless private sessions are counted (TSD §4.8), for rollup tables.
    pub fn private_filter(&self, col: &str) -> String {
        if self.include_private {
            String::new()
        } else {
            format!(" AND {col} = 0")
        }
    }
}

/// `data` as `Covered<T>`; `with_u` = the response also reads library records (tags, item ids).
pub fn covered<T, Tz: TimeZone>(
    cx: &Ctx<Tz>,
    data: T,
    coverage: Coverage,
    with_u: bool,
) -> Result<Covered<T>> {
    let mut s = sourced(cx.conn, cx.include_private, data)?;
    if !with_u {
        s.sources.retain(|x| *x != "U");
    }
    Ok(Covered {
        sourced: s,
        coverage,
    })
}

// ---------- Dates

pub fn day(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn month(d: NaiveDate) -> String {
    d.format("%Y-%m").to_string()
}

/// "4 Sep 2026", as the UI writes a date.
pub fn show(d: NaiveDate) -> String {
    d.format("%-d %b %Y").to_string()
}

pub fn parse_day(s: &str) -> Option<NaiveDate> {
    (s.len() >= 10)
        .then(|| NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d").ok())
        .flatten()
}

/// A local `YYYY-MM-DD` from the UI: exactly that shape, years 1900–9999.
pub fn strict_day(s: &str) -> Option<NaiveDate> {
    let ok = s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-';
    ok.then(|| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .flatten()
        .filter(|d| (1900..=9999).contains(&d.year()))
}

/// Local time text as the UI shows it: `YYYY-MM-DDTHH:MM`.
pub fn minute(local: &str) -> String {
    local.chars().take(16).collect()
}

/// First day of `d`'s month.
pub fn month_start(d: NaiveDate) -> NaiveDate {
    d.with_day(1).expect("day 1 exists")
}

pub fn month_end(d: NaiveDate) -> NaiveDate {
    Period::Month(d).span().1
}

/// The months `from..=to` touch, as their first days.
pub fn months(from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    let mut out = vec![];
    let mut m = month_start(from);
    while m <= to {
        out.push(m);
        m = m + chrono::Months::new(1);
    }
    out
}

/// The class most of `days` have (ties: all devices, then this PC).
pub fn majority(
    days: impl Iterator<Item = NaiveDate>,
    class: impl Fn(NaiveDate) -> CoverageClass,
) -> CoverageClass {
    let mut n = [0usize; 3];
    for d in days {
        n[class(d) as usize] += 1;
    }
    let best = (0..3).max_by_key(|&i| (n[i], 2 - i)).unwrap_or(2);
    [
        CoverageClass::AllDevices,
        CoverageClass::ThisPc,
        CoverageClass::NoData,
    ][best]
}

// ---------- The period header

/// A `PeriodQuery` made concrete.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub grain: Grain,
    pub vs_kind: Compare,
    pub period: Period,
    pub cur: Span,
    /// The comparison windows (newest first for usual); `None` for all time.
    pub vs: Option<Vec<Span>>,
}

fn bad_date() -> AppError {
    AppError::validation("That period isn't a valid date.")
}

/// Resolve the header: `at` (null = the current period) by grain, a custom range (both dates, from
/// ≤ to), all time from the first play (`first`, else today) to today. `validation` otherwise.
pub fn resolve(q: &PeriodQuery, today: NaiveDate, first: Option<NaiveDate>) -> Result<Resolved> {
    let at = |want: usize, suffix: &str| -> Result<NaiveDate> {
        match q.at.as_deref() {
            None => Ok(today),
            Some(s) if s.len() == want => strict_day(&format!("{s}{suffix}")).ok_or_else(bad_date),
            Some(_) => Err(bad_date()),
        }
    };
    let period = match q.grain {
        Grain::Week => Period::Week(at(10, "")?),
        Grain::Month => Period::Month(at(7, "-01")?),
        Grain::Year => Period::Year(at(4, "-01-01")?),
        Grain::All => Period::All(first.unwrap_or(today).min(today), today),
        Grain::Custom => {
            let d = |s: &Option<String>| s.as_deref().and_then(strict_day);
            match (d(&q.from), d(&q.to)) {
                (Some(a), Some(b)) if a <= b => Period::Custom(a, b),
                _ => return Err(AppError::validation("Choose a start and an end date.")),
            }
        }
    };
    let vs = (q.grain != Grain::All).then(|| compare_with(period, q.vs));
    Ok(Resolved {
        grain: q.grain,
        vs_kind: q.vs,
        period,
        cur: period.span(),
        vs,
    })
}

/// The stepper's `at` for a period: Monday's date, `YYYY-MM` or `YYYY`.
fn at_of(p: Period) -> Option<String> {
    let a = p.span().0;
    match p {
        Period::Week(_) => Some(day(a)),
        Period::Month(_) => Some(month(a)),
        Period::Year(_) => Some(a.format("%Y").to_string()),
        _ => None,
    }
}

impl Resolved {
    /// `(prev_at, next_at)`: null before the first play, after the current period, or custom / all.
    pub fn steps(
        &self,
        today: NaiveDate,
        first: Option<NaiveDate>,
    ) -> (Option<String>, Option<String>) {
        let prev = self
            .period
            .shift(-1)
            .filter(|p| first.is_some_and(|f| p.span().1 >= f))
            .and_then(at_of);
        let next = self
            .period
            .shift(1)
            .filter(|p| p.span().0 <= today)
            .and_then(at_of);
        match self.period {
            Period::Week(_) | Period::Month(_) | Period::Year(_) => (prev, next),
            _ => (None, None),
        }
    }

    /// One comparison window (previous / same last year), else none.
    pub fn single_vs(&self) -> Option<Span> {
        match self.vs.as_deref() {
            Some([s]) => Some(*s),
            _ => None,
        }
    }

    /// The earliest date any window starts.
    pub fn earliest(&self) -> NaiveDate {
        self.vs
            .iter()
            .flatten()
            .map(|s| s.0)
            .fold(self.cur.0, NaiveDate::min)
    }
}

/// The FR-048 coverage of a resolved period, from a model loaded `back` days before its earliest
/// window (streak lookback, the fingerprint's average) to its end.
pub fn period_coverage<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    r: &Resolved,
    back: i64,
) -> Result<(Model, Coverage)> {
    let today = cx.today();
    let model = cx.model(r.earliest() - TimeDelta::days(back), r.cur.1.max(today))?;
    let cov = model.compare(r.cur, r.vs.as_deref(), today);
    Ok((model, cov))
}

/// The coverage model for a view over `from..=to` (whole history, Explore). Dates inside the export
/// are all devices whatever the window, so the desktop log's dates are only read after the export's
/// end (NFR-04: the desktop log can hold most rows).
// ponytail: a no-data stretch between a desktop play before the export's first date and that date
// isn't listed in `gaps`; real histories start with the export.
pub fn view_model<Tz: TimeZone>(cx: &Ctx<Tz>, from: NaiveDate, to: NaiveDate) -> Result<Model> {
    let probe = cx.model(to, to)?;
    let from = match probe.export {
        Some((_, last)) => (last + TimeDelta::days(1)).max(from).min(to),
        None => from,
    };
    cx.model(from, to)
}

/// Coverage for a whole-history or explore view: no comparison, gaps and the desktop-only date for
/// `span` (the mock's shape: `no_compare_reason` stays null, there is nothing to compare).
pub fn view_coverage(model: &Model, span: Span, today: NaiveDate) -> Coverage {
    let c = model.compare(span, None, today);
    Coverage {
        compared_sources: vec![],
        no_compare_reason: None,
        ..c
    }
}

// ---------- Events and aggregates

/// Which events a figure counts.
#[derive(Debug, Clone, Copy)]
pub enum Src<'c> {
    /// The base view: everything there is.
    All,
    /// Like for like over `coverage.compared_sources` (must not be empty).
    Like(&'c Coverage),
    /// Desktop plays only (design §4.4: a period past the last export, "on this PC").
    Desktop,
}

impl Src<'_> {
    /// Counts what the base view counts (the 0008 rollups can answer).
    pub fn is_base(self) -> bool {
        match self {
            Src::All => true,
            Src::Like(c) => c.compared_sources.len() == 2,
            Src::Desktop => false,
        }
    }

    /// The events as a CTE named `ev` with `base`'s columns (`local` included).
    pub fn cte(self, include_private: bool) -> String {
        let desktop;
        let cov = match self {
            Src::All => None,
            Src::Like(c) if c.compared_sources.len() == 2 => None,
            Src::Like(c) => Some(c),
            Src::Desktop => {
                desktop = Coverage {
                    compared_sources: vec!["S"],
                    desktop_only_since: None,
                    gaps: vec![],
                    no_compare_reason: None,
                };
                Some(&desktop)
            }
        };
        match cov.and_then(|c| coverage::compared(c, include_private)) {
            Some(cmp) => format!("{cmp}, ev AS NOT MATERIALIZED (SELECT * FROM cmp)"),
            None => format!(
                "{}, ev AS NOT MATERIALIZED (SELECT * FROM base)",
                base(include_private)
            ),
        }
    }
}

/// `local` in `from..=to` plus a padded UTC bound for the time index (local is at most a day off).
/// Spans over two years read with `+` (a table scan beats ~all rows by index, TSD §3).
pub fn between(s: Span) -> String {
    between_on(s, (s.1 - s.0).num_days() > 730)
}

/// `between`, on the time index or not (`+`: a table scan, or a range of row ids).
fn between_on(s: Span, scan: bool) -> String {
    let plus = if scan { "+" } else { "" };
    let (a, b) = (day(s.0), day(s.1));
    format!(
        "substr(local, 1, 10) BETWEEN '{a}' AND '{b}'
         AND {plus}ended_at_utc >= date('{a}', '-1 day') AND {plus}ended_at_utc < date('{b}', '+2 day')"
    )
}

/// `(ms, plays)` per local date.
pub type Days = BTreeMap<NaiveDate, (i64, i64)>;

/// `(ms, plays)` per local date in `s` (dates without events left out).
pub fn daily<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    src: Src,
    s: Span,
) -> Result<BTreeMap<NaiveDate, (i64, i64)>> {
    let sql = if src.is_base() {
        format!(
            "SELECT day, SUM(ms_played), SUM(plays) FROM listening_day
             WHERE day BETWEEN '{}' AND '{}' AND events > 0{} GROUP BY day",
            day(s.0),
            day(s.1),
            cx.private_filter("private_session")
        )
    } else {
        format!(
            "{} SELECT substr(local, 1, 10), SUM(ms_played), SUM(counts_as_play) FROM ev
             WHERE {} GROUP BY 1",
            src.cte(cx.include_private),
            between(s)
        )
    };
    let mut st = cx.conn.prepare(&sql)?;
    let mut out = BTreeMap::new();
    for r in st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (d, ms, plays) = r?;
        if let Some(d) = parse_day(&d) {
            out.insert(d, (ms, plays));
        }
    }
    Ok(out)
}

/// One window's totals (design §4.2 basic counts).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Agg {
    pub ms: i64,
    pub plays: i64,
    /// Distinct artists / tracks (`norm_key`) / albums (album + artist) with ≥ 1 play.
    pub artists: i64,
    pub tracks: i64,
    pub albums: i64,
    /// Local dates with ≥ 1 play.
    pub days: i64,
    pub export_events: i64,
    /// Export events ended `trackdone`.
    pub finished: i64,
}

impl Agg {
    pub fn tracks_per_artist(&self) -> f64 {
        per(self.tracks, self.artists)
    }
    pub fn albums_per_artist(&self) -> f64 {
        per(self.albums, self.artists)
    }
    pub fn finished_pct(&self) -> Option<f64> {
        (self.export_events > 0).then(|| super::pct(self.finished, self.export_events))
    }
}

fn per(a: i64, b: i64) -> f64 {
    if b == 0 {
        0.0
    } else {
        (a as f64 * 10.0 / b as f64).round() / 10.0
    }
}

/// `Agg` per window in one pass (windows may not overlap); an empty window gives zeros. The
/// distinct counts are kept in Rust (NFR-04: SQL's per-window join and COUNT(DISTINCT) sorts).
pub fn aggs<Tz: TimeZone>(cx: &Ctx<Tz>, src: Src, spans: &[Span]) -> Result<Vec<Agg>> {
    Ok(aggs_days(cx, src, spans)?.0)
}

/// Case-folded (ASCII, as SQL `lower`) 64-bit hash of `parts`: distinct counts without a copy per
/// event. A collision would undercount by one; at 2^-64 per pair that never happens in practice.
pub(crate) fn folded_hash(buf: &mut Vec<u8>, parts: &[&str]) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    buf.clear();
    for p in parts {
        buf.extend(p.bytes().map(|b| b.to_ascii_lowercase()));
        buf.push(0x1f);
    }
    BuildHasherDefault::<DefaultHasher>::default().hash_one(&buf[..])
}

/// Hasher for keys that are already hashes (`folded_hash`) or small numbers: the key itself, so a
/// lookup costs no second SipHash (NFR-04: a few per event in the period passes).
#[derive(Default)]
pub(crate) struct IdHash(u64);

impl std::hash::Hasher for IdHash {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(*b);
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = self.0.rotate_left(32) ^ n;
    }
    fn write_u32(&mut self, n: u32) {
        self.write_u64(n.into());
    }
}

/// A map keyed by hashes or small numbers (`IdHash`).
pub(crate) type IdMap<K, V> = HashMap<K, V, std::hash::BuildHasherDefault<IdHash>>;

/// 64-bit hash of `s` as stored (case kept), for distinct counts like `folded_hash`.
pub(crate) fn exact_hash(s: &str) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    BuildHasherDefault::<DefaultHasher>::default().hash_one(s)
}

/// `YYYY-MM-DD…` → `YYYYMMDD` without parsing a date.
pub(crate) fn day_num(d: &str) -> Option<u32> {
    let b = d.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    [0, 1, 2, 3, 5, 6, 8, 9].iter().try_fold(0u32, |n, &i| {
        b[i].is_ascii_digit().then(|| n * 10 + (b[i] - b'0') as u32)
    })
}

/// One event of a `scan`: its local date (`YYYYMMDD`), time, the play rule and which views hold
/// it; the text columns are read only when a figure asks.
pub struct Pass<'r, 's> {
    row: &'r rusqlite::Row<'s>,
    pub day: u32,
    pub ms: i64,
    pub play: bool,
    /// In the base view (`reconciled_with IS NULL`).
    pub base: bool,
    /// A desktop (logger) row; else an export row.
    pub logger: bool,
}

impl Pass<'_, '_> {
    pub fn local(&self) -> rusqlite::Result<&str> {
        super::text(self.row, 0)
    }
    pub fn artist(&self) -> rusqlite::Result<Option<&str>> {
        super::text_opt(self.row, 3)
    }
    pub fn album(&self) -> rusqlite::Result<Option<&str>> {
        super::text_opt(self.row, 4)
    }
    pub fn track(&self) -> rusqlite::Result<Option<&str>> {
        super::text_opt(self.row, 5)
    }
    pub fn norm_key(&self) -> rusqlite::Result<Option<&str>> {
        super::text_opt(self.row, 6)
    }
    pub fn item_id(&self) -> rusqlite::Result<Option<i64>> {
        self.row.get(7)
    }
    /// Ended `trackdone` (export rows).
    pub fn trackdone(&self) -> rusqlite::Result<bool> {
        Ok(self.row.get::<_, Option<bool>>(10)? == Some(true))
    }
}

impl Src<'_> {
    /// The event is one this source counts (as `cte`: base, like for like, desktop rows).
    pub fn has(self, p: &Pass) -> bool {
        let sources = match self {
            Src::All => return p.base,
            Src::Desktop => return p.logger,
            Src::Like(c) => c.compared_sources.as_slice(),
        };
        match sources {
            ["S"] => p.logger,
            ["E"] => !p.logger,
            _ => p.base,
        }
    }
}

/// Every stored event (private sessions per setting) whose local date is in `s`, unordered, as one
/// SQL pass that several figures share (NFR-04: one read of the period instead of one per figure).
/// `f` picks the views it counts with `Src::has`.
pub fn scan<Tz: TimeZone>(cx: &Ctx<Tz>, s: Span, f: impl FnMut(&Pass) -> Result<()>) -> Result<()> {
    scan_rows(cx.conn, cx.include_private, s, None, f)
}

/// `scan` over the rows with ids in `ids` only (a part of the table, `scan_parts`).
fn scan_rows(
    conn: &Connection,
    include_private: bool,
    s: Span,
    ids: Option<(i64, i64)>,
    mut f: impl FnMut(&Pass) -> Result<()>,
) -> Result<()> {
    let private = if include_private {
        ""
    } else {
        " AND e.private_session = 0"
    };
    let id_range = ids.map_or(String::new(), |(a, b)| {
        format!(" AND e.id BETWEEN {a} AND {b}")
    });
    let mut st = conn.prepare(&format!(
        // unfiltered: base, like for like and desktop rows at once; `Src::has` applies each view
        "WITH ev AS NOT MATERIALIZED (SELECT e.*, COALESCE(e.ended_at_local, substr(e.ended_at_utc, 1, 19)) AS local
           FROM listening_event e WHERE 1{private}{id_range})
         SELECT local, ms_played, counts_as_play, artist_name, album_name, track_name, norm_key, item_id,
                reconciled_with IS NULL, source = 'logger', reason_end = 'trackdone'
         FROM ev WHERE {}",
        between_on(s, ids.is_some() || (s.1 - s.0).num_days() > 730)
    ))?;
    let mut rows = st.query([])?;
    while let Some(row) = rows.next()? {
        let Some(day) = day_num(super::text(row, 0)?) else {
            continue;
        };
        f(&Pass {
            row,
            day,
            ms: row.get(1)?,
            play: row.get::<_, i64>(2)? != 0,
            base: row.get(8)?,
            logger: row.get(9)?,
        })?;
    }
    Ok(())
}

/// `spans` as the stretches to read: overlapping or nearby spans (a month apart at most) together,
/// far ones apart (the newest period and the 12 months before the first play would otherwise read
/// the whole history).
fn segments(spans: &[Span]) -> Vec<Span> {
    let mut v = spans.to_vec();
    v.sort();
    let mut segs: Vec<Span> = vec![];
    for s in v {
        match segs.last_mut() {
            Some(l) if s.0 <= l.1 + TimeDelta::days(31) => l.1 = l.1.max(s.1),
            _ => segs.push(s),
        }
    }
    segs
}

/// `scan` over every date of `spans` once (`segments`).
pub fn scan_spans<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    spans: &[Span],
    mut f: impl FnMut(&Pass) -> Result<()>,
) -> Result<()> {
    for s in segments(spans) {
        scan(cx, s, &mut f)?;
    }
    Ok(())
}

/// Threads a long pass is split over: the logical CPUs, at most 8.
fn parts() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get().min(8))
}

/// A split pass is cut into this many pieces per thread, each thread taking the next one.
const PIECES_PER_PART: usize = 4;
/// A stretch at least this long is split over `parts()` threads (NFR-04).
const SPLIT_MIN_DAYS: i64 = 60;
/// Split stretches up to this long in all go by date, each thread reading its days on the time
/// index; longer ones by row id, each thread scanning its share of the table (a scan beats ~all
/// rows by index, TSD §3).
const BY_DATE_MAX_DAYS: i64 = 4 * 366;

/// Part `i` of `n` of `s`, by date (`None` when it has no days).
fn stretch(s: Span, i: i64, n: i64) -> Option<Span> {
    let days = (s.1 - s.0).num_days() + 1;
    let a = s.0 + TimeDelta::days(days * i / n);
    let b = s.0 + TimeDelta::days(days * (i + 1) / n) - TimeDelta::days(1);
    (a <= b).then_some((a, b))
}

/// How part `i` of `n` reads `s`: its days (time index), or all of it within a row id range.
#[derive(Clone, Copy)]
enum Split {
    Dates,
    Ids(i64, i64),
}

impl Split {
    /// The plan for long stretches `long`: `None` = read on `cx`'s connection, unsplit.
    fn plan<Tz: TimeZone>(cx: &Ctx<Tz>, long: &[Span]) -> Result<Option<(String, Split)>> {
        let path = cx.conn.path().filter(|p| !p.is_empty()).map(str::to_owned);
        let (Some(path), false, true) = (path, long.is_empty(), parts() > 1) else {
            return Ok(None);
        };
        let days: i64 = long.iter().map(|s| (s.1 - s.0).num_days() + 1).sum();
        if days <= BY_DATE_MAX_DAYS {
            return Ok(Some((path, Split::Dates)));
        }
        let ids: Option<(i64, i64)> = cx
            .conn
            // unfiltered: only the id range to split work; each part reads through base
            .query_row("SELECT MIN(id), MAX(id) FROM listening_event", [], |r| {
                Ok(r.get::<_, Option<i64>>(0)?.zip(r.get(1)?))
            })?;
        Ok(ids.map(|(lo, hi)| (path, Split::Ids(lo, hi))))
    }

    /// Part `i`'s piece of `s`: (span, row ids), `None` when it has none.
    fn part(self, s: Span, i: i64, n: i64) -> Option<(Span, Option<(i64, i64)>)> {
        match self {
            Split::Dates => stretch(s, i, n).map(|x| (x, None)),
            Split::Ids(lo, hi) => {
                let step = (hi - lo) / n + 1;
                Some((s, Some((lo + i * step, (lo + (i + 1) * step - 1).min(hi)))))
            }
        }
    }
}

/// `scan_spans` into copies of `acc`, one per part, for the caller to merge. Stretches of two
/// months or more are split over `parts()` threads (`Split`), each on its own read-only connection
/// to the library file (NFR-04: a whole history is 500k events); the rest is read meanwhile on
/// `cx`'s, as is everything for a library that isn't a file (tests).
pub fn scan_parts<Tz: TimeZone, A: Clone + Send>(
    cx: &Ctx<Tz>,
    spans: &[Span],
    acc: A,
    add: &(dyn Fn(&mut A, &Pass) -> Result<()> + Sync),
) -> Result<Vec<A>> {
    let segs = segments(spans);
    let (long, short): (Vec<Span>, Vec<Span>) = segs
        .iter()
        .partition(|s| (s.1 - s.0).num_days() >= SPLIT_MIN_DAYS);
    let Some((path, split)) = Split::plan(cx, &long)? else {
        let mut a = acc;
        for s in segs {
            scan(cx, s, |p| add(&mut a, p))?;
        }
        return Ok(vec![a]);
    };
    // more pieces than threads, taken in turn (NFR-04: rows cost unevenly across the table)
    let n = parts();
    let pieces = (n * PIECES_PER_PART) as i64;
    let next = std::sync::atomic::AtomicI64::new(0);
    let include_private = cx.include_private;
    std::thread::scope(|sc| {
        let handles: Vec<_> = (0..n)
            .map(|_| {
                let mut a = acc.clone();
                let (path, long, next) = (&path, &long, &next);
                sc.spawn(move || -> Result<A> {
                    let conn = open_read(path)?;
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if i >= pieces {
                            return Ok(a);
                        }
                        for (s, ids) in long.iter().filter_map(|s| split.part(*s, i, pieces)) {
                            scan_rows(&conn, include_private, s, ids, |p| add(&mut a, p))?;
                        }
                    }
                })
            })
            .collect();
        // the short stretches on this thread meanwhile
        let mut first = acc.clone();
        let mut out = vec![];
        for s in &short {
            if let Err(e) = scan(cx, *s, |p| add(&mut first, p)) {
                out.push(Err(e));
            }
        }
        out.push(Ok(first));
        for h in handles {
            out.push(
                h.join()
                    .unwrap_or_else(|_| Err(AppError::internal("A listening read stopped."))),
            );
        }
        out.into_iter().collect()
    })
}

/// The caller's own query over the events of `s` (`None`: every event), into one `A` per part,
/// for the caller to merge. `sql` gets the WHERE condition for its piece (`between`, and a row id
/// range when split by id; `1` for every event unsplit). Split over threads as `scan_parts`
/// (NFR-04); else one part on `cx`'s connection.
pub fn par_rows<Tz: TimeZone, A: Send>(
    cx: &Ctx<Tz>,
    s: Option<Span>,
    sql: &(dyn Fn(&str) -> String + Sync),
    init: &(dyn Fn() -> A + Sync),
    row: &(dyn Fn(&mut A, &rusqlite::Row) -> Result<()> + Sync),
) -> Result<Vec<A>> {
    let one = |conn: &Connection, w: &str, a: &mut A| -> Result<()> {
        let mut st = conn.prepare(&sql(w))?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            row(a, r)?;
        }
        Ok(())
    };
    // every event: as one stretch longer than `BY_DATE_MAX_DAYS`, so split by id
    let whole = s.unwrap_or_else(|| {
        let d = NaiveDate::from_ymd_opt(2000, 1, 1).expect("a date");
        (d, d + TimeDelta::days(BY_DATE_MAX_DAYS))
    });
    let long: Vec<Span> = [whole]
        .into_iter()
        .filter(|s| (s.1 - s.0).num_days() >= SPLIT_MIN_DAYS)
        .collect();
    let Some((path, split)) = Split::plan(cx, &long)? else {
        let mut a = init();
        one(cx.conn, &s.map_or("1".into(), between), &mut a)?;
        return Ok(vec![a]);
    };
    // more pieces than threads, taken in turn (as `scan_parts`)
    let n = parts();
    let pieces = (n * PIECES_PER_PART) as i64;
    let next = std::sync::atomic::AtomicI64::new(0);
    std::thread::scope(|sc| {
        let handles: Vec<_> = (0..n)
            .map(|_| {
                let (path, one, next) = (&path, &one, &next);
                sc.spawn(move || -> Result<A> {
                    let conn = open_read(path)?;
                    let mut a = init();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if i >= pieces {
                            return Ok(a);
                        }
                        let Some((x, ids)) = split.part(whole, i, pieces) else {
                            continue;
                        };
                        let w = match (ids, s) {
                            (Some((a, b)), Some(_)) => {
                                format!("{} AND id BETWEEN {a} AND {b}", between_on(x, true))
                            }
                            (Some((a, b)), None) => format!("id BETWEEN {a} AND {b}"),
                            (None, _) => between(x),
                        };
                        one(&conn, &w, &mut a)?;
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err(AppError::internal("A listening read stopped.")))
            })
            .collect()
    })
}

/// A job on its own read-only connection on another thread while the caller reads on (a library
/// file, NFR-04), or done right here (a library in memory: tests).
pub enum Aside<T> {
    Done(Result<T>),
    Running(std::thread::JoinHandle<Result<T>>),
}

impl<T: Send + 'static> Aside<T> {
    /// Starts `job(connection, include_private)`.
    pub fn run<Tz: TimeZone>(
        cx: &Ctx<Tz>,
        job: impl FnOnce(&Connection, bool) -> Result<T> + Send + 'static,
    ) -> Self {
        Self::on(cx.conn, cx.include_private, job)
    }

    /// `run` beside a reader that has only the connection.
    pub fn on(
        conn: &Connection,
        private: bool,
        job: impl FnOnce(&Connection, bool) -> Result<T> + Send + 'static,
    ) -> Self {
        match conn.path().filter(|p| !p.is_empty()).map(str::to_owned) {
            Some(path) => {
                Aside::Running(std::thread::spawn(move || job(&open_read(&path)?, private)))
            }
            None => Aside::Done(job(conn, private)),
        }
    }

    /// Starts `job` on a `Ctx` of its own (same setting, zone and now) beside the caller.
    pub fn run_cx<Tz: TimeZone + Send + 'static>(
        cx: &Ctx<Tz>,
        job: impl FnOnce(&Ctx<Tz>) -> Result<T> + Send + 'static,
    ) -> Self {
        let (tz, now) = (cx.tz.clone(), cx.now);
        Self::run(cx, move |conn, include_private| {
            job(&Ctx {
                conn,
                include_private,
                tz: &tz,
                now,
            })
        })
    }

    pub fn join(self) -> Result<T> {
        match self {
            Aside::Done(r) => r,
            Aside::Running(h) => h
                .join()
                .unwrap_or_else(|_| Err(AppError::internal("A listening read stopped."))),
        }
    }
}

/// A read-only connection to the library file, for a `scan_parts` thread (pragmas as `db::open`).
fn open_read(path: &str) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.pragma_update(None, "cache_size", -65536)?;
    conn.pragma_update(None, "mmap_size", 1i64 << 30)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

/// `YYYYMMDD` of a date.
pub fn num(d: NaiveDate) -> u32 {
    d.year() as u32 * 10_000 + d.month() * 100 + d.day()
}

/// A cheap hash of texts (8 bytes a step), for finding a text already seen; callers compare the
/// text itself on a hit, so a collision costs a retry, never a wrong count.
pub(crate) fn quick_hash(parts: &[Option<&str>]) -> u64 {
    const K: u64 = 0x517c_c1b7_2722_0a95;
    let mut h = 0u64;
    for p in parts {
        let Some(p) = p else {
            h = (h.rotate_left(5) ^ 0xfe).wrapping_mul(K);
            continue;
        };
        for c in p.as_bytes().chunks(8) {
            let mut w = [0u8; 8];
            w[..c.len()].copy_from_slice(c);
            h = (h.rotate_left(5) ^ u64::from_le_bytes(w)).wrapping_mul(K);
        }
        h = (h.rotate_left(5) ^ 0xff).wrapping_mul(K);
    }
    h
}

/// The distinct-count keys of one (artist, norm_key, album): `[artist, track, album]` hashes as
/// `aggs` counts them (`None` = empty, not counted).
#[derive(Clone)]
struct Keys {
    text: [String; 3],
    keys: [Option<u64>; 3],
}

/// `aggs_days` as a `scan` figure: `Agg` per window and `(ms, plays)` per date of `src`'s events.
/// Each distinct (artist, track, album) is hashed for the counts once, not once per play (NFR-04).
#[derive(Clone)]
pub struct AggAcc<'c> {
    src: Src<'c>,
    pub spans: Vec<Span>,
    bounds: Vec<(u32, u32)>,
    out: Vec<Agg>,
    /// quick hash (retried on a collision) → the texts and their count keys
    keys: IdMap<u64, Keys>,
    /// per window: the `keys` entries with a play
    sets: Vec<IdMap<u64, ()>>,
    by_day: IdMap<u32, (usize, i64, i64)>,
    buf: Vec<u8>,
}

impl<'c> AggAcc<'c> {
    pub fn new(src: Src<'c>, spans: &[Span]) -> Self {
        AggAcc {
            src,
            spans: spans.to_vec(),
            bounds: spans.iter().map(|s| (num(s.0), num(s.1))).collect(),
            out: vec![Agg::default(); spans.len()],
            keys: IdMap::default(),
            sets: (0..spans.len()).map(|_| Default::default()).collect(),
            by_day: IdMap::default(),
            buf: Vec::new(),
        }
    }

    pub fn add(&mut self, p: &Pass) -> Result<()> {
        if !self.src.has(p) {
            return Ok(());
        }
        let d = p.day;
        let Some(i) = self.bounds.iter().position(|(a, b)| *a <= d && d <= *b) else {
            return Ok(());
        };
        let a = &mut self.out[i];
        a.ms += p.ms;
        let day = self.by_day.entry(d).or_insert((i, 0, 0));
        day.1 += p.ms;
        day.2 += p.play as i64;
        if !p.logger {
            a.export_events += 1;
            a.finished += p.trackdone()? as i64;
        }
        if !p.play {
            return Ok(());
        }
        a.plays += 1;
        let k = self.key_of([
            p.artist()?.unwrap_or(""),
            p.norm_key()?.unwrap_or(""),
            p.album()?.unwrap_or(""),
        ]);
        self.sets[i].insert(k, ());
        Ok(())
    }

    /// The `keys` entry of (artist, norm_key, album), added when new.
    fn key_of(&mut self, text: [&str; 3]) -> u64 {
        let mut k = quick_hash(&text.map(Some));
        loop {
            match self.keys.get(&k) {
                Some(e) if e.text.iter().zip(text).all(|(x, y)| x == y) => return k,
                Some(_) => k = k.wrapping_add(0x9e37_79b9_7f4a_7c15),
                None => {
                    let [artist, key, album] = text;
                    let buf = &mut self.buf;
                    let keys = [
                        (!artist.is_empty()).then(|| folded_hash(buf, &[artist])),
                        // `norm_key` is compared as stored
                        (!key.is_empty()).then(|| exact_hash(key)),
                        (!album.is_empty()).then(|| folded_hash(buf, &[album, artist])),
                    ];
                    let text = text.map(str::to_owned);
                    self.keys.insert(k, Keys { text, keys });
                    return k;
                }
            }
        }
    }

    /// Adds another part's events (`scan_parts`).
    pub fn merge(&mut self, o: AggAcc) {
        for (a, b) in self.out.iter_mut().zip(&o.out) {
            a.ms += b.ms;
            a.plays += b.plays;
            a.export_events += b.export_events;
            a.finished += b.finished;
        }
        for (d, (i, ms, plays)) in o.by_day {
            let e = self.by_day.entry(d).or_insert((i, 0, 0));
            e.1 += ms;
            e.2 += plays;
        }
        for (w, set) in o.sets.iter().enumerate() {
            for k in set.keys() {
                let t = &o.keys[k].text;
                let k = self.key_of([&t[0], &t[1], &t[2]]);
                self.sets[w].insert(k, ());
            }
        }
    }

    pub fn finish(self) -> (Vec<Agg>, Days) {
        let mut out = self.out;
        let mut days = BTreeMap::new();
        for (d, (i, ms, plays)) in self.by_day {
            out[i].days += (plays > 0) as i64;
            if let Some(d) = NaiveDate::from_ymd_opt((d / 10_000) as i32, d / 100 % 100, d % 100) {
                days.insert(d, (ms, plays));
            }
        }
        for (a, set) in out.iter_mut().zip(self.sets) {
            let mut distinct: [IdMap<u64, ()>; 3] = Default::default();
            for k in set.keys() {
                for (d, key) in distinct.iter_mut().zip(self.keys[k].keys) {
                    if let Some(key) = key {
                        d.insert(key, ());
                    }
                }
            }
            let [artists, tracks, albums] = distinct;
            a.artists = artists.len() as i64;
            a.tracks = tracks.len() as i64;
            a.albums = albums.len() as i64;
        }
        (out, days)
    }
}

/// `aggs` plus `(ms, plays)` per local date inside the windows (as `daily`, same events).
pub fn aggs_days<Tz: TimeZone>(cx: &Ctx<Tz>, src: Src, spans: &[Span]) -> Result<(Vec<Agg>, Days)> {
    let mut acc = AggAcc::new(src, spans);
    scan_spans(cx, spans, |p| acc.add(p))?;
    Ok(acc.finish())
}

/// `Agg` over all time (range `all`, NFR-04): distinct counts from the 0005 `listening_rollup`
/// (one row per record and names), days from `listening_day`; only Finished reads the export rows.
pub fn aggs_all<Tz: TimeZone>(cx: &Ctx<Tz>) -> Result<Agg> {
    let p = cx.private_filter("private_session");
    let mut a = cx.conn.query_row(
        &format!(
            "SELECT IFNULL(SUM(ms_played), 0), IFNULL(SUM(counts_as_play), 0),
               COUNT(DISTINCT CASE WHEN counts_as_play > 0 AND artist_name <> '' THEN lower(artist_name) END),
               COUNT(DISTINCT CASE WHEN counts_as_play > 0 AND norm_key <> '' THEN norm_key END),
               COUNT(DISTINCT CASE WHEN counts_as_play > 0 AND album_name <> ''
                     THEN lower(album_name) || char(31) || lower(artist_name) END)
             FROM listening_rollup WHERE events > 0{p}"
        ),
        [],
        |r| {
            Ok(Agg {
                ms: r.get(0)?,
                plays: r.get(1)?,
                artists: r.get(2)?,
                tracks: r.get(3)?,
                albums: r.get(4)?,
                ..Agg::default()
            })
        },
    )?;
    a.days = cx.conn.query_row(
        &format!("SELECT COUNT(DISTINCT day) FROM listening_day WHERE plays > 0{p}"),
        [],
        |r| r.get(0),
    )?;
    (a.export_events, a.finished) = cx.conn.query_row(
        &format!(
            // unfiltered: base's filters by hand (reconciled rows are desktop rows, private per setting)
            "SELECT COUNT(*), IFNULL(SUM(reason_end = 'trackdone'), 0) FROM listening_event
             WHERE source = 'export'{p}"
        ),
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(a)
}

/// The span covers whole months of the stored data: it starts on a month's first day (or before
/// the first play) and ends on a month's last day (or from today on).
pub fn whole_months(s: Span, first: Option<NaiveDate>, today: NaiveDate) -> bool {
    (s.0.day() == 1 || first.is_some_and(|f| s.0 <= f)) && (s.1 == month_end(s.1) || s.1 >= today)
}

/// Per artist key (`lower(artist_name)`, none empty): `(ms, plays)` in `s`. Whole months over the
/// base view read `listening_month_artist` (`by_artist_rollup`); the rest is an `ArtistAcc` pass.
pub fn by_artist<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    src: Src,
    s: Span,
    first: Option<NaiveDate>,
) -> Result<HashMap<String, (i64, i64)>> {
    if by_artist_rollup(cx, src, s, first) {
        // summed here, not by GROUP BY (NFR-04: `artist` isn't the rollup's key prefix, so SQL sorts)
        let mut st = cx.conn.prepare(&format!(
            "SELECT artist, ms_played, plays FROM listening_month_artist
             WHERE month BETWEEN ?1 AND ?2 AND events > 0 AND artist <> ''{}",
            cx.private_filter("private_session")
        ))?;
        let mut out: HashMap<String, (i64, i64)> = HashMap::new();
        let mut rows = st.query([month(s.0), month(s.1)])?;
        while let Some(r) = rows.next()? {
            let a = super::text(r, 0)?;
            let (ms, plays): (i64, i64) = (r.get(1)?, r.get(2)?);
            match out.get_mut(a) {
                Some(e) => {
                    e.0 += ms;
                    e.1 += plays;
                }
                None => {
                    out.insert(a.to_owned(), (ms, plays));
                }
            }
        }
        return Ok(out);
    }
    let mut acc = ArtistAcc::new(src, s);
    scan_spans(cx, &[s], |p| acc.add(p))?;
    Ok(acc.out)
}

/// `by_artist` reads the 0008 rollup (whole months of the base view), not the events.
pub fn by_artist_rollup<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    src: Src,
    s: Span,
    first: Option<NaiveDate>,
) -> bool {
    src.is_base() && whole_months(s, first, cx.today())
}

/// `by_artist` over the events as a `scan` figure.
#[derive(Clone)]
pub struct ArtistAcc<'c> {
    src: Src<'c>,
    pub span: Span,
    bounds: (u32, u32),
    buf: String,
    pub out: HashMap<String, (i64, i64)>,
}

impl<'c> ArtistAcc<'c> {
    pub fn new(src: Src<'c>, s: Span) -> Self {
        ArtistAcc {
            src,
            span: s,
            bounds: (num(s.0), num(s.1)),
            buf: String::new(),
            out: HashMap::new(),
        }
    }

    pub fn add(&mut self, p: &Pass) -> Result<()> {
        if p.day < self.bounds.0 || p.day > self.bounds.1 || !self.src.has(p) {
            return Ok(());
        }
        let Some(a) = p.artist()?.filter(|a| !a.is_empty()) else {
            return Ok(());
        };
        // SQL's lower(): ASCII only
        self.buf.clear();
        self.buf.extend(a.chars().map(|c| c.to_ascii_lowercase()));
        if !self.out.contains_key(self.buf.as_str()) {
            self.out.insert(self.buf.clone(), (0, 0));
        }
        let e = self.out.get_mut(self.buf.as_str()).expect("inserted");
        e.0 += p.ms;
        e.1 += p.play as i64;
        Ok(())
    }

    /// Adds another part's events (`scan_parts`).
    pub fn merge(&mut self, o: ArtistAcc) {
        for (k, v) in o.out {
            let e = self.out.entry(k).or_default();
            e.0 += v.0;
            e.1 += v.1;
        }
    }
}

/// One month and artist of `listening_month_artist`: `(month, artist key, ms, plays)`.
pub type MonthArtist = (String, String, i64, i64);

/// `listening_month_artist` over the months `from..=to` (`YYYY-MM`, inclusive), artists named, rows
/// with events, private per setting: one row per month and artist (a private and a non-private row
/// summed), in (month, artist) order. Read in primary-key order and summed here (NFR-04: a GROUP BY
/// there costs several times the read).
pub fn month_artists<Tz: TimeZone>(cx: &Ctx<Tz>, from: &str, to: &str) -> Result<Vec<MonthArtist>> {
    let mut st = cx.conn.prepare(&format!(
        "SELECT month, artist, ms_played, plays FROM listening_month_artist
         WHERE month BETWEEN ?1 AND ?2 AND events > 0 AND artist <> ''{} ORDER BY month, artist",
        cx.private_filter("private_session")
    ))?;
    let mut out: Vec<MonthArtist> = Vec::new();
    let mut rows = st.query([from, to])?;
    while let Some(r) = rows.next()? {
        let (m, a) = (super::text(r, 0)?, super::text(r, 1)?);
        let (ms, plays): (i64, i64) = (r.get(2)?, r.get(3)?);
        match out.last_mut() {
            Some(l) if l.0 == m && l.1 == a => {
                l.2 += ms;
                l.3 += plays;
            }
            _ => out.push((m.to_owned(), a.to_owned(), ms, plays)),
        }
    }
    Ok(out)
}

/// `month_artists` over desktop plays only (`Src::Desktop`: reconciled ones included) in the local
/// dates `s`, every artist key ('' = unnamed), with each month's total time. FR-048: a comparison
/// past the export's end counts this PC on both sides.
pub fn desktop_month_artists<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    s: Span,
) -> Result<(Vec<MonthArtist>, HashMap<String, i64>)> {
    let mut st = cx.conn.prepare(&format!(
        "{} SELECT substr(local, 1, 7), lower(IFNULL(artist_name, '')), SUM(ms_played),
                  SUM(counts_as_play)
         FROM ev WHERE {} GROUP BY 1, 2 ORDER BY 1, 2",
        Src::Desktop.cte(cx.include_private),
        between(s)
    ))?;
    let (mut rows, mut totals) = (vec![], HashMap::new());
    for r in st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })? {
        let (m, a, ms, plays) = r?;
        *totals.entry(m.clone()).or_insert(0) += ms;
        if !a.is_empty() {
            rows.push((m, a, ms, plays));
        }
    }
    Ok((rows, totals))
}

/// An artist key's stored name (the newest spelling) and library record, for display.
pub fn artist_name(conn: &Connection, key: &str) -> Result<(String, Option<i64>, Option<Status>)> {
    let name: String = conn
        .query_row(
            // unfiltered: a display name only; 0004's index answers it
            "SELECT artist_name FROM listening_event WHERE lower(artist_name) = ?1
             ORDER BY ended_at_utc DESC LIMIT 1",
            [key],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_else(|| key.to_owned());
    let (id, status) = artist_item(conn, &name)?;
    Ok((name, id, status))
}

/// The library's artist record with this name (case-insensitive) and its status.
pub fn artist_item(conn: &Connection, name: &str) -> Result<(Option<i64>, Option<Status>)> {
    Ok(conn
        .query_row(
            "SELECT id, status FROM item WHERE type = 'artist' AND display_name = ?1 COLLATE NOCASE
             ORDER BY id LIMIT 1",
            [name],
            |r| Ok((Some(r.get(0)?), Some(r.get(1)?))),
        )
        .optional()?
        .unwrap_or((None, None)))
}

/// Every library artist by `lower(display_name)` → the lowest id and its status: `artist_item` for
/// many names in one pass (`COLLATE NOCASE` and `lower` both fold ASCII only).
pub fn artist_items(conn: &Connection) -> Result<HashMap<String, (i64, Status)>> {
    items_by_name(conn, "artist")
}

/// `artist_items` for any record type.
pub fn items_by_name(conn: &Connection, kind: &str) -> Result<HashMap<String, (i64, Status)>> {
    let mut out = HashMap::new();
    let mut st = conn.prepare(
        "SELECT lower(display_name), id, status FROM item
         WHERE type = ?1 AND display_name IS NOT NULL ORDER BY id DESC",
    )?;
    for r in st.query_map([kind], |r| {
        Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?))
    })? {
        let (k, id, status) = r?;
        out.insert(k, (id, status));
    }
    Ok(out)
}

/// Played local dates in `s` (≥ 1 play).
pub fn played(map: &BTreeMap<NaiveDate, (i64, i64)>) -> std::collections::BTreeSet<NaiveDate> {
    map.iter()
        .filter(|(_, v)| v.1 > 0)
        .map(|(d, _)| *d)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn q(grain: Grain, vs: Compare, at: Option<&str>) -> PeriodQuery {
        PeriodQuery {
            grain,
            vs,
            at: at.map(Into::into),
            from: None,
            to: None,
        }
    }

    #[test]
    fn header_resolves_and_validates() {
        let today = d("2026-10-19");
        let first = Some(d("2026-01-10"));
        // null at = the current period
        let r = resolve(&q(Grain::Month, Compare::Prev, None), today, first).unwrap();
        assert_eq!(r.cur, (d("2026-10-01"), d("2026-10-31")));
        assert_eq!(r.vs, Some(vec![(d("2026-09-01"), d("2026-09-30"))]));
        assert_eq!(r.steps(today, first), (Some("2026-09".into()), None));
        // a week by any day; stepping back stops before the first play
        let r = resolve(
            &q(Grain::Week, Compare::Usual, Some("2026-01-14")),
            today,
            first,
        )
        .unwrap();
        assert_eq!(r.cur, (d("2026-01-12"), d("2026-01-18")));
        assert_eq!(r.vs.as_ref().unwrap().len(), 12);
        assert_eq!(r.single_vs(), None);
        assert_eq!(
            r.steps(today, first),
            (Some("2026-01-05".into()), Some("2026-01-19".into()))
        );
        assert_eq!(
            resolve(
                &q(Grain::Week, Compare::Prev, Some("2026-01-05")),
                today,
                first
            )
            .unwrap()
            .steps(today, first)
            .0,
            None
        );
        let r = resolve(
            &q(Grain::Year, Compare::LastYear, Some("2025")),
            today,
            first,
        )
        .unwrap();
        assert_eq!(r.cur, (d("2025-01-01"), d("2025-12-31")));
        assert_eq!(r.single_vs(), Some((d("2024-01-01"), d("2024-12-31"))));
        assert_eq!(r.steps(today, first), (None, Some("2026".into())));
        // all time: first play .. today, nothing to compare, no stepper
        let r = resolve(&q(Grain::All, Compare::Prev, None), today, first).unwrap();
        assert_eq!((r.cur, r.vs.clone()), ((d("2026-01-10"), today), None));
        assert_eq!(r.steps(today, first), (None, None));
        // custom: both dates, from ≤ to
        let mut c = q(Grain::Custom, Compare::Prev, None);
        c.from = Some("2026-03-01".into());
        c.to = Some("2026-03-10".into());
        let r = resolve(&c, today, first).unwrap();
        assert_eq!(r.vs, Some(vec![(d("2026-02-19"), d("2026-02-28"))]));
        for (from, to) in [
            (Some("2026-03-10"), Some("2026-03-01")),
            (Some("2026-03-01"), None),
            (Some("2026-3-1"), Some("2026-03-02")),
        ] {
            c.from = from.map(Into::into);
            c.to = to.map(Into::into);
            assert_eq!(
                resolve(&c, today, first).unwrap_err().code,
                crate::error::Code::Validation
            );
        }
        // malformed at for its grain
        for (g, at) in [
            (Grain::Month, "2026-13"),
            (Grain::Month, "2026-10-01"),
            (Grain::Year, "26"),
            (Grain::Week, "2026-02-30"),
            (Grain::Year, "0999"),
        ] {
            assert_eq!(
                resolve(&q(g, Compare::Prev, Some(at)), today, first)
                    .unwrap_err()
                    .code,
                crate::error::Code::Validation,
                "{at}"
            );
        }
    }

    #[test]
    fn months_majority_and_alignment() {
        assert_eq!(
            months(d("2025-11-15"), d("2026-02-01")),
            vec![
                d("2025-11-01"),
                d("2025-12-01"),
                d("2026-01-01"),
                d("2026-02-01")
            ]
        );
        let class = |x: NaiveDate| {
            if x.day() <= 2 {
                CoverageClass::NoData
            } else {
                CoverageClass::ThisPc
            }
        };
        assert_eq!(
            majority(d("2026-01-01").iter_days().take(5), class),
            CoverageClass::ThisPc
        );
        assert_eq!(
            majority(d("2026-01-01").iter_days().take(4), class),
            CoverageClass::ThisPc
        ); // tie → this PC
        let today = d("2026-10-19");
        assert!(whole_months(
            (d("2026-10-01"), d("2026-10-31")),
            None,
            today
        ));
        assert!(whole_months(
            (d("2025-03-05"), today),
            Some(d("2025-03-05")),
            today
        ));
        assert!(!whole_months(
            (d("2026-10-05"), d("2026-10-11")),
            None,
            today
        ));
        assert_eq!(minute("2026-09-04T21:14:33"), "2026-09-04T21:14");
    }
}
