//! Whole-history tabs (API.md §4.1): `get_trends` (design §5 Trends) and `get_records` (Records).
//! All-time figures use all-device dates only (design §4.4); heavy lifting stays in SQL over the
//! 0008 rollups (`listening_day`, `listening_month_artist`), so only small results reach Rust.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::{Datelike, NaiveDate, NaiveDateTime, TimeDelta, TimeZone};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use super::coverage::{self, Model};
use super::heuristics::{
    self, ARC_BEFORE, ARC_MONTHS, ARC_RECENT, ArcKind, CONSTANT_TOP, ERA_SHARE, Era, back_again_at,
    is_constant, moved_most, window_share,
};
use super::listening::{Kind, RankRow, rank};
use super::terms::{
    self, CoverageClass, FORGOTTEN_TOP, Fingerprint, FpParts, LOYAL_TOP, fingerprint_of, streaks,
};
use super::v3::{
    Ctx, Src, artist_name, covered, daily, day, majority, minute, month, month_end, months,
    view_coverage,
};
use super::{Covered, base, hours, pct, rollup};
use crate::error::Result;

/// Arcs shown per kind.
pub const ARCS_PER_KIND: usize = 5;
/// Arc candidates: the top this many artists of the arc months (plus any recent comeback).
const ARC_CANDIDATES: usize = 50;
/// Tags month by month: this many tags.
const TAG_ROWS: usize = 6;

/// `get_trends` `measure`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    Hours,
    Plays,
    Artists,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MonthValue {
    pub month: String,
    pub value: f64,
    pub coverage: CoverageClass,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Arc {
    pub artist: String,
    pub item_id: Option<i64>,
    pub kind: ArcKind,
    /// % of each month's time; None = no data that month (G-48).
    pub shares: Vec<Option<f64>>,
    pub change_pct: Option<f64>,
    pub since: Option<String>,
    pub quiet_from: Option<String>,
    pub related: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct YearFingerprint {
    pub year: i32,
    pub fingerprint: Fingerprint,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagRow {
    pub tag: String,
    pub shares: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagMonths {
    pub months: Vec<String>,
    pub rows: Vec<TagRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DayMs {
    pub date: String,
    pub ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StreakOut {
    pub days: i64,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalendarYear {
    pub year: i32,
    pub minutes: Vec<Option<i64>>,
    pub biggest_day: Option<DayMs>,
    pub longest_streak: Option<StreakOut>,
    pub days_without_music: i64,
}

/// API.md §4.1 `Trends`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trends {
    pub months: Vec<MonthValue>,
    pub eras: Vec<Era>,
    pub arcs: Vec<Arc>,
    pub fingerprint_by_year: Vec<YearFingerprint>,
    pub tag_months: TagMonths,
    pub calendar: Vec<CalendarYear>,
}

pub fn history_model<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    first: NaiveDate,
    today: NaiveDate,
) -> Result<Model> {
    super::v3::view_model(cx, first, today)
}

/// `(month, artist key, ms)` rows of the rollup.
type MonthRows = BTreeMap<String, Vec<(String, i64)>>;

/// The whole 0008 month-and-artist rollup, read once for every figure built on it: the named
/// artists' rows (`v3::month_artists`' shape and order) and each month's total time, unnamed
/// events included. One ordered read of the primary key, summed here (NFR-04: each GROUP BY there
/// sorts the table).
struct MonthArtistsAll {
    rows: Vec<super::v3::MonthArtist>,
    totals: HashMap<String, i64>,
}

fn month_artists_all<Tz: TimeZone>(cx: &Ctx<Tz>) -> Result<MonthArtistsAll> {
    let mut st = cx.conn.prepare(&format!(
        "SELECT month, artist, ms_played, plays FROM listening_month_artist
         WHERE events > 0{} ORDER BY month, artist",
        cx.private_filter("private_session")
    ))?;
    let mut out = MonthArtistsAll {
        rows: vec![],
        totals: HashMap::new(),
    };
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        let (m, a) = (super::text(r, 0)?, super::text(r, 1)?);
        let (ms, plays): (i64, i64) = (r.get(2)?, r.get(3)?);
        match out.totals.get_mut(m) {
            Some(t) => *t += ms,
            None => {
                out.totals.insert(m.to_owned(), ms);
            }
        }
        if a.is_empty() {
            continue;
        }
        match out.rows.last_mut() {
            Some(l) if l.0 == m && l.1 == a => {
                l.2 += ms;
                l.3 += plays;
            }
            _ => out.rows.push((m.to_owned(), a.to_owned(), ms, plays)),
        }
    }
    Ok(out)
}

impl MonthArtistsAll {
    /// The rows of `keep`'s artists in the months `from..=to`, by month.
    fn by_month(&self, from: &str, to: &str, keep: &HashSet<&str>) -> MonthRows {
        let mut out: MonthRows = BTreeMap::new();
        for (m, a, ms, _) in &self.rows {
            if (from..=to).contains(&m.as_str()) && keep.contains(a.as_str()) {
                out.entry(m.clone()).or_default().push((a.clone(), *ms));
            }
        }
        out
    }

    /// `(year, artist, ms)` in (year, artist) order (the year as SQL's CAST of the month's first 4).
    fn by_year(&self) -> Vec<(i32, String, i64)> {
        let mut by: BTreeMap<(i32, &str), i64> = BTreeMap::new();
        for (m, a, ms, _) in &self.rows {
            let y = m.get(..4).and_then(|y| y.parse().ok()).unwrap_or(0);
            *by.entry((y, a.as_str())).or_default() += ms;
        }
        by.into_iter()
            .map(|((y, a), ms)| (y, a.to_owned(), ms))
            .collect()
    }
}

/// Each year's top `n` artist keys by time.
fn top_per_year(years: &[(i32, String, i64)], n: usize) -> BTreeMap<i32, Vec<String>> {
    terms::top_per_year(years, n)
}

/// `get_trends`.
pub fn trends<Tz: TimeZone>(cx: &Ctx<Tz>, measure: Measure) -> Result<Covered<Trends>> {
    let today = cx.today();
    let Some(first) = coverage::first_day(cx.conn)? else {
        let model = cx.model(today, today)?;
        let data = Trends {
            months: vec![],
            eras: vec![],
            arcs: vec![],
            fingerprint_by_year: vec![],
            tag_months: TagMonths {
                months: vec![],
                rows: vec![],
            },
            calendar: vec![],
        };
        return covered(cx, data, view_coverage(&model, (today, today), today), true);
    };
    let model = history_model(cx, first, today)?;
    let days = daily(cx, Src::All, (first, today))?;
    let all_months = months(first, today);

    let months_out = lifetime_months(cx, measure, &days, &model, first, today)?;
    let month_totals: Vec<(NaiveDate, i64)> = all_months
        .iter()
        .map(|m| (*m, days.range(*m..=month_end(*m)).map(|(_, v)| v.0).sum()))
        .collect();

    // Eras: only artists that ever hold ≥ 15% of a month can name one; the rest of each month is
    // one unnamed row, so shares and lifetime shares stay exact.
    let all = month_artists_all(cx)?;
    let total_of = |m: &str| all.totals.get(m).copied().unwrap_or(0) as f64;
    let named: HashSet<&str> = all
        .rows
        .iter()
        .filter(|(m, _, ms, _)| *ms as f64 >= ERA_SHARE * total_of(m))
        .map(|r| r.1.as_str())
        .collect();
    let candidates = all.by_month("", "~", &named);
    let era_months: Vec<(String, Vec<(String, i64)>)> = month_totals
        .iter()
        .map(|(m, total)| {
            let mut rows = candidates.get(&month(*m)).cloned().unwrap_or_default();
            let named: i64 = rows.iter().map(|r| r.1).sum();
            rows.push((String::new(), (total - named).max(0)));
            (month(*m), rows)
        })
        .collect();
    let mut names: HashMap<String, (String, Option<i64>)> = HashMap::new();
    let mut display = |key: &str| -> Result<(String, Option<i64>)> {
        if let Some(v) = names.get(key) {
            return Ok(v.clone());
        }
        let (n, id, _) = artist_name(cx.conn, key)?;
        names.insert(key.to_owned(), (n.clone(), id));
        Ok((n, id))
    };
    let mut eras = heuristics::eras(&era_months);
    for e in &mut eras {
        for a in &mut e.artists {
            *a = display(a)?.0;
        }
    }

    // Arcs over the last 36 months.
    let arc_months: Vec<NaiveDate> = months(
        super::v3::month_start(today) - chrono::Months::new(ARC_MONTHS as u32 - 1),
        today,
    );
    let arc_from = month(arc_months[0]);
    let arc_to = month(today);
    // FR-048 (G-46): once the windows reach past the export's end, an arc compares this PC with
    // itself (desktop plays on both sides); months the log didn't cover have no time: no data.
    let pc_arcs = model.export.is_some_and(|(_, last)| last < today);
    let pc_rows = if pc_arcs {
        let (rows, totals) = super::v3::desktop_month_artists(cx, (arc_months[0], today))?;
        Some(MonthArtistsAll { rows, totals })
    } else {
        None
    };
    let arc_all = pc_rows.as_ref().unwrap_or(&all);
    let arc_total = |m: &str| arc_all.totals.get(m).copied().unwrap_or(0) as f64;
    let totals_by_month: HashMap<String, i64> = match &pc_rows {
        Some(p) => p.totals.clone(),
        None => month_totals.iter().map(|(m, t)| (month(*m), *t)).collect(),
    };
    // Arc candidates: the top artists of the arc months, and any back again in the recent ones.
    let recent = month(arc_months[ARC_MONTHS - ARC_RECENT]);
    let mut arc_ms: BTreeMap<&str, i64> = BTreeMap::new();
    for (m, a, ms, _) in &arc_all.rows {
        if (arc_from.as_str()..=arc_to.as_str()).contains(&m.as_str()) {
            *arc_ms.entry(a.as_str()).or_default() += ms;
        }
    }
    let mut top: Vec<(&str, i64)> = arc_ms.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let mut keep: HashSet<&str> = top.into_iter().take(ARC_CANDIDATES).map(|x| x.0).collect();
    keep.extend(
        arc_all
            .rows
            .iter()
            .filter(|(m, _, ms, _)| {
                *m >= recent && *ms as f64 >= heuristics::BACK_AGAIN * arc_total(m)
            })
            .map(|r| r.1.as_str()),
    );
    let arc_rows = arc_all.by_month(&arc_from, &arc_to, &keep);
    let totals: Vec<i64> = arc_months
        .iter()
        .map(|m| totals_by_month.get(&month(*m)).copied().unwrap_or(0))
        .collect();
    let mut series: HashMap<String, Vec<(i64, i64)>> = HashMap::new();
    for (i, m) in arc_months.iter().enumerate() {
        for (a, ms) in arc_rows.get(&month(*m)).into_iter().flatten() {
            series
                .entry(a.clone())
                .or_insert_with(|| totals.iter().map(|t| (0, *t)).collect())[i]
                .0 = *ms;
        }
    }
    let years = all.by_year();
    let top20 = top_per_year(&years, CONSTANT_TOP);
    let years_n = top20.len();
    let mut in_top: HashMap<&str, usize> = HashMap::new();
    for a in top20.values().flatten() {
        *in_top.entry(a.as_str()).or_default() += 1;
    }
    let mut by_kind: BTreeMap<u8, Vec<(f64, Arc)>> = BTreeMap::new();
    let mut keys: Vec<&String> = series.keys().collect();
    keys.sort();
    for key in keys {
        let m = &series[key];
        let constant = is_constant(in_top.get(key.as_str()).copied().unwrap_or(0), years_n);
        let Some(kind) = heuristics::arc(m, constant) else {
            continue;
        };
        let n = m.len();
        let recent = window_share(&m[n - ARC_RECENT..]);
        let before = window_share(&m[n - ARC_RECENT - ARC_BEFORE..n - ARC_RECENT]);
        let change = recent
            .zip(before.filter(|b| *b > 0.0))
            .map(|(r, b)| ((r / b - 1.0) * 100.0).round());
        // arc() gave rising or fading only with data on both sides (no-data months, design §4.4)
        let recent = recent.unwrap_or(0.0);
        let at = |i: usize| Some(month(arc_months[i]));
        let (since, quiet_from, change_pct, related, order) = match kind {
            ArcKind::Rising => (
                at(n - ARC_RECENT),
                None,
                change,
                moved_most(key, &series, false),
                recent,
            ),
            ArcKind::Fading => (
                at(n - ARC_RECENT),
                None,
                change,
                moved_most(key, &series, true),
                -recent,
            ),
            ArcKind::BackAgain => {
                let (q, b) = back_again_at(m).expect("arc said back again");
                (at(b), at(q), None, vec![], recent)
            }
            ArcKind::Constant => (None, None, None, vec![], window_share(m).unwrap_or(0.0)),
        };
        let mut related_names = vec![];
        for r in related {
            related_names.push(display(&r)?.0);
        }
        let (artist, item_id) = display(key)?;
        by_kind.entry(kind as u8).or_default().push((
            order,
            Arc {
                artist,
                item_id,
                kind,
                shares: m.iter().map(|x| (x.1 > 0).then(|| pct(x.0, x.1))).collect(),
                change_pct,
                since,
                quiet_from,
                related: related_names,
            },
        ));
    }
    let arcs = by_kind
        .into_values()
        .flat_map(|mut v| {
            v.sort_by(|a, b| {
                b.0.total_cmp(&a.0)
                    .then_with(|| a.1.artist.cmp(&b.1.artist))
            });
            v.into_iter().take(ARCS_PER_KIND).map(|x| x.1)
        })
        .collect();

    let fingerprint_by_year = fingerprint_by_year(cx, &years, &days, &model, first, today)?;
    let tag_months = tag_months(cx, today)?;
    let calendar = (first.year()..=today.year())
        .map(|y| calendar_year(y, &days, &model, today))
        .collect();
    let data = Trends {
        months: months_out,
        eras,
        arcs,
        fingerprint_by_year,
        tag_months,
        calendar,
    };
    covered(cx, data, view_coverage(&model, (first, today), today), true)
}

/// Lifetime months (design §6: < 100 ms, from the 0008 rollups): every month of the history with
/// its hours, plays or artists (≥ 1 play) and the class most of its days have.
pub fn lifetime_months<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    measure: Measure,
    days: &BTreeMap<NaiveDate, (i64, i64)>,
    model: &Model,
    first: NaiveDate,
    today: NaiveDate,
) -> Result<Vec<MonthValue>> {
    let mut artists_per_month: HashMap<String, i64> = HashMap::new();
    if measure == Measure::Artists {
        let mut st = cx.conn.prepare(&format!(
            "SELECT month, COUNT(DISTINCT artist) FROM listening_month_artist
             WHERE events > 0 AND plays > 0 AND artist <> ''{} GROUP BY 1",
            cx.private_filter("private_session")
        ))?;
        for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (m, n) = r?;
            artists_per_month.insert(m, n);
        }
    }
    Ok(months(first, today)
        .into_iter()
        .map(|m| {
            let (ms, plays) = days
                .range(m..=month_end(m))
                .fold((0, 0), |a, (_, v)| (a.0 + v.0, a.1 + v.1));
            let (a, b) = (m.max(first), month_end(m).min(today));
            MonthValue {
                month: month(m),
                value: match measure {
                    Measure::Hours => hours(ms),
                    Measure::Plays => plays as f64,
                    Measure::Artists => {
                        artists_per_month.get(&month(m)).copied().unwrap_or(0) as f64
                    }
                },
                coverage: majority(a.iter_days().take_while(|d| *d <= b), |d| model.class(d)),
            }
        })
        .collect())
}

/// Only all-device dates count in all-time figures (design §4.4); the rest is skipped.
fn all_devices(model: &Model) -> impl Fn(NaiveDate) -> CoverageClass + '_ {
    |d| match model.class(d) {
        CoverageClass::AllDevices => CoverageClass::AllDevices,
        _ => CoverageClass::NoData,
    }
}

fn streak_out(s: Option<terms::Streak>) -> Option<StreakOut> {
    s.map(|s| StreakOut {
        days: s.days,
        from: day(s.from),
        to: day(s.to),
    })
}

/// One calendar year: minutes per all-device date (others null), biggest day, longest streak and
/// days without music (all-device dates up to today without a play).
fn calendar_year(
    y: i32,
    days: &BTreeMap<NaiveDate, (i64, i64)>,
    model: &Model,
    today: NaiveDate,
) -> CalendarYear {
    let (a, b) = (
        NaiveDate::from_ymd_opt(y, 1, 1).expect("1 Jan"),
        NaiveDate::from_ymd_opt(y, 12, 31).expect("31 Dec"),
    );
    let class = all_devices(model);
    let covered = |d: NaiveDate| d <= today && class(d) == CoverageClass::AllDevices;
    let played: BTreeSet<NaiveDate> = days
        .range(a..=b)
        .filter(|(_, v)| v.1 > 0)
        .map(|(d, _)| *d)
        .collect();
    let biggest = days
        .range(a..=b)
        .filter(|(d, v)| covered(**d) && v.0 > 0)
        .max_by(|x, y| x.1.0.cmp(&y.1.0).then_with(|| y.0.cmp(x.0)))
        .map(|(d, v)| DayMs {
            date: day(*d),
            ms: v.0,
        });
    CalendarYear {
        year: y,
        minutes: a
            .iter_days()
            .take_while(|d| *d <= b)
            .map(|d| covered(d).then(|| (days.get(&d).map_or(0, |v| v.0) + 30_000) / 60_000))
            .collect(),
        biggest_day: biggest,
        longest_streak: streak_out(
            streaks(a, b.min(today), &played, |d| {
                if d <= today {
                    class(d)
                } else {
                    CoverageClass::NoData
                }
            })
            .0,
        ),
        days_without_music: a
            .iter_days()
            .take_while(|d| *d <= b)
            .filter(|d| covered(*d) && !played.contains(d))
            .count() as i64,
    }
}

/// The fingerprint of each year with listening, from the rollups plus one grouped pass for Repeat.
fn fingerprint_by_year<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    years: &[(i32, String, i64)],
    days: &BTreeMap<NaiveDate, (i64, i64)>,
    model: &Model,
    first: NaiveDate,
    today: NaiveDate,
) -> Result<Vec<YearFingerprint>> {
    let mut artists: HashMap<i32, Vec<(String, i64)>> = HashMap::new();
    for (y, a, ms) in years {
        artists.entry(*y).or_default().push((a.clone(), *ms));
    }
    let first_year: HashMap<String, i32> = terms::first_heard(cx.conn, cx.include_private)?
        .into_iter()
        .map(|f| (f.artist, f.local.year()))
        .collect();
    // Repeat: plays of tracks played in an earlier year (the year is the period). One unsorted pass,
    // split over threads (NFR-04: grouping every play by track and year sorts the whole table): a
    // year's repeat plays are its plays minus those of tracks first played that year.
    type Firsts = (
        HashMap<i32, i64>,
        // quick hash of the track (retried on a collision) → (track, first year with a play, its
        // plays that year)
        super::v3::IdMap<u64, (String, i32, i64)>,
    );
    fn note(f: &mut Firsts, k: &str, y: i32, n: i64) {
        let mut h = super::v3::quick_hash(&[Some(k)]);
        loop {
            match f.1.get_mut(&h) {
                Some(e) if e.0 == k => {
                    if y < e.1 {
                        (e.1, e.2) = (y, n);
                    } else if y == e.1 {
                        e.2 += n;
                    }
                    return;
                }
                Some(_) => h = h.wrapping_add(0x9e37_79b9_7f4a_7c15),
                None => {
                    f.1.insert(h, (k.to_owned(), y, n));
                    return;
                }
            }
        }
    }
    let base_sql = base(cx.include_private);
    let sql = |w: &str| {
        format!(
            "{base_sql} SELECT norm_key, CAST(substr(local, 1, 4) AS INT) FROM base
             WHERE counts_as_play AND norm_key <> '' AND +ended_at_utc >= '' AND {w}"
        )
    };
    let row = |f: &mut Firsts, r: &rusqlite::Row| -> Result<()> {
        let (k, y): (&str, i32) = (super::text(r, 0)?, r.get(1)?);
        *f.0.entry(y).or_default() += 1;
        note(f, k, y, 1);
        Ok(())
    };
    let (plays_by_year, first_year_of) =
        super::v3::par_rows(cx, None, &sql, &Firsts::default, &row)?
            .into_iter()
            .reduce(|mut a, b| {
                for (y, n) in b.0 {
                    *a.0.entry(y).or_default() += n;
                }
                for (k, y, n) in b.1.into_values() {
                    note(&mut a, &k, y, n);
                }
                a
            })
            .unwrap_or_default();
    let mut repeat: HashMap<i32, i64> = plays_by_year;
    for (_, y, n) in first_year_of.into_values() {
        *repeat.entry(y).or_default() -= n;
    }
    let class = all_devices(model);
    let mut out = vec![];
    for y in first.year()..=today.year() {
        let (a, b) = (
            NaiveDate::from_ymd_opt(y, 1, 1).expect("1 Jan"),
            NaiveDate::from_ymd_opt(y, 12, 31)
                .expect("31 Dec")
                .min(today),
        );
        let (ms, plays) = days
            .range(a..=b)
            .fold((0, 0), |s, (_, v)| (s.0 + v.0, s.1 + v.1));
        if ms == 0 {
            continue;
        }
        let rows = artists.remove(&y).unwrap_or_default();
        let covered: Vec<NaiveDate> = a
            .iter_days()
            .take_while(|d| *d <= b)
            .filter(|d| class(*d) != CoverageClass::NoData)
            .collect();
        out.push(YearFingerprint {
            year: y,
            fingerprint: fingerprint_of(&FpParts {
                ms,
                new_ms: rows
                    .iter()
                    .filter(|(k, _)| first_year.get(k) == Some(&y))
                    .map(|r| r.1)
                    .sum(),
                artist_ms: rows.iter().map(|r| r.1).collect(),
                days_listened: covered
                    .iter()
                    .filter(|d| days.get(d).is_some_and(|v| v.1 > 0))
                    .count() as i64,
                covered_days: covered.len() as i64,
                plays,
                repeat_plays: repeat.get(&y).copied().unwrap_or(0),
            }),
        });
    }
    Ok(out)
}

/// Tags month by month: the top tags of the last 12 months by tagged time, each month's share (%)
/// of that month's tagged listening (U, via `insights::SRC` like `terms::top_tags`).
fn tag_months<Tz: TimeZone>(cx: &Ctx<Tz>, today: NaiveDate) -> Result<TagMonths> {
    let ms12 = months(
        super::v3::month_start(today) - chrono::Months::new(11),
        today,
    );
    let span = (ms12[0], today);
    // per record and month first (summed here: a GROUP BY sorts every event), then its tags
    // (NFR-04, as `terms::top_tags`)
    let mut st = cx.conn.prepare(&format!(
        "{} SELECT item_id, substr(local, 1, 7), ms_played FROM base
         WHERE {} AND item_id IS NOT NULL",
        base(cx.include_private),
        super::v3::between(span)
    ))?;
    // keyed by the month's index in `ms12` (NFR-04: no text copy per event)
    let keys: Vec<String> = ms12.iter().map(|m| month(*m)).collect();
    let ix: HashMap<&str, usize> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| (k.as_str(), i))
        .collect();
    let mut per_ix: HashMap<(i64, usize), i64> = HashMap::new();
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        let (id, m, ms): (i64, &str, i64) = (r.get(0)?, super::text(r, 1)?, r.get(2)?);
        if let Some(&i) = ix.get(m) {
            *per_ix.entry((id, i)).or_default() += ms;
        }
    }
    drop(rows);
    let per_item = per_ix
        .into_iter()
        .map(|((id, i), ms)| ((id, keys[i].clone()), ms));
    let tags = terms::item_tags(cx.conn)?;
    let mut per_month: HashMap<String, i64> = HashMap::new();
    let mut by_tag: BTreeMap<String, HashMap<String, i64>> = BTreeMap::new();
    for ((id, m), ms) in per_item {
        let Some(ts) = tags.get(&id) else {
            continue;
        };
        *per_month.entry(m.clone()).or_default() += ms;
        for t in ts {
            *by_tag
                .entry(t.clone())
                .or_default()
                .entry(m.clone())
                .or_default() += ms;
        }
    }
    let mut ranked: Vec<(i64, String)> = by_tag
        .iter()
        .map(|(t, m)| (m.values().sum(), t.clone()))
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let rows = ranked
        .into_iter()
        .take(TAG_ROWS)
        .map(|(_, tag)| TagRow {
            shares: keys
                .iter()
                .map(|m| {
                    pct(
                        by_tag[&tag].get(m).copied().unwrap_or(0),
                        per_month.get(m).copied().unwrap_or(0),
                    )
                })
                .collect(),
            tag,
        })
        .collect();
    Ok(TagMonths { months: keys, rows })
}

// ---------- Records

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FirstPlay {
    pub track: String,
    pub artist: String,
    pub at_local: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct YearMs {
    pub year: i32,
    pub ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MonthArtists {
    pub month: String,
    pub artists: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Loyal {
    pub name: String,
    pub item_id: Option<i64>,
    pub years_in_top20: usize,
    pub years: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Milestone {
    pub plays: i64,
    pub eta_days: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TopKind {
    Artist,
    Album,
    Track,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayTop {
    pub kind: TopKind,
    pub name: String,
    pub by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OnThisDay {
    pub year: i32,
    pub plays: i64,
    pub ms: i64,
    pub top: Option<DayTop>,
    pub first_artists: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Forgotten {
    pub name: String,
    pub item_id: Option<i64>,
    pub top10_year: i32,
    pub last_played_local: String,
    pub play_uri: Option<String>,
}

/// API.md §4.1 `Records`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Records {
    pub since: Option<String>,
    pub plays_total: i64,
    pub first_play: Option<FirstPlay>,
    pub top_track: Option<RankRow>,
    pub top_artist: Option<RankRow>,
    pub top_album: Option<RankRow>,
    pub longest_streak: Option<StreakOut>,
    pub biggest_day: Option<DayMs>,
    pub biggest_year: Option<YearMs>,
    pub most_artists_month: Option<MonthArtists>,
    pub most_loyal: Option<Loyal>,
    pub next_milestone: Milestone,
    pub on_this_day: Vec<OnThisDay>,
    pub forgotten: Vec<Forgotten>,
}

/// On this day: the day's top is an album when one holds at least this share (%) of its time.
pub const DAY_ALBUM_PCT: f64 = 50.0;

/// `get_records`.
pub fn records<Tz: TimeZone>(cx: &Ctx<Tz>) -> Result<Covered<Records>> {
    let today = cx.today();
    let first = coverage::first_day(cx.conn)?;
    let model = match first {
        Some(f) => history_model(cx, f, today)?,
        None => cx.model(today, today)?,
    };
    let start = first.unwrap_or(today);
    let days = daily(cx, Src::All, (start, today))?;
    let class = all_devices(&model);
    let all_dev = |d: &NaiveDate| class(*d) == CoverageClass::AllDevices;
    let plays_total: i64 = days.values().map(|v| v.1).sum();
    let b = base(cx.include_private);
    let first_play = cx
        .conn
        .query_row(
            &format!(
                "{b} SELECT IFNULL(track_name, ''), IFNULL(artist_name, ''), local FROM base
                 WHERE counts_as_play ORDER BY ended_at_utc, id LIMIT 1"
            ),
            [],
            |r| {
                Ok(FirstPlay {
                    track: r.get(0)?,
                    artist: r.get(1)?,
                    at_local: minute(&r.get::<_, String>(2)?),
                })
            },
        )
        .optional()?;
    let top = |k| -> Result<Option<RankRow>> {
        Ok(rank(cx.conn, &rollup(cx.include_private), k, "?1 = ''", &"", 1)?.pop())
    };
    let played: BTreeSet<NaiveDate> = days
        .iter()
        .filter(|(_, v)| v.1 > 0)
        .map(|(d, _)| *d)
        .collect();
    let longest = streak_out(streaks(start, today, &played, &class).0);
    let biggest_day = days
        .iter()
        .filter(|(d, v)| all_dev(d) && v.0 > 0)
        .max_by(|x, y| x.1.0.cmp(&y.1.0).then_with(|| y.0.cmp(x.0)))
        .map(|(d, v)| DayMs {
            date: day(*d),
            ms: v.0,
        });
    let mut years: BTreeMap<i32, i64> = BTreeMap::new();
    for (d, v) in days.iter().filter(|(d, _)| all_dev(d)) {
        *years.entry(d.year()).or_default() += v.0;
    }
    let biggest_year = terms::biggest(&years).map(|(year, ms)| YearMs { year, ms });
    let most_artists_month = match model.export {
        Some((a, b)) => cx
            .conn
            .query_row(
                &format!(
                    "SELECT month, COUNT(DISTINCT artist) AS n FROM listening_month_artist
                     WHERE events > 0 AND plays > 0 AND artist <> '' AND month BETWEEN '{}' AND '{}'{}
                     GROUP BY month ORDER BY n DESC, month LIMIT 1",
                    month(a),
                    month(b),
                    cx.private_filter("private_session")
                ),
                [],
                |r| Ok(MonthArtists { month: r.get(0)?, artists: r.get(1)? }),
            )
            .optional()?,
        None => None,
    };
    let years = month_artists_all(cx)?.by_year();
    let loyal = terms::most_loyal(&top_per_year(&years, LOYAL_TOP));
    let most_loyal = match loyal {
        Some((key, years_in_top20, years)) => {
            let (name, item_id, _) = artist_name(cx.conn, &key)?;
            Some(Loyal {
                name,
                item_id,
                years_in_top20,
                years,
            })
        }
        None => None,
    };
    let last90: i64 = days
        .range(today - TimeDelta::days(terms::MILESTONE_PACE_DAYS - 1)..=today)
        .map(|(_, v)| v.1)
        .sum();
    let (next, eta) = terms::next_milestone(plays_total, last90);

    // On this day: today's date in earlier years, all-device dates only, newest first.
    let heard = terms::first_heard(cx.conn, cx.include_private)?;
    let mut on_this_day = vec![];
    for y in (start.year()..today.year()).rev() {
        let Some(d) = NaiveDate::from_ymd_opt(y, today.month(), today.day()) else {
            continue;
        };
        if !all_dev(&d) {
            continue;
        }
        let (ms, plays) = days.get(&d).copied().unwrap_or((0, 0));
        on_this_day.push(OnThisDay {
            year: y,
            plays,
            ms,
            top: if ms > 0 { day_top(cx, d, ms)? } else { None },
            first_artists: heard
                .iter()
                .filter(|f| f.local.date() == d)
                .map(|f| f.name.clone())
                .collect(),
        });
    }

    // Forgotten favourites: in some year's top 10, no play for ≥ 730 days.
    let top10 = top_per_year(&years, FORGOTTEN_TOP);
    let mut last_play: HashMap<String, NaiveDateTime> = HashMap::new();
    let mut st = cx.conn.prepare(&format!(
        // 0004's index answers each artist's newest event
        "{b} SELECT local FROM base WHERE lower(artist_name) = ?1 AND counts_as_play ORDER BY ended_at_utc DESC LIMIT 1"
    ))?;
    for a in top10.values().flatten() {
        if last_play.contains_key(a) {
            continue;
        }
        let local: Option<String> = st.query_row([a], |r| r.get(0)).optional()?;
        if let Some(t) = local
            .and_then(|s| NaiveDateTime::parse_from_str(s.get(..19)?, "%Y-%m-%dT%H:%M:%S").ok())
        {
            last_play.insert(a.clone(), t);
        }
    }
    let now_local = cx.now.with_timezone(cx.tz).naive_local();
    let mut forgotten = vec![];
    let mut uri = cx.conn.prepare(&format!(
        "{b} SELECT spotify_track_uri FROM base WHERE lower(artist_name) = ?1 AND spotify_track_uri LIKE 'spotify:track:%'
         GROUP BY spotify_track_uri ORDER BY SUM(counts_as_play) DESC, spotify_track_uri LIMIT 1"
    ))?;
    for (key, year) in terms::forgotten(&top10, &last_play, now_local) {
        let (name, item_id, _) = artist_name(cx.conn, &key)?;
        forgotten.push(Forgotten {
            name,
            item_id,
            top10_year: year,
            last_played_local: minute(&last_play[&key].format("%Y-%m-%dT%H:%M:%S").to_string()),
            play_uri: uri.query_row([&key], |r| r.get(0)).optional()?,
        });
    }
    let data = Records {
        since: first.map(day),
        plays_total,
        first_play,
        top_track: top(Kind::Track)?,
        top_artist: top(Kind::Artist)?,
        top_album: top(Kind::Album)?,
        longest_streak: longest,
        biggest_day,
        biggest_year,
        most_artists_month,
        most_loyal,
        next_milestone: Milestone {
            plays: next,
            eta_days: eta,
        },
        on_this_day,
        forgotten,
    };
    covered(
        cx,
        data,
        view_coverage(&model, (start, today), today),
        false,
    )
}

/// The day's top: its album when one holds ≥ 50% of the day's time, else its top artist.
fn day_top<Tz: TimeZone>(cx: &Ctx<Tz>, d: NaiveDate, day_ms: i64) -> Result<Option<DayTop>> {
    let span = super::v3::between((d, d));
    let b = base(cx.include_private);
    let album: Option<(String, String, i64)> = cx
        .conn
        .query_row(
            &format!(
                "{b} SELECT MAX(album_name), IFNULL(MAX(artist_name), ''), SUM(ms_played) AS ms FROM base
                 WHERE {span} AND album_name <> '' GROUP BY lower(album_name) ORDER BY ms DESC LIMIT 1"
            ),
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((name, by, ms)) = album
        && pct(ms, day_ms) >= DAY_ALBUM_PCT
    {
        return Ok(Some(DayTop {
            kind: TopKind::Album,
            name,
            by,
        }));
    }
    Ok(cx
        .conn
        .query_row(
            &format!(
                "{b} SELECT MAX(artist_name), SUM(ms_played) AS ms FROM base
                 WHERE {span} AND artist_name <> '' GROUP BY lower(artist_name) ORDER BY ms DESC LIMIT 1"
            ),
            [],
            |r| Ok(DayTop { kind: TopKind::Artist, name: r.get(0)?, by: String::new() }),
        )
        .optional()?)
}
