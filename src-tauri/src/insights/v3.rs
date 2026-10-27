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
    let plus = if (s.1 - s.0).num_days() > 730 {
        "+"
    } else {
        ""
    };
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

/// `aggs` plus `(ms, plays)` per local date inside the windows (as `daily`, same events).
pub fn aggs_days<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    src: Src,
    spans: &[Span],
) -> Result<(Vec<Agg>, Days)> {
    let mut out = vec![Agg::default(); spans.len()];
    if spans.is_empty() {
        return Ok((out, BTreeMap::new()));
    }
    let lo = spans.iter().map(|s| s.0).min().expect("non-empty");
    let hi = spans.iter().map(|s| s.1).max().expect("non-empty");
    let num = |d: NaiveDate| d.year() as u32 * 10_000 + d.month() * 100 + d.day();
    let bounds: Vec<(u32, u32)> = spans.iter().map(|s| (num(s.0), num(s.1))).collect();
    let mut sets: Vec<[std::collections::HashSet<u64>; 3]> =
        (0..spans.len()).map(|_| Default::default()).collect();
    let mut by_day: HashMap<u32, (usize, i64, i64)> = HashMap::new();
    let mut buf = Vec::new();
    let mut st = cx.conn.prepare(&format!(
        "{} SELECT local, ms_played, counts_as_play, IFNULL(artist_name, ''),
                IFNULL(norm_key, ''), IFNULL(album_name, ''), source = 'export', reason_end = 'trackdone'
         FROM ev WHERE {}",
        src.cte(cx.include_private),
        between((lo, hi))
    ))?;
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        let Some(d) = day_num(super::text(r, 0)?) else {
            continue;
        };
        let Some(i) = bounds.iter().position(|(a, b)| *a <= d && d <= *b) else {
            continue;
        };
        let a = &mut out[i];
        let ms = r.get::<_, i64>(1)?;
        let play = r.get::<_, i64>(2)? != 0;
        a.ms += ms;
        let day = by_day.entry(d).or_insert((i, 0, 0));
        day.1 += ms;
        day.2 += play as i64;
        if r.get::<_, Option<bool>>(6)? == Some(true) {
            a.export_events += 1;
            a.finished += (r.get::<_, Option<bool>>(7)? == Some(true)) as i64;
        }
        if !play {
            continue;
        }
        a.plays += 1;
        let (artist, key, album) = (super::text(r, 3)?, super::text(r, 4)?, super::text(r, 5)?);
        let [artists, tracks, albums] = &mut sets[i];
        if !artist.is_empty() {
            artists.insert(folded_hash(&mut buf, &[artist]));
        }
        if !key.is_empty() {
            // `norm_key` is compared as stored
            tracks.insert(std::hash::BuildHasher::hash_one(
                &std::hash::BuildHasherDefault::<std::hash::DefaultHasher>::default(),
                key,
            ));
        }
        if !album.is_empty() {
            albums.insert(folded_hash(&mut buf, &[album, artist]));
        }
    }
    let mut days = BTreeMap::new();
    for (d, (i, ms, plays)) in by_day {
        out[i].days += (plays > 0) as i64;
        if let Some(d) = NaiveDate::from_ymd_opt((d / 10_000) as i32, d / 100 % 100, d % 100) {
            days.insert(d, (ms, plays));
        }
    }
    for (a, [artists, tracks, albums]) in out.iter_mut().zip(sets) {
        a.artists = artists.len() as i64;
        a.tracks = tracks.len() as i64;
        a.albums = albums.len() as i64;
    }
    Ok((out, days))
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
/// base view read `listening_month_artist`.
pub fn by_artist<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    src: Src,
    s: Span,
    first: Option<NaiveDate>,
) -> Result<HashMap<String, (i64, i64)>> {
    let sql = if src.is_base() && whole_months(s, first, cx.today()) {
        format!(
            "SELECT artist, SUM(ms_played), SUM(plays) FROM listening_month_artist
             WHERE month BETWEEN '{}' AND '{}' AND events > 0 AND artist <> ''{} GROUP BY artist",
            month(s.0),
            month(s.1),
            cx.private_filter("private_session")
        )
    } else {
        format!(
            "{} SELECT lower(artist_name), SUM(ms_played), SUM(counts_as_play) FROM ev
             WHERE {} AND artist_name <> '' GROUP BY 1",
            src.cte(cx.include_private),
            between(s)
        )
    };
    let mut st = cx.conn.prepare(&sql)?;
    let rows = st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?),
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
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
