//! Period tabs past the Report (API.md §4.1): `get_discovery` (design §5 Discovery) and
//! `get_listening_habits` (How you listen: export plays only, except devices and sessions).
//! Stick rate, sessions, deep dives and start to finish are estimates (design §4.2).
use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, NaiveDate, NaiveDateTime, TimeDelta, TimeZone};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use super::heuristics::{self, ARC_MONTHS, DiveKind, back_again_at};
use super::report::{Compared, new_artists_compared};
use super::terms::{
    self, CoverageClass, DeviceClass, EndKind, Ev, FirstHeard, NowState, QUICK_BAIL_MIN,
    QUICK_BAIL_MS, QUICK_BAIL_TOP, StartKind, device, discovery_now, end_kind, start_kind,
    stick_rate, stick_window, stuck_by,
};
use super::v3::{
    Ctx, PeriodQuery, Src, artist_item, artist_items, between, by_artist, covered, day, majority,
    minute, month, month_end, month_start, months, period_coverage, resolve,
};
use super::{Covered, base, pct};
use crate::error::Result;
use crate::importer::events::Source;

/// Discovery: per-month bars go back at least this many months.
pub const PER_MONTH_MIN: u32 = 24;
/// The discovery log's length.
pub const LOG_MAX: usize = 100;
/// Best stick-rate year: needs at least this many artists whose window has closed.
pub const STICK_BEST_MIN: usize = 5;
/// Most skipped: among the period's top this many artists.
pub const MOST_SKIPPED_TOP: usize = 20;
/// Deep dives and on-repeat days shown.
pub const DIVES_MAX: usize = 10;
/// Session length buckets (minutes): 10 buckets.
pub const SESSION_EDGES_MIN: [i64; 9] = [5, 15, 30, 45, 60, 90, 120, 180, 240];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StickRate {
    pub pct: Option<f64>,
    pub best_pct: Option<f64>,
    pub best_year: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MonthNew {
    pub month: String,
    pub stuck: i64,
    pub dropped: i64,
    pub open: i64,
    pub coverage: CoverageClass,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Share<K> {
    pub kind: K,
    pub pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiscoveryRow {
    pub artist: String,
    pub item_id: Option<i64>,
    pub first_played_local: String,
    pub first_track: String,
    pub plays_since: i64,
    pub now: NowState,
}

/// API.md §4.1 `Discovery`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Discovery {
    pub new_artists: Compared,
    pub days_per_new: Option<f64>,
    pub stick_rate: StickRate,
    pub new_time_pct: f64,
    pub artists_known: i64,
    pub known_since: Option<String>,
    pub per_month: Vec<MonthNew>,
    pub starts: Vec<Share<StartKind>>,
    pub log: Vec<DiscoveryRow>,
}

/// Stick outcome per artist key (`None` = window not over): plays ≥ 30 s in the window after the
/// first play, counted on 0004's index.
fn stick_outcomes<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    heard: &[FirstHeard],
) -> Result<HashMap<String, Option<bool>>> {
    let now = cx.now.naive_utc();
    let mut st = cx.conn.prepare(&format!(
        "{} SELECT COUNT(*) FROM base WHERE lower(artist_name) = ?1 AND ended_at_utc >= ?2
           AND ended_at_utc < ?3 AND counts_as_play",
        base(cx.include_private)
    ))?;
    let fmt = |t: NaiveDateTime| t.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut out = HashMap::new();
    for f in heard {
        let (a, b) = stick_window(f.end.naive_utc());
        let outcome = if now < b {
            None
        } else {
            let n: i64 = st.query_row([&f.artist, &fmt(a), &fmt(b)], |r| r.get(0))?;
            stuck_by(b, n as usize, now)
        };
        out.insert(f.artist.clone(), outcome);
    }
    Ok(out)
}

/// `(month total ms, artist ms)` for `keys` over the 36 arc months ending `to`'s month.
fn arc_series<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    keys: &[&str],
    to: NaiveDate,
) -> Result<HashMap<String, Vec<(i64, i64)>>> {
    let ms = months(
        month_start(to) - chrono::Months::new(ARC_MONTHS as u32 - 1),
        to,
    );
    let (a, b) = (month(ms[0]), month(to));
    let p = cx.private_filter("private_session");
    let mut totals: HashMap<String, i64> = HashMap::new();
    let mut st = cx.conn.prepare(&format!(
        "SELECT substr(day, 1, 7), SUM(ms_played) FROM listening_day
         WHERE day BETWEEN '{a}-01' AND '{b}-31' AND events > 0{p} GROUP BY 1"
    ))?;
    for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (m, v) = r?;
        totals.insert(m, v);
    }
    let idx: HashMap<String, usize> = ms.iter().enumerate().map(|(i, m)| (month(*m), i)).collect();
    let mut out: HashMap<String, Vec<(i64, i64)>> = keys
        .iter()
        .map(|k| {
            (
                (*k).to_owned(),
                ms.iter()
                    .map(|m| (0, totals.get(&month(*m)).copied().unwrap_or(0)))
                    .collect(),
            )
        })
        .collect();
    if keys.is_empty() {
        return Ok(out);
    }
    // one range scan of the months for every key (NFR-04: `artist` isn't the rollup's key prefix)
    let mut st = cx.conn.prepare(&format!(
        "SELECT artist, month, SUM(ms_played) FROM listening_month_artist
         WHERE month BETWEEN '{a}' AND '{b}' AND events > 0{p} AND artist IN ({}) GROUP BY 1, 2",
        vec!["?"; keys.len()].join(", ")
    ))?;
    let rows = st.query_map(rusqlite::params_from_iter(keys), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    for r in rows {
        let (k, m, v) = r?;
        if let (Some(&i), Some(s)) = (idx.get(&m), out.get_mut(&k)) {
            s[i].0 = v;
        }
    }
    Ok(out)
}

/// `get_discovery`.
pub fn discovery<Tz: TimeZone>(cx: &Ctx<Tz>, q: &PeriodQuery) -> Result<Covered<Discovery>> {
    let today = cx.today();
    let first = super::coverage::first_day(cx.conn)?;
    let r = resolve(q, today, first)?;
    let end = r.cur.1.min(today);
    let per_from = month_start(
        r.cur
            .0
            .min(month_start(today) - chrono::Months::new(PER_MONTH_MIN - 1)),
    );
    let back = (r.earliest() - per_from).num_days().max(0);
    let (model, cov) = period_coverage(cx, &r, back)?;
    let vs = r.vs.clone().unwrap_or_default();
    let heard = terms::first_heard(cx.conn, cx.include_private)?;
    let new_artists = new_artists_compared(&heard, &cov, r.cur, &vs);
    let in_cur: Vec<&FirstHeard> = heard
        .iter()
        .filter(|f| (r.cur.0..=r.cur.1).contains(&f.local.date()))
        .collect();
    let covered_days = r
        .cur
        .0
        .iter_days()
        .take_while(|d| *d <= end)
        .filter(|d| model.class(*d) != CoverageClass::NoData)
        .count();
    let outcomes = stick_outcomes(cx, &heard)?;

    // Stick rate: the period's new artists, and the best year (enough closed windows).
    let pct_now = stick_rate(in_cur.iter().map(|f| outcomes[&f.artist]));
    let mut by_year: BTreeMap<i32, Vec<bool>> = BTreeMap::new();
    for f in &heard {
        if let Some(s) = outcomes[&f.artist] {
            by_year.entry(f.local.year()).or_default().push(s);
        }
    }
    let best = by_year
        .iter()
        .filter(|(_, v)| v.len() >= STICK_BEST_MIN)
        .filter_map(|(y, v)| Some((*y, stick_rate(v.iter().map(|s| Some(*s)))?)))
        .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(&a.0)));

    let artists = by_artist(cx, Src::All, r.cur, first)?;
    let total: i64 = artists.values().map(|v| v.0).sum();
    let new_keys: HashSet<&str> = in_cur.iter().map(|f| f.artist.as_str()).collect();
    let new_ms: i64 = artists
        .iter()
        .filter(|(k, _)| new_keys.contains(k.as_str()))
        .map(|(_, v)| v.0)
        .sum();

    let per_month = months(per_from, end)
        .into_iter()
        .map(|m| {
            let me = month_end(m).min(end);
            let class = majority(m.iter_days().take_while(|d| *d <= me), |d| model.class(d));
            let (mut stuck, mut dropped, mut open) = (0, 0, 0);
            if class != CoverageClass::NoData {
                for f in heard.iter().filter(|f| (m..=me).contains(&f.local.date())) {
                    match outcomes[&f.artist] {
                        Some(true) => stuck += 1,
                        Some(false) => dropped += 1,
                        None => open += 1,
                    }
                }
            }
            MonthNew {
                month: month(m),
                stuck,
                dropped,
                open,
                coverage: class,
            }
        })
        .collect();

    // How you started tracks (export only).
    let mut st = cx.conn.prepare(&format!(
        "{} SELECT reason_start, COUNT(*) FROM ev WHERE {} AND source = 'export' GROUP BY 1",
        Src::All.cte(cx.include_private),
        between(r.cur)
    ))?;
    let mut starts: BTreeMap<StartKind, i64> = BTreeMap::new();
    for row in st.query_map([], |r| {
        Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
    })? {
        let (k, n) = row?;
        *starts
            .entry(start_kind(k.as_deref().unwrap_or("")))
            .or_default() += n;
    }
    let n_starts: i64 = starts.values().sum();
    let mut starts: Vec<Share<StartKind>> = starts
        .into_iter()
        .map(|(kind, n)| Share {
            kind,
            pct: pct(n, n_starts),
        })
        .collect();
    starts.sort_by(|a, b| b.pct.total_cmp(&a.pct));

    // The discovery log: newest first.
    let log_rows: Vec<&FirstHeard> = in_cur.iter().take(LOG_MAX).copied().collect();
    let keys: Vec<&str> = log_rows.iter().map(|f| f.artist.as_str()).collect();
    let series = arc_series(cx, &keys, today)?;
    // Plays since the first one = all its plays (nothing counts before an artist's first play), from
    // the 0008 rollup; the last play by the 0004 index from the newest end (NFR-04: no scan per artist).
    let since: HashMap<String, i64> = if keys.is_empty() {
        HashMap::new()
    } else {
        cx.conn
            .prepare(&format!(
                "SELECT artist, SUM(plays) FROM listening_month_artist WHERE artist IN ({}){}
                 GROUP BY 1",
                vec!["?"; keys.len()].join(", "),
                cx.private_filter("private_session")
            ))?
            .query_map(rusqlite::params_from_iter(&keys), |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<rusqlite::Result<_>>()?
    };
    let items = artist_items(cx.conn)?;
    let mut last_play = cx.conn.prepare(&format!(
        "{} SELECT local FROM base WHERE lower(artist_name) = ?1 AND counts_as_play
         ORDER BY ended_at_utc DESC, id DESC LIMIT 1",
        base(cx.include_private)
    ))?;
    let now_local = cx.now.with_timezone(cx.tz).naive_local();
    let mut log = vec![];
    for f in log_rows {
        let plays = since.get(&f.artist).copied().unwrap_or(0);
        let last: Option<String> = last_play.query_row([&f.artist], |r| r.get(0)).optional()?;
        let last = last
            .and_then(|s| NaiveDateTime::parse_from_str(s.get(..19)?, "%Y-%m-%dT%H:%M:%S").ok())
            .unwrap_or(f.local);
        let back_again = series
            .get(&f.artist)
            .is_some_and(|s| back_again_at(s).is_some());
        log.push(DiscoveryRow {
            artist: f.name.clone(),
            item_id: items.get(&f.name.to_ascii_lowercase()).map(|x| x.0),
            first_played_local: minute(&f.local.format("%Y-%m-%dT%H:%M:%S").to_string()),
            first_track: f.track.clone(),
            plays_since: plays,
            now: discovery_now(back_again, last, now_local),
        });
    }
    let data = Discovery {
        days_per_new: (new_artists.value > 0.0)
            .then(|| (covered_days as f64 / new_artists.value * 10.0).round() / 10.0),
        new_artists,
        stick_rate: StickRate {
            pct: pct_now,
            best_pct: best.map(|b| b.1),
            best_year: best.map(|b| b.0),
        },
        new_time_pct: pct(new_ms, total),
        artists_known: heard.iter().filter(|f| f.local.date() <= end).count() as i64,
        known_since: heard.last().map(|f| day(f.local.date())),
        per_month,
        starts,
        log,
    };
    covered(cx, data, cov, false)
}

// ---------- How you listen

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MostSkipped {
    pub name: String,
    pub item_id: Option<i64>,
    pub skip_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuickBailRow {
    pub track: String,
    pub artist: String,
    pub item_id: Option<i64>,
    pub times: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct YearPct {
    pub year: i32,
    pub pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Shuffle {
    pub pct: Option<f64>,
    pub by_year: Vec<YearPct>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Offline {
    pub pct: f64,
    pub peak_month: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finished {
    pub pct: f64,
    pub how_far_pct: Option<f64>,
    pub how_far_coverage_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Longest {
    pub ms: i64,
    pub start_local: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sessions {
    pub count: usize,
    pub typical_ms: i64,
    pub longest: Option<Longest>,
    pub spread: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeepKind {
    Artist,
    Album,
    Repeat,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeepDive {
    pub kind: DeepKind,
    pub name: String,
    pub by: String,
    pub start_local: String,
    pub end_local: String,
    pub tracks: usize,
    pub ms: i64,
    pub times: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StartToFinish {
    pub item_id: i64,
    pub name: String,
    pub by: String,
    pub times: usize,
}

/// API.md §4.1 `Habits`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Habits {
    pub export_plays: i64,
    pub ends: Vec<Share<EndKind>>,
    pub skip_rate: f64,
    pub skips_by_hour: Vec<f64>,
    pub most_skipped: Option<MostSkipped>,
    pub quick_bails: Vec<QuickBailRow>,
    pub shuffle: Shuffle,
    pub devices: Vec<Share<DeviceClass>>,
    pub offline: Offline,
    pub finished: Finished,
    pub sessions: Sessions,
    pub deep_dives: Vec<DeepDive>,
    pub start_to_finish: Vec<StartToFinish>,
}

/// `YYYY-MM-DDTHH:MM:SS` without chrono's format parser (the session loader reads every play).
fn fast_time(s: &str) -> Option<NaiveDateTime> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<u32>().ok();
    NaiveDate::from_ymd_opt(n(0..4)? as i32, n(5..7)?, n(8..10)?)?.and_hms_opt(
        n(11..13)?,
        n(14..16)?,
        n(17..19)?,
    )
}

/// The bucket of a session length (minutes edges `SESSION_EDGES_MIN`, 10 buckets).
pub fn session_bucket(ms: i64) -> usize {
    SESSION_EDGES_MIN
        .iter()
        .take_while(|e| ms >= **e * 60_000)
        .count()
}

/// `get_listening_habits`.
pub fn habits<Tz: TimeZone>(cx: &Ctx<Tz>, q: &PeriodQuery) -> Result<Covered<Habits>> {
    let today = cx.today();
    let first = super::coverage::first_day(cx.conn)?;
    let r = resolve(q, today, first)?;
    let (_, cov) = period_coverage(cx, &r, 0)?;
    let span = r.cur;
    let cte = Src::All.cte(cx.include_private);
    let w = between(span);

    // One pass over the period's events (NFR-04: no SQL sort or index-ordered group per figure):
    // the per-event figures, offline peak, most skipped, quick bails, how far in, and the plays
    // sessions read (≥ 30 s, up to today, put in end order below).
    let durations: HashMap<String, i64> = cx
        .conn
        .prepare(
            // unfiltered: a track's length is known from any desktop row (0009's small index)
            "SELECT norm_key, MAX(duration_ms) FROM listening_event INDEXED BY listening_event_duration
             WHERE duration_ms IS NOT NULL AND norm_key <> '' GROUP BY 1",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut st = cx.conn.prepare(&format!(
        "{cte} SELECT id, ended_at_utc, local, ms_played, IFNULL(artist_name, ''), IFNULL(album_name, ''),
                IFNULL(norm_key, ''), item_id, source, IFNULL(track_name, ''), platform, reason_end,
                skipped, shuffle, offline, counts_as_play
         FROM ev WHERE {w}"
    ))?;
    let today_s = day(today);
    let (mut export_plays, mut export_events, mut finished) = (0i64, 0i64, 0i64);
    let mut ends: BTreeMap<EndKind, i64> = BTreeMap::new();
    let (mut flagged, mut skipped) = (0i64, 0i64);
    let mut hour_flags = [(0i64, 0i64); 24];
    let (mut shuffle_n, mut shuffle_on) = (0i64, 0i64);
    let mut this_pc = 0i64;
    let mut platforms: HashMap<Option<String>, i64> = HashMap::new();
    let mut offline = 0i64;
    let mut offline_months: BTreeMap<String, i64> = BTreeMap::new();
    // artist key (folded hash) → (max display name, ms, skipped, flagged)
    let mut artists: HashMap<u64, (String, i64, i64, i64)> = HashMap::new();
    let mut buf = Vec::new();
    // norm_key → (max track, max artist, max item id, times)
    let mut bails: HashMap<String, (String, String, Option<i64>, usize)> = HashMap::new();
    let (mut far_sum, mut far_n) = (0f64, 0i64);
    let mut plays = vec![];
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        let text =
            |i: usize| -> rusqlite::Result<&str> { Ok(super::text_opt(r, i)?.unwrap_or("")) };
        let ms: i64 = r.get(3)?;
        let play = r.get::<_, i64>(15)? != 0;
        let export = text(8)? == "export";
        let skip: Option<bool> = r.get(12)?;
        let local_s = text(2)?;
        let local = fast_time(local_s);
        if play {
            if export {
                let p = super::text_opt(r, 10)?.map(str::to_owned);
                *platforms.entry(p).or_default() += 1;
            } else {
                this_pc += 1;
            }
        }
        if let Some(s) = skip {
            flagged += 1;
            skipped += s as i64;
            if let Some(t) = local {
                let h = chrono::Timelike::hour(&t) as usize;
                hour_flags[h].0 += 1;
                hour_flags[h].1 += s as i64;
            }
        }
        if let Some(s) = r.get::<_, Option<bool>>(13)? {
            shuffle_n += 1;
            shuffle_on += s as i64;
        }
        let end = super::text_opt(r, 11)?;
        let key = text(6)?;
        let artist = text(4)?;
        if export {
            export_plays += play as i64;
            export_events += 1;
            *ends.entry(end_kind(end.unwrap_or(""))).or_default() += 1;
            if end == Some("trackdone") {
                finished += 1;
            }
            if play && r.get::<_, Option<bool>>(14)? == Some(true) {
                offline += 1;
                *offline_months
                    .entry(local_s.chars().take(7).collect())
                    .or_default() += 1;
            }
            if ms < QUICK_BAIL_MS && end == Some("fwdbtn") && !key.is_empty() {
                let (track, iid) = (text(9)?, r.get::<_, Option<i64>>(7)?);
                match bails.get_mut(key) {
                    Some(b) => {
                        if track > b.0.as_str() {
                            b.0 = track.to_owned();
                        }
                        if artist > b.1.as_str() {
                            b.1 = artist.to_owned();
                        }
                        b.2 = b.2.max(iid);
                        b.3 += 1;
                    }
                    None => {
                        bails.insert(
                            key.to_owned(),
                            (track.to_owned(), artist.to_owned(), iid, 1),
                        );
                    }
                }
            }
        }
        if !artist.is_empty() {
            let a = artists
                .entry(super::v3::folded_hash(&mut buf, &[artist]))
                .or_insert_with(|| (artist.to_owned(), 0, 0, 0));
            if artist > a.0.as_str() {
                a.0 = artist.to_owned();
            }
            a.1 += ms;
            if let Some(s) = skip {
                a.2 += s as i64;
                a.3 += 1;
            }
        }
        if play {
            if let Some(dur) = durations.get(key).filter(|d| **d > 0) {
                far_sum += (ms as f64 / *dur as f64).min(1.0);
                far_n += 1;
            }
            if local_s.get(..10).is_some_and(|d| d <= today_s.as_str()) {
                let end_t = fast_time(text(1)?).unwrap_or_default();
                plays.push(Ev {
                    id: r.get(0)?,
                    source: if export {
                        Source::Export
                    } else {
                        Source::Logger
                    },
                    end: end_t.and_utc(),
                    local: local.unwrap_or(end_t),
                    ms,
                    track: text(9)?.to_owned(),
                    artist: artist.to_owned(),
                    album: text(5)?.to_owned(),
                    norm_key: key.to_owned(),
                    platform: None,
                    reason_start: None,
                    reason_end: None,
                    shuffle: None,
                    skipped: None,
                    offline: None,
                    item_id: r.get(7)?,
                });
            }
        }
    }
    drop(rows);
    // end order for sessions, sorted here (NFR-04: SQL would sort every wide row)
    plays.sort_unstable_by_key(|e| (e.end, e.id));
    let mut devices: BTreeMap<DeviceClass, i64> = BTreeMap::new();
    if this_pc > 0 {
        devices.insert(DeviceClass::ThisPc, this_pc);
    }
    for (p, n) in platforms {
        *devices
            .entry(device(Source::Export, p.as_deref()))
            .or_default() += n;
    }
    let n_dev: i64 = devices.values().sum();
    let mut devices: Vec<Share<DeviceClass>> = devices
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(kind, n)| Share {
            kind,
            pct: pct(n, n_dev),
        })
        .collect();
    devices.sort_by(|a, b| b.pct.total_cmp(&a.pct));
    // the busiest month, the earliest on a tie
    let peak_month = offline_months
        .into_iter()
        .rev()
        .max_by_key(|(_, n)| *n)
        .map(|(m, _)| m);

    // Most skipped of the period's top 20 artists (by time), among events with a skip flag.
    let mut top: Vec<(String, i64, i64, i64)> = artists.into_values().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top.truncate(MOST_SKIPPED_TOP);
    let most_skipped = top
        .into_iter()
        .filter(|a| a.3 > 0)
        .map(|(name, _, s, n)| (name, pct(s, n)))
        .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(&a.0)));
    let most_skipped = match most_skipped {
        Some((name, skip_pct)) => Some(MostSkipped {
            item_id: artist_item(cx.conn, &name)?.0,
            name,
            skip_pct,
        }),
        None => None,
    };

    // Quick bails (terms: < 10 s, next button, ≥ 3 times, top 10).
    let mut bails: Vec<(String, String, Option<i64>, usize)> = bails
        .into_values()
        .filter(|b| b.3 >= QUICK_BAIL_MIN)
        .collect();
    bails.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| a.0.cmp(&b.0)));
    bails.truncate(QUICK_BAIL_TOP);
    let mut is_track = cx
        .conn
        .prepare("SELECT EXISTS(SELECT 1 FROM item WHERE id = ?1 AND type = 'track')")?;
    let mut quick_bails = vec![];
    for (track, artist, iid, times) in bails {
        let item_id = match iid {
            Some(id) if is_track.query_row([id], |r| r.get::<_, bool>(0))? => Some(id),
            _ => None,
        };
        quick_bails.push(QuickBailRow {
            track,
            artist,
            item_id,
            times: times as i64,
        });
    }

    // Shuffle by year: the whole history.
    let by_year = cx
        .conn
        .prepare(&format!(
            // unfiltered: base's filters by hand, on 0009's covering index (NFR-04)
            "SELECT CAST(substr(COALESCE(ended_at_local, ended_at_utc), 1, 4) AS INT), SUM(shuffle), COUNT(*)
             FROM listening_event INDEXED BY listening_event_shuffle
             WHERE shuffle IS NOT NULL AND reconciled_with IS NULL{} GROUP BY 1 ORDER BY 1",
            cx.private_filter("private_session")
        ))?
        .query_map([], |r| Ok(YearPct { year: r.get(0)?, pct: pct(r.get(1)?, r.get(2)?) }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let far_sum = Some(far_sum);

    // Sessions and deep dives (all plays), start to finish (library albums with a track list).
    let sessions = heuristics::sessions(&plays);
    let mut lengths: Vec<i64> = sessions.iter().map(|s| s.span_ms()).collect();
    lengths.sort_unstable();
    let typical_ms = terms::median(&lengths.iter().map(|x| *x as f64).collect::<Vec<_>>())
        .map_or(0, |m| m.round() as i64);
    let longest = sessions
        .iter()
        .max_by_key(|s| (s.span_ms(), std::cmp::Reverse(s.start)))
        .map(|s| Longest {
            ms: s.span_ms(),
            start_local: minute(&s.start.format("%Y-%m-%dT%H:%M:%S").to_string()),
        });
    let mut spread = vec![0i64; SESSION_EDGES_MIN.len() + 1];
    for l in &lengths {
        spread[session_bucket(*l)] += 1;
    }
    let fmt = |t: NaiveDateTime| minute(&t.format("%Y-%m-%dT%H:%M:%S").to_string());
    let mut dives: Vec<DeepDive> = sessions
        .iter()
        .flat_map(|s| {
            let n_plays = |d: &heuristics::Dive| {
                s.plays
                    .iter()
                    .filter(|e| {
                        e.local >= d.start
                            && e.local <= d.end
                            && match d.kind {
                                DiveKind::Album => e.album.eq_ignore_ascii_case(&d.name),
                                DiveKind::Artist => e.artist.eq_ignore_ascii_case(&d.name),
                            }
                    })
                    .count()
            };
            heuristics::deep_dives(s)
                .into_iter()
                .map(move |d| DeepDive {
                    kind: match d.kind {
                        DiveKind::Artist => DeepKind::Artist,
                        DiveKind::Album => DeepKind::Album,
                    },
                    times: (d.kind == DiveKind::Album)
                        .then(|| (n_plays(&d) / d.tracks.max(1)).max(1) as i64),
                    name: d.name.clone(),
                    by: d.by.clone(),
                    start_local: fmt(d.start),
                    end_local: fmt(d.end),
                    tracks: d.tracks,
                    ms: d.ms,
                })
        })
        .collect();
    let repeats = heuristics::on_repeat(&plays);
    let mut by_day_key: HashMap<(NaiveDate, &str), Vec<&Ev>> = HashMap::new();
    if !repeats.is_empty() {
        for e in &plays {
            by_day_key
                .entry((e.date(), e.norm_key.as_str()))
                .or_default()
                .push(e);
        }
    }
    for rep in repeats {
        let its = by_day_key
            .get(&(rep.date, rep.norm_key.as_str()))
            .cloned()
            .unwrap_or_default();
        dives.push(DeepDive {
            kind: DeepKind::Repeat,
            name: rep.track.clone(),
            by: rep.artist.clone(),
            start_local: fmt(its
                .first()
                .map_or(rep.date.and_hms_opt(0, 0, 0).expect("midnight"), |e| {
                    e.local - TimeDelta::milliseconds(e.ms)
                })),
            end_local: fmt(its
                .last()
                .map_or(rep.date.and_hms_opt(0, 0, 0).expect("midnight"), |e| {
                    e.local
                })),
            tracks: 1,
            ms: its.iter().map(|e| e.ms).sum(),
            times: Some(rep.times as i64),
        });
    }
    dives.sort_by(|a, b| {
        b.ms.cmp(&a.ms)
            .then_with(|| a.start_local.cmp(&b.start_local))
    });
    dives.truncate(DIVES_MAX);
    let start_to_finish = start_to_finish(cx.conn, &sessions)?;

    let data = Habits {
        export_plays,
        ends: ends
            .into_iter()
            .map(|(kind, n)| Share {
                kind,
                pct: pct(n, export_events),
            })
            .collect(),
        skip_rate: pct(skipped, flagged),
        skips_by_hour: hour_flags.iter().map(|(n, s)| pct(*s, *n)).collect(),
        most_skipped,
        quick_bails,
        shuffle: Shuffle {
            pct: (shuffle_n > 0).then(|| pct(shuffle_on, shuffle_n)),
            by_year,
        },
        devices,
        offline: Offline {
            pct: pct(offline, export_plays),
            peak_month,
        },
        finished: Finished {
            pct: pct(finished, export_events),
            how_far_pct: far_sum
                .filter(|_| far_n > 0)
                .map(|s| (s * 1000.0 / far_n as f64).round() / 10.0),
            how_far_coverage_pct: pct(far_n, n_dev),
        },
        sessions: Sessions {
            count: sessions.len(),
            typical_ms,
            longest,
            spread,
        },
        deep_dives: dives,
        start_to_finish,
    };
    covered(cx, data, cov, false)
}

/// Library albums with a track list, played start to finish in a session (≥ 1 time).
fn start_to_finish(
    conn: &Connection,
    sessions: &[heuristics::Session],
) -> Result<Vec<StartToFinish>> {
    if sessions.is_empty() {
        return Ok(vec![]);
    }
    let mut st = conn.prepare(
        "SELECT a.id, IFNULL(a.display_name, ''), IFNULL((SELECT ar.display_name FROM item_link l JOIN item ar ON ar.id = l.to_id
                  WHERE l.from_id = a.id AND l.kind = 'by_artist' LIMIT 1), ''), l.from_id
         FROM item a JOIN item_link l ON l.to_id = a.id AND l.kind = 'on_album'
         JOIN item t ON t.id = l.from_id AND t.type = 'track' WHERE a.type = 'album'",
    )?;
    let mut albums: BTreeMap<i64, (String, String, HashSet<i64>)> = BTreeMap::new();
    for r in st.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })? {
        let (id, name, by, track) = r?;
        albums
            .entry(id)
            .or_insert_with(|| (name, by, HashSet::new()))
            .2
            .insert(track);
    }
    // Only sessions holding one of the album's tracks can hold them all (NFR-04: thousands of albums
    // × thousands of sessions otherwise).
    let heard: Vec<HashSet<i64>> = sessions.iter().map(heuristics::Session::item_ids).collect();
    let mut with_track: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, ids) in heard.iter().enumerate() {
        for id in ids {
            with_track.entry(*id).or_default().push(i);
        }
    }
    let mut out: Vec<StartToFinish> = albums
        .into_iter()
        .filter_map(|(item_id, (name, by, tracks))| {
            let first = tracks
                .iter()
                .min_by_key(|t| with_track.get(t).map_or(0, Vec::len))?;
            let times = with_track
                .get(first)?
                .iter()
                .filter(|&&i| heuristics::played_all(&tracks, &heard[i]))
                .count();
            (times > 0).then_some(StartToFinish {
                item_id,
                name,
                by,
                times,
            })
        })
        .collect();
    out.sort_by(|a, b| b.times.cmp(&a.times).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_buckets_and_fast_times() {
        assert_eq!(session_bucket(0), 0);
        assert_eq!(session_bucket(5 * 60_000 - 1), 0);
        assert_eq!(session_bucket(5 * 60_000), 1);
        assert_eq!(session_bucket(45 * 60_000), 4);
        assert_eq!(session_bucket(240 * 60_000), 9);
        assert_eq!(session_bucket(999 * 60_000), 9);
        assert_eq!(
            fast_time("2026-10-19T21:14:05Z"),
            NaiveDate::from_ymd_opt(2026, 10, 19)
                .unwrap()
                .and_hms_opt(21, 14, 5)
        );
        assert_eq!(fast_time("2026-02-30T00:00:00"), None);
        assert_eq!(fast_time("2026-10-19"), None);
    }
}
