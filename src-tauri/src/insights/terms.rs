//! Insights v3 definitions (design/M10-insights-v3.md §4.2, TSD §4.14): one function and one
//! constant per term, so every figure the M10 commands show has a single tested meaning.
//! Periods and comparisons, basic counts, discovery, behaviour, fingerprint, records and tags live here; sessions and the
//! heuristics (eras, arcs, …) in `heuristics.rs`. Sources U, S, E only: events come from
//! `insights::base`, tags from `item_tag`; no cache DB handle (FR-031).
//!
//! Period membership is by **local** date (the design's calendar periods). An artist is keyed by
//! `artist_key` (ASCII lower case, as SQLite's `lower()` in the 0008 rollups).
// ponytail: the terms work on events loaded into memory; WI 5 swaps a term to the 0008 rollups
// where NFR-04 needs it, keeping these as the reference the rollup answer is tested against.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::{DateTime, Datelike, Months, NaiveDate, NaiveDateTime, TimeDelta, Utc};
use rusqlite::Connection;
use serde::Serialize;

use super::{SRC, base, pct};
use crate::error::Result;
use crate::importer::events::Source;

/// Play rule (D25): an event counts as a play from 30 s.
pub const PLAY_MS: i64 = 30_000;
/// Stick rate: the window starts this many days after the first play …
pub const STICK_AFTER_DAYS: i64 = 30;
/// … and lasts this long …
pub const STICK_WINDOW_DAYS: i64 = 90;
/// … and needs this many plays.
pub const STICK_PLAYS: usize = 3;
/// Discovery "now": dropped after this many days without a play.
pub const DROPPED_DAYS: i64 = 90;
/// Quick bail: an export event shorter than this, ended with the next button …
pub const QUICK_BAIL_MS: i64 = 10_000;
/// … ranked when a track has at least this many.
pub const QUICK_BAIL_MIN: usize = 3;
/// Rows shown for quick bails.
pub const QUICK_BAIL_TOP: usize = 10;
/// Fingerprint Focus: the period's top artists.
pub const FOCUS_TOP: usize = 10;
/// Next milestone: plays are counted in steps of this many …
pub const MILESTONE_STEP: i64 = 5_000;
/// … and the ETA uses the pace of this many days.
pub const MILESTONE_PACE_DAYS: i64 = 90;
/// Records: forgotten favourites have had no play for this many days …
pub const FORGOTTEN_DAYS: i64 = 730;
/// … after being in a year's top this many.
pub const FORGOTTEN_TOP: usize = 10;
/// Records: most loyal = in a year's top this many in the most years.
pub const LOYAL_TOP: usize = 20;

/// Every `reason_start` value seen in a real export (kb/spotify.md); the synthetic fixture holds each.
pub const SEEN_REASON_START: &[&str] = &[
    "trackdone",
    "clickrow",
    "fwdbtn",
    "playbtn",
    "appload",
    "remote",
    "unknown",
    "backbtn",
    "trackerror",
    "switched-to-video",
    "switched-to-audio",
];
/// Every `reason_end` value seen in a real export (kb/spotify.md); the synthetic fixture holds each.
pub const SEEN_REASON_END: &[&str] = &[
    "trackdone",
    "endplay",
    "fwdbtn",
    "logout",
    "remote",
    "unexpected-exit-while-paused",
    "unknown",
    "backbtn",
    "unexpected-exit",
    "trackerror",
    "switched-to-video",
];

/// One event of the base view (reconciled logger rows out, private sessions per setting).
#[derive(Debug, Clone, PartialEq)]
pub struct Ev {
    pub id: i64,
    pub source: Source,
    pub end: DateTime<Utc>,
    /// Local end time (UTC when unknown or unparseable).
    pub local: NaiveDateTime,
    pub ms: i64,
    pub track: String,
    pub artist: String,
    pub album: String,
    pub norm_key: String,
    pub platform: Option<String>,
    pub reason_start: Option<String>,
    pub reason_end: Option<String>,
    pub shuffle: Option<bool>,
    pub skipped: Option<bool>,
    pub offline: Option<bool>,
    pub item_id: Option<i64>,
}

impl Ev {
    pub fn date(&self) -> NaiveDate {
        self.local.date()
    }
    pub fn export(&self) -> bool {
        self.source == Source::Export
    }
}

/// `{col}` (local text) between two local dates, inclusive, plus a padded UTC bound that keeps
/// SQLite on the time index (local is at most a day off UTC). Params `?1` = from, `?2` = to.
fn local_between() -> &'static str {
    "substr(local, 1, 10) BETWEEN ?1 AND ?2
     AND ended_at_utc >= date(?1, '-1 day') AND ended_at_utc < date(?2, '+2 day')"
}

fn day(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// Stored `ended_at_utc` and `local` text → times; an unparseable local time falls back to UTC
/// (G-29), an unparseable UTC one to the epoch (never written: the importer and logger format it).
fn times(utc: &str, local: &str) -> (DateTime<Utc>, NaiveDateTime) {
    let p = |s: &str| {
        s.get(..19)
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
    };
    let end = p(utc).unwrap_or_default().and_utc();
    (end, p(local).unwrap_or(end.naive_utc()))
}

/// The base view's events with a local date in `from..=to`, in end order.
pub fn load(
    conn: &Connection,
    include_private: bool,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<Ev>> {
    let sql = format!(
        "{} SELECT id, source, ended_at_utc, local, ms_played, IFNULL(track_name, ''),
                IFNULL(artist_name, ''), IFNULL(album_name, ''), IFNULL(norm_key, ''), platform,
                reason_start, reason_end, shuffle, skipped, offline, item_id
         FROM base WHERE {} ORDER BY ended_at_utc, id",
        base(include_private),
        local_between()
    );
    let mut st = conn.prepare(&sql)?;
    let rows = st.query_map([day(from), day(to)], |r| {
        let (end, local) = times(&r.get::<_, String>(2)?, &r.get::<_, String>(3)?);
        Ok(Ev {
            id: r.get(0)?,
            source: if r.get::<_, String>(1)? == "export" {
                Source::Export
            } else {
                Source::Logger
            },
            end,
            local,
            ms: r.get(4)?,
            track: r.get(5)?,
            artist: r.get(6)?,
            album: r.get(7)?,
            norm_key: r.get(8)?,
            platform: r.get(9)?,
            reason_start: r.get(10)?,
            reason_end: r.get(11)?,
            shuffle: r.get(12)?,
            skipped: r.get(13)?,
            offline: r.get(14)?,
            item_id: r.get(15)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// ---------- Basic counts

/// Play: `counts_as_play` (≥ 30 s, D25).
pub fn is_play(e: &Ev) -> bool {
    e.ms >= PLAY_MS
}

/// Listening time: Σ `ms_played` of all events.
pub fn listening_ms(evs: &[Ev]) -> i64 {
    evs.iter().map(|e| e.ms).sum()
}

/// The artist identity: its name, ASCII-lower-cased like SQLite `lower()` (0008 rollups).
pub fn artist_key(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// Days listened: local dates with ≥ 1 play.
pub fn days_listened(evs: &[Ev]) -> BTreeSet<NaiveDate> {
    evs.iter().filter(|e| is_play(e)).map(Ev::date).collect()
}

/// Coverage class of a local date (design §4.4); WI 4 decides it, the terms only read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageClass {
    AllDevices,
    ThisPc,
    NoData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Streak {
    pub days: i64,
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// Streaks over `from..=to`: consecutive covered dates with ≥ 1 play. A no-data date is skipped
/// (breaks nothing, counts nothing); a covered date without a play breaks the run.
/// Returns (longest, the run still going at `to`: the current streak, 0 when broken).
pub fn streaks(
    from: NaiveDate,
    to: NaiveDate,
    played: &BTreeSet<NaiveDate>,
    class: impl Fn(NaiveDate) -> CoverageClass,
) -> (Option<Streak>, i64) {
    let (mut run, mut best): (Option<Streak>, Option<Streak>) = (None, None);
    for d in from.iter_days().take_while(|d| *d <= to) {
        if class(d) == CoverageClass::NoData {
            continue;
        }
        run = played.contains(&d).then(|| match run {
            Some(r) => Streak {
                days: r.days + 1,
                to: d,
                ..r
            },
            None => Streak {
                days: 1,
                from: d,
                to: d,
            },
        });
        if let Some(r) = run
            && best.is_none_or(|b| r.days > b.days)
        {
            best = Some(r);
        }
    }
    (best, run.map_or(0, |r| r.days))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ratio {
    pub tracks: i64,
    pub albums: i64,
    pub artists: i64,
    pub tracks_per_artist: f64,
    pub albums_per_artist: f64,
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn div(a: f64, b: f64) -> f64 {
    if b == 0.0 { 0.0 } else { a / b }
}

/// Music ratio: distinct tracks (`norm_key`), albums (album + artist) and artists with ≥ 1 play,
/// and tracks / albums per artist (one decimal).
pub fn ratio(evs: &[Ev]) -> Ratio {
    let plays = || evs.iter().filter(|e| is_play(e));
    let tracks: HashSet<&str> = plays()
        .filter(|e| !e.norm_key.is_empty())
        .map(|e| e.norm_key.as_str())
        .collect();
    let albums: HashSet<(String, String)> = plays()
        .filter(|e| !e.album.is_empty())
        .map(|e| (e.album.to_ascii_lowercase(), artist_key(&e.artist)))
        .collect();
    let artists: HashSet<String> = plays()
        .filter(|e| !e.artist.is_empty())
        .map(|e| artist_key(&e.artist))
        .collect();
    let n = artists.len() as f64;
    Ratio {
        tracks: tracks.len() as i64,
        albums: albums.len() as i64,
        artists: artists.len() as i64,
        tracks_per_artist: round1(div(tracks.len() as f64, n)),
        albums_per_artist: round1(div(albums.len() as f64, n)),
    }
}

// ---------- Periods and comparisons

/// Period: a local calendar week (Mon–Sun), month or year holding a date; all time (first play ..
/// today); or a custom range. Local dates, inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Week(NaiveDate),
    Month(NaiveDate),
    Year(NaiveDate),
    All(NaiveDate, NaiveDate),
    Custom(NaiveDate, NaiveDate),
}

/// Local dates `from..=to`.
pub type Span = (NaiveDate, NaiveDate);

/// Comparison: previous (the period before), same last year, usual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    Prev,
    LastYear,
    Usual,
}

/// Usual: the median of this many periods of the same length before this one.
pub const USUAL_PERIODS: i32 = 12;
/// Same last year for a week: this many weeks back (weekdays line up) …
pub const LAST_YEAR_WEEKS: i32 = 52;
/// … and for a custom range this many days back (same weekdays, as the mock).
pub const LAST_YEAR_DAYS: i64 = 364;

fn add_months(d: NaiveDate, n: i32) -> Option<NaiveDate> {
    let m = Months::new(n.unsigned_abs());
    if n >= 0 {
        d.checked_add_months(m)
    } else {
        d.checked_sub_months(m)
    }
}

impl Period {
    /// The period's local dates.
    pub fn span(self) -> Span {
        match self {
            Period::Week(at) => {
                let mon = at - TimeDelta::days(at.weekday().num_days_from_monday().into());
                (mon, mon + TimeDelta::days(6))
            }
            Period::Month(at) => {
                let first = at.with_day(1).expect("day 1 exists");
                (first, first + Months::new(1) - TimeDelta::days(1))
            }
            Period::Year(at) => (
                NaiveDate::from_ymd_opt(at.year(), 1, 1).expect("1 Jan exists"),
                NaiveDate::from_ymd_opt(at.year(), 12, 31).expect("31 Dec exists"),
            ),
            Period::All(a, b) | Period::Custom(a, b) => (a, b),
        }
    }

    /// The period of the same kind and length `n` steps later (negative = earlier); `None` for all
    /// time or out of the calendar's range.
    pub fn shift(self, n: i32) -> Option<Period> {
        let (a, b) = self.span();
        Some(match self {
            Period::Week(_) => Period::Week(a.checked_add_signed(TimeDelta::weeks(n.into()))?),
            Period::Month(_) => Period::Month(add_months(a, n)?),
            Period::Year(_) => Period::Year(add_months(a, n.checked_mul(12)?)?),
            Period::Custom(..) => {
                let d = TimeDelta::days(((b - a).num_days() + 1) * i64::from(n));
                Period::Custom(a.checked_add_signed(d)?, b.checked_add_signed(d)?)
            }
            Period::All(..) => return None,
        })
    }
}

/// The comparison periods: one for previous / same last year, `USUAL_PERIODS` (newest first) for
/// usual, none for all time.
pub fn compare_with(p: Period, vs: Compare) -> Vec<Span> {
    let back = |n: i32| p.shift(-n).map(Period::span);
    match vs {
        Compare::Prev => back(1).into_iter().collect(),
        Compare::LastYear => match p {
            Period::Week(_) => back(LAST_YEAR_WEEKS),
            Period::Month(_) => back(12),
            Period::Year(_) => back(1),
            Period::Custom(a, b) => {
                let d = TimeDelta::days(LAST_YEAR_DAYS);
                a.checked_sub_signed(d).zip(b.checked_sub_signed(d))
            }
            Period::All(..) => None,
        }
        .into_iter()
        .collect(),
        Compare::Usual => (1..=USUAL_PERIODS).filter_map(back).collect(),
    }
}

/// Usual's value: the median of the comparison periods' values (even count: mean of the middle
/// two); `None` when there are none.
pub fn median(xs: &[f64]) -> Option<f64> {
    let mut s = xs.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len();
    (n > 0).then(|| (s[(n - 1) / 2] + s[n / 2]) / 2.0)
}

// ---------- Discovery

/// An artist's first play (≥ 30 s) in all stored data (0008 `artist_first_play`); a shorter event
/// before it doesn't count.
#[derive(Debug, Clone, PartialEq)]
pub struct FirstHeard {
    pub artist: String,
    pub name: String,
    pub end: DateTime<Utc>,
    pub local: NaiveDateTime,
    pub track: String,
}

/// First heard, per artist, from the 0008 rollup (with private sessions included, the earlier of
/// the two rows). Newest first.
pub fn first_heard(conn: &Connection, include_private: bool) -> Result<Vec<FirstHeard>> {
    let sql = format!(
        "{} SELECT a.artist, e.artist_name, e.ended_at_utc, e.local, IFNULL(e.track_name, ''), e.id
         FROM artist_first_play a JOIN base e ON e.id = a.event_id
         ORDER BY e.ended_at_utc, e.id",
        base(include_private)
    );
    let mut st = conn.prepare(&sql)?;
    let mut seen = HashSet::new();
    let mut out = vec![];
    for r in st.query_map([], |r| {
        let (end, local) = times(&r.get::<_, String>(2)?, &r.get::<_, String>(3)?);
        Ok(FirstHeard {
            artist: r.get(0)?,
            name: r.get(1)?,
            end,
            local,
            track: r.get(4)?,
        })
    })? {
        let f = r?;
        // ordered by time: the first row per artist is the earlier one
        if seen.insert(f.artist.clone()) {
            out.push(f);
        }
    }
    out.reverse();
    Ok(out)
}

/// New artists: those whose first play in all stored data falls in `from..=to` (local dates).
pub fn new_artists(first: &[FirstHeard], from: NaiveDate, to: NaiveDate) -> Vec<&FirstHeard> {
    first
        .iter()
        .filter(|f| (from..=to).contains(&f.local.date()))
        .collect()
}

/// Stick: ≥ 3 plays in the 90 days that start 30 days after the first play. `None` while that
/// window hasn't ended at `now` (estimate; labelled).
pub fn stuck(first: NaiveDateTime, plays: &[NaiveDateTime], now: NaiveDateTime) -> Option<bool> {
    let a = first + TimeDelta::days(STICK_AFTER_DAYS);
    let b = a + TimeDelta::days(STICK_WINDOW_DAYS);
    (now >= b).then(|| plays.iter().filter(|t| (a..b).contains(*t)).count() >= STICK_PLAYS)
}

/// Stick rate: % of new artists (window over) that stuck; `None` when no window is over yet.
pub fn stick_rate(outcomes: impl IntoIterator<Item = Option<bool>>) -> Option<f64> {
    let closed: Vec<bool> = outcomes.into_iter().flatten().collect();
    (!closed.is_empty()).then(|| {
        pct(
            closed.iter().filter(|s| **s).count() as i64,
            closed.len() as i64,
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NowState {
    InRotation,
    Dropped,
    BackAgain,
}

/// Discovery log "now": back again when the arc says so, dropped after 90 days without a play,
/// else in rotation.
pub fn discovery_now(back_again: bool, last_play: NaiveDateTime, now: NaiveDateTime) -> NowState {
    if back_again {
        NowState::BackAgain
    } else if last_play < now - TimeDelta::days(DROPPED_DAYS) {
        NowState::Dropped
    } else {
        NowState::InRotation
    }
}

// ---------- Behaviour (export plays only)

/// Finished: % of export events with `reason_end = 'trackdone'`; `None` when there is none.
pub fn finished_pct(evs: &[Ev]) -> Option<f64> {
    let ex: Vec<&Ev> = evs.iter().filter(|e| e.export()).collect();
    (!ex.is_empty()).then(|| {
        pct(
            ex.iter()
                .filter(|e| e.reason_end.as_deref() == Some("trackdone"))
                .count() as i64,
            ex.len() as i64,
        )
    })
}

/// Skip rate (G-45): `skipped = 1` ÷ events **with a skip flag**. Desktop rows have none.
pub fn skip_rate(evs: &[Ev]) -> f64 {
    let flagged = evs.iter().filter_map(|e| e.skipped);
    let (n, s) = flagged.fold((0, 0), |(n, s), k| (n + 1, s + k as i64));
    pct(s, n)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuickBail {
    pub track: String,
    pub artist: String,
    pub item_id: Option<i64>,
    pub times: usize,
}

/// Quick bails: export events under 10 s ended with the next button; tracks with ≥ 3, most first.
pub fn quick_bails(evs: &[Ev]) -> Vec<QuickBail> {
    let mut by: HashMap<&str, QuickBail> = HashMap::new();
    for e in evs
        .iter()
        .filter(|e| e.export() && e.ms < QUICK_BAIL_MS && e.reason_end.as_deref() == Some("fwdbtn"))
    {
        let b = by.entry(&e.norm_key).or_insert_with(|| QuickBail {
            track: e.track.clone(),
            artist: e.artist.clone(),
            item_id: None,
            times: 0,
        });
        b.times += 1;
        b.item_id = b.item_id.or(e.item_id);
    }
    let mut out: Vec<_> = by
        .into_values()
        .filter(|b| b.times >= QUICK_BAIL_MIN)
        .collect();
    out.sort_by(|a, b| b.times.cmp(&a.times).then_with(|| a.track.cmp(&b.track)));
    out.truncate(QUICK_BAIL_TOP);
    out
}

/// How a track started (export only): *how*, not *from where*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartKind {
    /// Started by you: `clickrow`, `playbtn`.
    Picked,
    /// Followed on: `trackdone`.
    Followed,
    Back,
    Other,
}

pub fn start_kind(reason_start: &str) -> StartKind {
    match reason_start {
        "clickrow" | "playbtn" => StartKind::Picked,
        "trackdone" => StartKind::Followed,
        "backbtn" => StartKind::Back,
        _ => StartKind::Other,
    }
}

/// How a play ended (export only): `trackdone` / `fwdbtn` / `backbtn` / the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndKind {
    Finished,
    Skipped,
    Back,
    Stopped,
}

pub fn end_kind(reason_end: &str) -> EndKind {
    match reason_end {
        "trackdone" => EndKind::Finished,
        "fwdbtn" => EndKind::Skipped,
        "backbtn" => EndKind::Back,
        _ => EndKind::Stopped,
    }
}

/// % of export events per kind (in kind order); `[]` when there are none.
pub fn kind_shares<K: Ord + Copy>(evs: &[Ev], kind: impl Fn(&Ev) -> Option<K>) -> Vec<(K, f64)> {
    let mut n: BTreeMap<K, i64> = BTreeMap::new();
    for k in evs.iter().filter(|e| e.export()).filter_map(kind) {
        *n.entry(k).or_default() += 1;
    }
    let total = n.values().sum();
    n.into_iter().map(|(k, c)| (k, pct(c, total))).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceClass {
    Phone,
    Computer,
    Web,
    ThisPc,
    Other,
}

/// Device class: export `platform` by prefix (case-insensitive); desktop-log plays are This PC.
pub fn device(source: Source, platform: Option<&str>) -> DeviceClass {
    if source == Source::Logger {
        return DeviceClass::ThisPc;
    }
    let p = platform.unwrap_or_default().trim().to_ascii_lowercase();
    let is = |prefixes: &[&str]| prefixes.iter().any(|x| p.starts_with(x));
    if is(&["android", "ios"]) {
        DeviceClass::Phone
    } else if is(&["windows", "osx", "linux"]) {
        DeviceClass::Computer
    } else if is(&["web"]) {
        // `web`, `webplayer`
        DeviceClass::Web
    } else {
        DeviceClass::Other
    }
}

// ---------- Fingerprint (0–100 each; on screen only, never exported or shared)

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Fingerprint {
    pub regularity: f64,
    pub discovery: f64,
    pub focus: f64,
    pub variety: f64,
    pub repeat: f64,
}

fn score(x: f64) -> f64 {
    (x * 100.0).round().clamp(0.0, 100.0)
}

/// The five measures for a period's events. `covered_days` = days in the period minus no-data
/// days; `new` = new artists' keys; `known` = `norm_key`s played before the period.
pub fn fingerprint(
    evs: &[Ev],
    covered_days: i64,
    new: &HashSet<String>,
    known: &HashSet<String>,
) -> Fingerprint {
    let total = listening_ms(evs) as f64;
    let mut by_artist: HashMap<String, i64> = HashMap::new();
    for e in evs.iter().filter(|e| !e.artist.is_empty()) {
        *by_artist.entry(artist_key(&e.artist)).or_default() += e.ms;
    }
    let mut times: Vec<i64> = by_artist.values().copied().collect();
    times.sort_unstable_by(|a, b| b.cmp(a));
    let top: i64 = times.iter().take(FOCUS_TOP).sum();
    let on_new: i64 = by_artist
        .iter()
        .filter(|(a, _)| new.contains(*a))
        .map(|(_, ms)| ms)
        .sum();
    let artist_total: f64 = times.iter().sum::<i64>() as f64;
    let entropy: f64 = times
        .iter()
        .filter(|ms| **ms > 0)
        .map(|ms| {
            let p = *ms as f64 / artist_total;
            -p * p.ln()
        })
        .sum();
    let variety = if times.len() > 1 {
        entropy / (times.len() as f64).ln()
    } else {
        0.0
    };
    let plays: Vec<&Ev> = evs.iter().filter(|e| is_play(e)).collect();
    let again = plays.iter().filter(|e| known.contains(&e.norm_key)).count();
    Fingerprint {
        regularity: score(div(days_listened(evs).len() as f64, covered_days as f64)),
        discovery: score(div(on_new as f64, total)),
        focus: score(div(top as f64, total)),
        variety: score(variety),
        repeat: score(div(again as f64, plays.len() as f64)),
    }
}

// ---------- Records and tags

/// The largest value (the earliest key on a tie): biggest day, biggest year.
pub fn biggest<K: Ord + Clone>(by: &BTreeMap<K, i64>) -> Option<(K, i64)> {
    by.iter()
        .filter(|(_, v)| **v > 0)
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(k, v)| (k.clone(), *v))
}

/// Most artists in a month: `(month, artist, plays)` rows → the month with the most artists
/// that have ≥ 1 play (earliest on a tie).
pub fn most_artists_month(rows: &[(String, String, i64)]) -> Option<(String, i64)> {
    let mut n: BTreeMap<&str, i64> = BTreeMap::new();
    for (m, a, p) in rows {
        if *p > 0 && !a.is_empty() {
            *n.entry(m).or_default() += 1;
        }
    }
    biggest(&n).map(|(m, c)| (m.to_owned(), c))
}

/// Each year's top `n` artists by listening time (`(year, artist, ms)` rows; ties by name).
pub fn top_per_year(rows: &[(i32, String, i64)], n: usize) -> BTreeMap<i32, Vec<String>> {
    let mut by: BTreeMap<i32, Vec<(&str, i64)>> = BTreeMap::new();
    for (y, a, ms) in rows.iter().filter(|r| r.2 > 0 && !r.1.is_empty()) {
        by.entry(*y).or_default().push((a, *ms));
    }
    by.into_iter()
        .map(|(y, mut v)| {
            v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            (
                y,
                v.into_iter().take(n).map(|(a, _)| a.to_owned()).collect(),
            )
        })
        .collect()
}

/// Most loyal: the artist in the top 20 in the most years → (artist, years in it, years).
pub fn most_loyal(top20: &BTreeMap<i32, Vec<String>>) -> Option<(String, usize, usize)> {
    let mut n: BTreeMap<&str, usize> = BTreeMap::new();
    for a in top20.values().flatten() {
        *n.entry(a).or_default() += 1;
    }
    n.into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(a, k)| (a.to_owned(), k, top20.len()))
}

/// Forgotten favourites: in some year's top 10, no play for ≥ 730 days → (artist, the latest
/// such year), most recent year first.
pub fn forgotten(
    top10: &BTreeMap<i32, Vec<String>>,
    last_play: &HashMap<String, NaiveDateTime>,
    now: NaiveDateTime,
) -> Vec<(String, i32)> {
    let mut best: BTreeMap<&str, i32> = BTreeMap::new();
    for (y, artists) in top10 {
        for a in artists {
            best.insert(a, *y); // years ascend: the last write is the latest year
        }
    }
    let cut = now - TimeDelta::days(FORGOTTEN_DAYS);
    let mut out: Vec<(String, i32)> = best
        .into_iter()
        .filter(|(a, _)| last_play.get(*a).is_some_and(|t| *t <= cut))
        .map(|(a, y)| (a.to_owned(), y))
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// Next milestone: the next multiple of 5,000 plays, and the days to it at the last-90-day pace
/// (`None` with no plays in those days).
pub fn next_milestone(total: i64, plays_last_90: i64) -> (i64, Option<i64>) {
    let next = (total / MILESTONE_STEP + 1) * MILESTONE_STEP;
    let eta = (plays_last_90 > 0).then(|| {
        let left = (next - total) * MILESTONE_PACE_DAYS;
        (left + plays_last_90 - 1) / plays_last_90
    });
    (next, eta)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagShare {
    pub tag: String,
    pub share_pct: f64,
}

/// Top tags (U): each play linked to a record (`item_id`, then its `on_album` / `by_artist`
/// links) carries the tags of all those records, once each, weighted by listening time.
/// Returns the top `n` tags as % of tagged time, and covered = tagged time ÷ all time (%).
pub fn top_tags(
    conn: &Connection,
    include_private: bool,
    from: NaiveDate,
    to: NaiveDate,
    n: usize,
) -> Result<(Vec<TagShare>, f64)> {
    let sql = format!(
        "{}{SRC}, ev AS (SELECT id, item_id, ms_played FROM base WHERE {})
         SELECT (SELECT IFNULL(SUM(ms_played), 0) FROM ev),
                ev.id, ev.ms_played, t.name
         FROM ev JOIN src ON src.s = ev.item_id JOIN item_tag it ON it.item_id = src.t
         JOIN tag t ON t.id = it.tag_id
         GROUP BY ev.id, t.id",
        base(include_private),
        local_between()
    );
    let mut st = conn.prepare(&sql)?;
    let (mut total, mut tagged) = (0i64, HashMap::<i64, i64>::new());
    let mut by_tag: HashMap<String, i64> = HashMap::new();
    for r in st.query_map([day(from), day(to)], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (all, id, ms, tag) = r?;
        total = all;
        tagged.insert(id, ms);
        *by_tag.entry(tag).or_default() += ms;
    }
    let tagged: i64 = tagged.values().sum();
    let mut rows: Vec<(String, i64)> = by_tag.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let rows = rows
        .into_iter()
        .take(n)
        .map(|(tag, ms)| TagShare {
            tag,
            share_pct: pct(ms, tagged),
        })
        .collect();
    if total == 0 {
        // no tagged row returned the total: count it directly
        total = conn.query_row(
            &format!(
                "{} SELECT IFNULL(SUM(ms_played), 0) FROM base WHERE {}",
                base(include_private),
                local_between()
            ),
            [day(from), day(to)],
            |r| r.get(0),
        )?;
    }
    Ok((rows, pct(tagged, total)))
}

/// Test events: export, local = UTC, a play unless `ms` says otherwise.
#[cfg(test)]
pub(crate) fn ev(local: &str, ms: i64, artist: &str, track: &str) -> Ev {
    let t = NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M:%S").unwrap();
    Ev {
        id: 0,
        source: Source::Export,
        end: t.and_utc(),
        local: t,
        ms,
        track: track.into(),
        artist: artist.into(),
        album: format!("{artist} LP"),
        norm_key: format!("{}|{}", artist.to_lowercase(), track.to_lowercase()),
        platform: Some("windows".into()),
        reason_start: Some("trackdone".into()),
        reason_end: Some("trackdone".into()),
        shuffle: Some(false),
        skipped: Some(false),
        offline: Some(false),
        item_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::fixture;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn t(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    #[test]
    fn plays_time_days_and_ratio() {
        let evs = vec![
            ev("2026-05-01T10:00:00", 30_000, "Aster", "One"), // play (exactly 30 s)
            ev("2026-05-01T11:00:00", 29_999, "Aster", "Two"), // not a play
            ev("2026-05-02T10:00:00", 200_000, "aster", "One"), // same track, other case
            ev("2026-05-03T10:00:00", 100_000, "Birch", "Three"),
            ev("2026-05-04T10:00:00", 10_000, "Cedar", "Four"), // not a play: no day, no artist
        ];
        assert!(is_play(&evs[0]) && !is_play(&evs[1]));
        assert_eq!(listening_ms(&evs), 369_999);
        assert_eq!(
            days_listened(&evs).into_iter().collect::<Vec<_>>(),
            vec![d("2026-05-01"), d("2026-05-02"), d("2026-05-03")]
        );
        // tracks aster|one, birch|three; albums (aster lp, aster), (birch lp, birch); artists 2
        assert_eq!(
            ratio(&evs),
            Ratio {
                tracks: 2,
                albums: 2,
                artists: 2,
                tracks_per_artist: 1.0,
                albums_per_artist: 1.0,
            }
        );
        assert_eq!(artist_key("ÄBBA Band"), "Äbba band"); // ASCII only, as SQLite lower()
    }

    #[test]
    fn streaks_skip_no_data_and_break_on_silence() {
        use CoverageClass::*;
        // 1–3 played, 4 no data, 5–6 played, 7 covered silence, 8 played, 9 no data
        let played: BTreeSet<_> = ["01", "02", "03", "05", "06", "08"]
            .iter()
            .map(|x| d(&format!("2026-05-{x}")))
            .collect();
        let class = |x: NaiveDate| match x.format("%d").to_string().as_str() {
            "04" | "09" => NoData,
            "08" => ThisPc,
            _ => AllDevices,
        };
        let (best, current) = streaks(d("2026-05-01"), d("2026-05-09"), &played, class);
        assert_eq!(
            best,
            Some(Streak {
                days: 5, // 1, 2, 3, (4 skipped), 5, 6
                from: d("2026-05-01"),
                to: d("2026-05-06"),
            })
        );
        assert_eq!(current, 1); // 8, then 9 is no data: still going
        // ending on a covered silent day: current streak 0
        assert_eq!(
            streaks(d("2026-05-01"), d("2026-05-07"), &played, class).1,
            0
        );
        assert_eq!(
            streaks(d("2026-05-04"), d("2026-05-04"), &played, class),
            (None, 0)
        );
    }

    #[test]
    fn periods_and_comparisons() {
        let s = |a: &str, b: &str| (d(a), d(b));
        // a week crossing the year: Thu 1 Jan 2026 and Sun 4 Jan are both in Mon 29 Dec .. Sun 4 Jan
        let w = Period::Week(d("2026-01-01"));
        assert_eq!(w.span(), s("2025-12-29", "2026-01-04"));
        assert_eq!(Period::Week(d("2026-01-04")).span(), w.span());
        assert_eq!(Period::Week(d("2025-12-29")).span(), w.span());
        assert_eq!(
            compare_with(w, Compare::Prev),
            vec![s("2025-12-22", "2025-12-28")]
        );
        assert_eq!(
            compare_with(w, Compare::LastYear),
            vec![s("2024-12-30", "2025-01-05")] // 52 weeks: Monday again
        );
        // Feb in a leap year, and last year's Feb
        let feb = Period::Month(d("2024-02-10"));
        assert_eq!(feb.span(), s("2024-02-01", "2024-02-29"));
        assert_eq!(
            compare_with(feb, Compare::LastYear),
            vec![s("2023-02-01", "2023-02-28")]
        );
        assert_eq!(
            compare_with(Period::Month(d("2024-03-31")), Compare::Prev),
            vec![s("2024-02-01", "2024-02-29")]
        );
        // usual: the 12 months before Jan 2026, newest first
        let u = compare_with(Period::Month(d("2026-01-15")), Compare::Usual);
        assert_eq!(u.len(), 12);
        assert_eq!(u[0], s("2025-12-01", "2025-12-31"));
        assert_eq!(u[11], s("2025-01-01", "2025-01-31"));
        let y = Period::Year(d("2024-07-01"));
        assert_eq!(y.span(), s("2024-01-01", "2024-12-31"));
        assert_eq!(
            compare_with(y, Compare::LastYear),
            vec![s("2023-01-01", "2023-12-31")]
        );
        assert_eq!(
            compare_with(y, Compare::Usual)[11],
            s("2012-01-01", "2012-12-31")
        );
        // custom: 10 days; previous = the 10 before, last year = 364 days back, usual 12 × 10 back
        let c = Period::Custom(d("2026-03-01"), d("2026-03-10"));
        assert_eq!(c.span(), s("2026-03-01", "2026-03-10"));
        assert_eq!(
            compare_with(c, Compare::Prev),
            vec![s("2026-02-19", "2026-02-28")]
        );
        assert_eq!(
            compare_with(c, Compare::LastYear),
            vec![s("2025-03-02", "2025-03-11")]
        );
        assert_eq!(
            compare_with(c, Compare::Usual)[11],
            s("2025-11-01", "2025-11-10") // 120 days back
        );
        // all time: its span, nothing to compare with
        let all = Period::All(d("2019-04-02"), d("2026-10-24"));
        assert_eq!(all.span(), s("2019-04-02", "2026-10-24"));
        for vs in [Compare::Prev, Compare::LastYear, Compare::Usual] {
            assert!(compare_with(all, vs).is_empty());
        }
        // median of 12 values (one outlier): middle two are 6 and 7
        let twelve = [
            5.0, 1.0, 9.0, 100.0, 3.0, 7.0, 2.0, 8.0, 4.0, 6.0, 11.0, 10.0,
        ];
        assert_eq!(median(&twelve), Some(6.5));
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn new_artists_and_stick_rate() {
        let f = |a: &str, at: &str| FirstHeard {
            artist: a.into(),
            name: a.into(),
            end: t(at).and_utc(),
            local: t(at),
            track: "x".into(),
        };
        let first = [
            f("old", "2025-12-31T23:00:00"),
            f("new", "2026-01-15T08:00:00"),
        ];
        let n = new_artists(&first, d("2026-01-01"), d("2026-01-31"));
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].artist, "new");
        // window for a first play on 1 Jan: 31 Jan (incl.) .. 1 May (excl.)
        let first_play = t("2026-01-01T00:00:00");
        let now = t("2026-06-01T00:00:00");
        let plays = |xs: &[&str]| xs.iter().map(|x| t(x)).collect::<Vec<_>>();
        let three = plays(&[
            "2026-01-31T00:00:00",
            "2026-03-01T00:00:00",
            "2026-04-30T23:59:59",
        ]);
        assert_eq!(stuck(first_play, &three, now), Some(true));
        // too early (before day 30) and too late (day 120) don't count
        let edges = plays(&[
            "2026-01-30T23:59:59",
            "2026-03-01T00:00:00",
            "2026-03-02T00:00:00",
            "2026-05-01T00:00:00",
        ]);
        assert_eq!(stuck(first_play, &edges, now), Some(false));
        assert_eq!(stuck(first_play, &three, t("2026-04-30T00:00:00")), None);
        assert_eq!(
            stick_rate([Some(true), Some(false), None, Some(true)]),
            Some(66.7)
        );
        assert_eq!(stick_rate([None]), None);
        // discovery "now"
        let now = t("2026-06-01T00:00:00");
        assert_eq!(
            discovery_now(false, t("2026-03-03T00:00:00"), now),
            NowState::InRotation
        );
        assert_eq!(
            discovery_now(false, t("2026-03-02T23:59:59"), now),
            NowState::Dropped
        );
        assert_eq!(
            discovery_now(true, t("2026-03-02T00:00:00"), now),
            NowState::BackAgain
        );
    }

    #[test]
    fn first_heard_reads_the_rollup() {
        let (c, _) = fixture::build();
        let f = first_heard(&c, false).unwrap();
        // Zorblax first in 2025 (e6), Nobody 1 Jun (e8); Quuxley's only event (e3) is 20 s, not a
        // play, so it has no first play; private e5 hidden
        let got: Vec<(&str, &str)> = f
            .iter()
            .map(|x| (x.artist.as_str(), x.track.as_str()))
            .collect();
        assert_eq!(got, vec![("nobody", "Twin"), ("zorblax", "Song Two")]);
        let names = |from: &str, to: &str| -> Vec<String> {
            new_artists(&f, d(from), d(to))
                .iter()
                .map(|x| x.name.clone())
                .collect()
        };
        assert_eq!(names("2026-06-01", "2026-06-07"), vec!["Nobody"]);
        assert!(names("2026-10-12", "2026-10-18").is_empty());
        // the only private event (e5, Zorblax 2026) is later than Zorblax's first: same answer
        assert_eq!(first_heard(&c, true).unwrap(), f);
    }

    #[test]
    fn behaviour_terms() {
        let mut evs = vec![
            ev("2026-05-01T10:00:00", 200_000, "A", "One"), // finished, not skipped
            ev("2026-05-01T10:01:00", 5_000, "A", "Two"),
            ev("2026-05-01T10:02:00", 4_000, "A", "Two"),
            ev("2026-05-01T10:03:00", 9_999, "A", "Two"),
            ev("2026-05-01T10:04:00", 10_000, "A", "Three"), // 10 s: not a quick bail
            ev("2026-05-01T10:05:00", 100_000, "A", "Four"), // desktop: no flag, no reasons
        ];
        for e in &mut evs[1..5] {
            e.reason_end = Some("fwdbtn".into());
            e.reason_start = Some("clickrow".into());
            e.skipped = Some(true);
        }
        evs[4].reason_start = Some("backbtn".into());
        evs[5].source = Source::Logger;
        evs[5].skipped = None;
        evs[5].reason_end = None;
        evs[5].reason_start = None;
        // G-45: 4 skipped of the 5 events with a flag (the desktop row is left out, not 4 of 6)
        assert_eq!(skip_rate(&evs), 80.0);
        assert_eq!(skip_rate(&evs[5..]), 0.0);
        assert_eq!(finished_pct(&evs), Some(20.0)); // 1 of 5 export events
        assert_eq!(finished_pct(&evs[5..]), None);
        let qb = quick_bails(&evs);
        assert_eq!(
            qb,
            vec![QuickBail {
                track: "Two".into(),
                artist: "A".into(),
                item_id: None,
                times: 3
            }]
        );
        evs[3].ms = 10_000; // only 2 left: below the ranking threshold
        assert!(quick_bails(&evs).is_empty());
        assert_eq!(
            kind_shares(&evs, |e| e.reason_start.as_deref().map(start_kind)),
            vec![
                (StartKind::Picked, 60.0),
                (StartKind::Followed, 20.0),
                (StartKind::Back, 20.0)
            ]
        );
        assert_eq!(
            kind_shares(&evs, |e| e.reason_end.as_deref().map(end_kind)),
            vec![(EndKind::Finished, 20.0), (EndKind::Skipped, 80.0)]
        );
        assert!(kind_shares(&evs[5..], |e| e.reason_end.as_deref().map(end_kind)).is_empty());
    }

    #[test]
    fn every_seen_reason_has_a_kind_and_is_in_the_fixture() {
        use StartKind::*;
        let starts: Vec<_> = SEEN_REASON_START.iter().map(|r| start_kind(r)).collect();
        assert_eq!(
            starts,
            vec![
                Followed, Picked, Other, Picked, Other, Other, Other, Back, Other, Other, Other
            ]
        );
        let ends: Vec<_> = SEEN_REASON_END.iter().map(|r| end_kind(r)).collect();
        assert_eq!(ends.iter().filter(|k| **k == EndKind::Stopped).count(), 8);
        let e = crate::importer::source::read(&crate::importer::source::fixture()).unwrap();
        let p = crate::importer::streaming::parse(
            &e.streaming,
            &mut crate::importer::streaming::LocalTime::system(),
        )
        .unwrap();
        let has = |f: fn(&crate::importer::events::Event) -> Option<&String>, v: &str| {
            p.events.iter().any(|e| f(e).map(String::as_str) == Some(v))
        };
        for r in SEEN_REASON_START {
            assert!(
                has(|e| e.reason_start.as_ref(), r),
                "fixture lacks reason_start {r}"
            );
        }
        for r in SEEN_REASON_END {
            assert!(
                has(|e| e.reason_end.as_ref(), r),
                "fixture lacks reason_end {r}"
            );
        }
    }

    #[test]
    fn device_classes_over_seen_platforms() {
        use DeviceClass::*;
        let x = |p: &str| device(Source::Export, Some(p));
        // kb/spotify.md seen values, plus the fixture's longer forms
        assert_eq!(x("windows"), Computer);
        assert_eq!(x("Windows 10 (10.0.19041; x64)"), Computer);
        assert_eq!(x("osx"), Computer);
        assert_eq!(x("linux"), Computer);
        assert_eq!(x("android"), Phone);
        assert_eq!(x("ios"), Phone);
        assert_eq!(x("iOS 17.1 (iPhone15,2)"), Phone);
        assert_eq!(x("web_player windows"), Web);
        assert_eq!(x("webplayer"), Web);
        assert_eq!(x("unknown"), Other);
        assert_eq!(x(""), Other);
        assert_eq!(device(Source::Export, None), Other);
        assert_eq!(device(Source::Logger, Some("windows")), ThisPc);
    }

    #[test]
    fn fingerprint_measures() {
        // 4 covered days, listened on 2; A 300 s (new), B 100 s; "A|one" known before
        let evs = vec![
            ev("2026-05-01T10:00:00", 200_000, "A", "One"),
            ev("2026-05-01T11:00:00", 100_000, "A", "Two"),
            ev("2026-05-03T10:00:00", 100_000, "B", "Three"),
        ];
        let new: HashSet<String> = ["a".into()].into();
        let known: HashSet<String> = ["a|one".into()].into();
        let f = fingerprint(&evs, 4, &new, &known);
        // variety: p = .75/.25 → H = 0.5623, ÷ ln 2 = 0.811
        assert_eq!(
            f,
            Fingerprint {
                regularity: 50.0,
                discovery: 75.0,
                focus: 100.0,
                variety: 81.0,
                repeat: 33.0,
            }
        );
        let one = fingerprint(&evs[..1], 0, &HashSet::new(), &HashSet::new());
        assert_eq!((one.variety, one.regularity, one.repeat), (0.0, 0.0, 0.0));
        let empty = fingerprint(&[], 0, &new, &known);
        assert_eq!(empty.focus, 0.0);
    }

    #[test]
    fn records() {
        let days: BTreeMap<String, i64> = [("2026-01-01", 5), ("2026-01-02", 9), ("2026-01-03", 9)]
            .map(|(k, v)| (k.to_owned(), v))
            .into();
        assert_eq!(biggest(&days), Some(("2026-01-02".to_owned(), 9))); // earliest on a tie
        assert_eq!(biggest(&BTreeMap::<i32, i64>::from([(2020, 0)])), None);
        let rows = |xs: &[(&str, &str, i64)]| {
            xs.iter()
                .map(|(m, a, p)| (m.to_string(), a.to_string(), *p))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            most_artists_month(&rows(&[
                ("2026-01", "a", 1),
                ("2026-01", "b", 0),
                ("2026-02", "a", 2),
                ("2026-02", "b", 1),
                ("2026-02", "", 9),
            ])),
            Some(("2026-02".into(), 2))
        );
        let years: Vec<(i32, String, i64)> = [
            (2020, "a", 30),
            (2020, "b", 20),
            (2020, "c", 10),
            (2021, "b", 50),
            (2021, "c", 40),
            (2022, "c", 5),
        ]
        .iter()
        .map(|(y, a, ms)| (*y, a.to_string(), *ms))
        .collect();
        let top2 = top_per_year(&years, 2);
        assert_eq!(top2[&2020], vec!["a", "b"]);
        assert_eq!(top2[&2021], vec!["b", "c"]);
        // b and c are in the top 2 in 2 of 3 years: the tie goes to the first name
        assert_eq!(most_loyal(&top2), Some(("b".into(), 2, 3)));
        let now = t("2026-06-01T00:00:00");
        let last: HashMap<String, NaiveDateTime> = [
            ("a".to_owned(), t("2024-06-01T00:00:00")), // exactly 730 days before: forgotten
            ("b".to_owned(), t("2024-06-01T00:00:01")), // a second later: not yet
            ("c".to_owned(), t("2020-01-01T00:00:00")),
        ]
        .into();
        assert_eq!(
            forgotten(&top2, &last, now),
            vec![("c".into(), 2022), ("a".into(), 2020)]
        );
        assert_eq!(next_milestone(12_340, 900), (15_000, Some(266))); // 2,660 at 10/day
        assert_eq!(next_milestone(5_000, 0), (10_000, None));
        assert_eq!(next_milestone(0, 1), (5_000, Some(450_000)));
    }

    #[test]
    fn top_tags_weight_linked_plays_by_time() {
        let (c, _) = fixture::build();
        // Song One (T1) 'calm'; Album One 'calm' + 'late'; artist 'late'
        c.execute_batch(
            "INSERT INTO tag(id, name) VALUES (1, 'calm'), (2, 'late');
             INSERT INTO item_tag(item_id, tag_id) VALUES (10, 1), (2, 1), (2, 2), (1, 2);",
        )
        .unwrap();
        // 12–18 Oct, not private: e1 200 s (T1), e2 100 s (T1), e3 20 s (no record); e4 reconciled
        let (rows, covered) = top_tags(&c, false, d("2026-10-12"), d("2026-10-18"), 6).unwrap();
        assert_eq!(
            rows,
            vec![
                TagShare {
                    tag: "calm".into(),
                    share_pct: 100.0
                },
                TagShare {
                    tag: "late".into(),
                    share_pct: 100.0
                },
            ]
        );
        assert_eq!(covered, pct(300_000, 320_000)); // 93.8
        // nothing tagged in the period: no rows, 0 %, still no error
        assert_eq!(
            top_tags(&c, false, d("2026-06-01"), d("2026-06-02"), 6).unwrap(),
            (vec![], 0.0)
        );
    }

    #[test]
    fn load_reads_the_base_view_by_local_date() {
        let (c, ids) = fixture::build();
        let evs = load(&c, false, d("2026-10-12"), d("2026-10-18")).unwrap();
        // e3, e2, e1 in end order; e4 reconciled, e5 private, e7 on 10 Oct
        assert_eq!(
            evs.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![ids[2], ids[1], ids[0]]
        );
        assert_eq!(skip_rate(&evs), 66.7);
        let with_private = load(&c, true, d("2026-10-12"), d("2026-10-18")).unwrap();
        assert_eq!(with_private.len(), 4);
    }
}
