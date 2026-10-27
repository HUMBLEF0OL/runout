//! `explore` (design §5 Explore: up to 3 artists / albums / tracks over time, < 200 ms, NFR-04)
//! and `get_data_coverage` (Your data: coverage per source, import history). Names and dates come
//! from the UI: validated again here (TSD §7) and bound as parameters, never put into SQL text.
use std::collections::{BTreeSet, HashMap};

use chrono::{Datelike, NaiveDate, TimeDelta, TimeZone};
use rusqlite::{ToSql, params_from_iter};
use serde::{Deserialize, Serialize};

use super::coverage::{self, export_batches};
use super::terms::{DeviceClass, device_sql};
use super::v3::{
    Ctx, Src, between, covered, daily, day, minute, strict_day, view_coverage, view_model,
};
use super::{Covered, hours, pct};
use crate::error::{AppError, Result};
use crate::text::{clean_line, limit};

/// Explore: at most this many series.
pub const SERIES_MAX: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeriesKind {
    Artist,
    Album,
    Track,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SeriesQuery {
    pub kind: SeriesKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExploreGrain {
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExploreMeasure {
    Hours,
    Share,
    Plays,
}

/// API.md §4.1 `ExploreQuery`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExploreQuery {
    pub series: Vec<SeriesQuery>,
    pub from: String,
    pub to: String,
    pub grain: ExploreGrain,
    pub measure: ExploreMeasure,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<Vec<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekdays: Option<Vec<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devices: Option<Vec<DeviceClass>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shuffle: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Peak {
    pub bucket: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeriesOut {
    pub kind: SeriesKind,
    pub name: String,
    pub by: String,
    pub values: Vec<f64>,
    pub total: f64,
    pub peak: Option<Peak>,
    pub first_played_local: Option<String>,
}

/// API.md §4.1 `ExploreResult`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExploreResult {
    pub buckets: Vec<String>,
    pub series: Vec<SeriesOut>,
    pub export_only: bool,
}

fn bucket_of(g: ExploreGrain, d: NaiveDate) -> String {
    match g {
        ExploreGrain::Week => day(d - TimeDelta::days(d.weekday().num_days_from_monday().into())),
        ExploreGrain::Month => d.format("%Y-%m").to_string(),
        ExploreGrain::Year => d.format("%Y").to_string(),
    }
}

/// The bucket of an event's local time, as SQL (fixed text).
fn bucket_sql(g: ExploreGrain) -> &'static str {
    match g {
        ExploreGrain::Week => {
            "date(substr(local, 1, 10), '-' || ((CAST(strftime('%w', local) AS INT) + 6) % 7) || ' days')"
        }
        ExploreGrain::Month => "substr(local, 1, 7)",
        ExploreGrain::Year => "substr(local, 1, 4)",
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// The builder, validated again (TSD §7) and cleaned: names trimmed, an empty `by` dropped. Shared
/// by `explore` and `save_explore_view`.
pub fn clean_query(q: &ExploreQuery) -> Result<ExploreQuery> {
    if q.series.is_empty() || q.series.len() > SERIES_MAX {
        return Err(AppError::validation(
            "Choose one to three artists, albums or tracks.",
        ));
    }
    let series = q
        .series
        .iter()
        .map(|s| {
            let name = clean_line(&s.name, limit::NAME);
            let by =
                s.by.as_deref()
                    .map(|b| clean_line(b, limit::NAME))
                    .filter(|b| !b.is_empty());
            (!name.is_empty())
                .then_some(SeriesQuery {
                    kind: s.kind,
                    name,
                    by,
                })
                .ok_or_else(|| AppError::validation("Each line needs a name."))
        })
        .collect::<Result<_>>()?;
    match (strict_day(&q.from), strict_day(&q.to)) {
        (Some(from), Some(to)) if from <= to => {}
        _ => return Err(AppError::validation("Choose a start and an end date.")),
    }
    if q.hours.iter().flatten().any(|h| !(0..=23).contains(h))
        || q.weekdays.iter().flatten().any(|w| !(1..=7).contains(w))
    {
        return Err(AppError::validation(
            "That hour or weekday is out of range.",
        ));
    }
    Ok(ExploreQuery {
        series,
        ..q.clone()
    })
}

/// `explore`.
pub fn explore<Tz: TimeZone>(cx: &Ctx<Tz>, q: &ExploreQuery) -> Result<Covered<ExploreResult>> {
    let q = &clean_query(q)?;
    let series: Vec<(SeriesKind, String, Option<String>)> = q
        .series
        .iter()
        .map(|s| (s.kind, s.name.clone(), s.by.clone()))
        .collect();
    let (Some(from), Some(to)) = (strict_day(&q.from), strict_day(&q.to)) else {
        unreachable!("clean_query checked both dates")
    };
    let hours_f = q.hours.clone().unwrap_or_default();
    let weekdays_f = q.weekdays.clone().unwrap_or_default();
    let devices = q.devices.clone().unwrap_or_default();
    let export_only = !devices.is_empty() || q.shuffle.is_some();

    let today = cx.today();
    let model = view_model(cx, from, to.min(today).max(from))?;
    let mut end = to.min(today);
    if export_only {
        end = end.min(model.export.map_or(from - TimeDelta::days(1), |e| e.1));
    }
    let buckets: Vec<String> = from
        .iter_days()
        .take_while(|d| *d <= end)
        .map(|d| bucket_of(q.grain, d))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let cov = view_coverage(&model, (from, to), today);

    // Filters: fixed SQL from validated numbers and enums only.
    let mut filters = String::new();
    if !hours_f.is_empty() {
        let l: Vec<String> = hours_f.iter().map(i64::to_string).collect();
        filters += &format!(
            " AND CAST(strftime('%H', local) AS INT) IN ({})",
            l.join(", ")
        );
    }
    if !weekdays_f.is_empty() {
        let l: Vec<String> = weekdays_f.iter().map(i64::to_string).collect();
        filters += &format!(
            " AND (CAST(strftime('%w', local) AS INT) + 6) % 7 + 1 IN ({})",
            l.join(", ")
        );
    }
    if export_only {
        filters += " AND source = 'export'";
    }
    if !devices.is_empty() {
        let l: Vec<&str> = devices.iter().map(|d| device_sql(*d)).collect();
        filters += &format!(" AND ({})", l.join(" OR "));
    }
    if let Some(s) = q.shuffle {
        filters += &format!(" AND shuffle = {}", s as i64);
    }

    // One pass: per bucket, everything's time (for share) and each series' time, plays, first play.
    let mut params: Vec<&dyn ToSql> = vec![];
    let mut cols = String::new();
    let mut matches = vec![];
    for (i, (kind, name, by)) in series.iter().enumerate() {
        params.push(name);
        let n = params.len();
        let col = match kind {
            SeriesKind::Artist => "artist_name",
            SeriesKind::Album => "album_name",
            SeriesKind::Track => "track_name",
        };
        let mut m = format!("lower({col}) = lower(?{n})");
        if let (Some(b), false) = (by, *kind == SeriesKind::Artist) {
            params.push(b);
            m += &format!(" AND lower(artist_name) = lower(?{})", params.len());
        }
        cols += &format!(
            ", SUM(CASE WHEN {m} THEN ms_played END), SUM(CASE WHEN {m} THEN counts_as_play END),
               MIN(CASE WHEN {m} AND counts_as_play THEN local END), MAX(CASE WHEN {m} THEN artist_name END) AS by{i}"
        );
        matches.push(format!("({m})"));
    }
    let only = format!(" AND ({})", matches.join(" OR "));
    let mut out: Vec<SeriesOut> = series
        .iter()
        .map(|(kind, name, by)| SeriesOut {
            kind: *kind,
            name: name.clone(),
            by: by.clone().unwrap_or_default(),
            values: vec![0.0; buckets.len()],
            total: 0.0,
            peak: None,
            first_played_local: None,
        })
        .collect();
    if end >= from {
        let sql = format!(
            "{} SELECT {} AS b, SUM(ms_played){cols} FROM ev WHERE {}{filters}{only} GROUP BY b",
            Src::All.cte(cx.include_private),
            bucket_sql(q.grain),
            between((from, end))
        );
        let idx: HashMap<&str, usize> = buckets
            .iter()
            .enumerate()
            .map(|(i, b)| (b.as_str(), i))
            .collect();
        // Share: each bucket's time, from `listening_day` unless a filter applies (NFR-04).
        let mut totals = vec![0i64; buckets.len()];
        if q.measure == ExploreMeasure::Share {
            if filters.is_empty() {
                for (d, v) in daily(cx, Src::All, (from, end))? {
                    if let Some(&i) = idx.get(bucket_of(q.grain, d).as_str()) {
                        totals[i] += v.0;
                    }
                }
            } else {
                let mut st = cx.conn.prepare(&format!(
                    "{} SELECT {} AS b, SUM(ms_played) FROM ev WHERE {}{filters} GROUP BY b",
                    Src::All.cte(cx.include_private),
                    bucket_sql(q.grain),
                    between((from, end))
                ))?;
                let mut rows = st.query([])?;
                while let Some(r) = rows.next()? {
                    if let Some(&i) = r
                        .get::<_, Option<String>>(0)?
                        .as_deref()
                        .and_then(|b| idx.get(b))
                    {
                        totals[i] = r.get::<_, Option<i64>>(1)?.unwrap_or(0);
                    }
                }
            }
        }
        let mut st = cx.conn.prepare(&sql)?;
        let mut rows = st.query(params_from_iter(params))?;
        while let Some(r) = rows.next()? {
            let Some(&bi) = r
                .get::<_, Option<String>>(0)?
                .as_deref()
                .and_then(|b| idx.get(b))
            else {
                continue;
            };
            let total = totals[bi];
            for (i, s) in out.iter_mut().enumerate() {
                let c = 2 + i * 4;
                let ms: i64 = r.get::<_, Option<i64>>(c)?.unwrap_or(0);
                let plays: i64 = r.get::<_, Option<i64>>(c + 1)?.unwrap_or(0);
                s.values[bi] = match q.measure {
                    ExploreMeasure::Hours => hours(ms),
                    ExploreMeasure::Plays => plays as f64,
                    ExploreMeasure::Share => pct(ms, total),
                };
                if let Some(first) = r.get::<_, Option<String>>(c + 2)?
                    && s.first_played_local.as_ref().is_none_or(|f| first < *f)
                {
                    s.first_played_local = Some(first);
                }
                if s.by.is_empty()
                    && series[i].0 != SeriesKind::Artist
                    && let Some(b) = r.get::<_, Option<String>>(c + 3)?
                {
                    s.by = b;
                }
            }
        }
    }
    for s in &mut out {
        s.first_played_local = s.first_played_local.as_deref().map(minute);
        let sum: f64 = s.values.iter().sum();
        s.total = match q.measure {
            ExploreMeasure::Share if !s.values.is_empty() => round1(sum / s.values.len() as f64),
            _ => round1(sum),
        };
        s.peak = s
            .values
            .iter()
            .enumerate()
            .filter(|(_, v)| **v > 0.0)
            .max_by(|a, b| a.1.total_cmp(b.1).then_with(|| b.0.cmp(&a.0)))
            .map(|(i, v)| Peak {
                bucket: buckets[i].clone(),
                value: *v,
            });
    }
    covered(
        cx,
        ExploreResult {
            buckets,
            series: out,
            export_only,
        },
        cov,
        false,
    )
}

// ---------- Your data

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportRange {
    pub from: String,
    pub to: String,
    pub plays: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DesktopRange {
    pub from: String,
    pub to: String,
    pub plays: i64,
    pub matched: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportRow {
    pub id: i64,
    pub imported_at: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub added: i64,
    pub already_had: i64,
}

/// API.md §4.1 `DataCoverage`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DataCoverage {
    pub export: Option<ExportRange>,
    pub desktop: Option<DesktopRange>,
    pub logger_runs_since: Option<String>,
    pub imports: Vec<ImportRow>,
    pub private_excluded: i64,
}

/// `get_data_coverage`: every stored row counts (private and reconciled ones show a source covered
/// the date); `coverage.gaps` = every no-data stretch of the whole history.
pub fn data_coverage<Tz: TimeZone>(cx: &Ctx<Tz>) -> Result<Covered<DataCoverage>> {
    let today = cx.today();
    let first = coverage::first_day(cx.conn)?.unwrap_or(today);
    let model = view_model(cx, first, today)?;
    let batches = export_batches(cx.conn)?;
    let export = match (
        batches.iter().map(|b| b.from).min(),
        batches.iter().map(|b| b.to).max(),
    ) {
        (Some(a), Some(b)) => Some(ExportRange {
            from: day(a),
            to: day(b),
            plays: batches.iter().map(|b| b.plays).sum(),
        }),
        _ => None,
    };
    let desktop = cx.conn.query_row(
        concat!(
            // unfiltered: what the desktop log stored; 0008's partial index holds only its rows
            "SELECT (SELECT substr(COALESCE(ended_at_local, ended_at_utc), 1, 10) FROM listening_event",
            " WHERE source = 'logger' ORDER BY ended_at_utc LIMIT 1),",
            // unfiltered: as above, the last desktop day
            " (SELECT substr(COALESCE(ended_at_local, ended_at_utc), 1, 10) FROM listening_event",
            " WHERE source = 'logger' ORDER BY ended_at_utc DESC LIMIT 1),",
            " IFNULL(SUM(counts_as_play), 0),",
            " IFNULL(SUM(CASE WHEN reconciled_with IS NOT NULL THEN counts_as_play END), 0)",
            // unfiltered: reconciled rows are counted on purpose (Your data shows them)
            " FROM listening_event WHERE source = 'logger'"
        ),
        [],
        |r| {
            let (a, b): (Option<String>, Option<String>) = (r.get(0)?, r.get(1)?);
            Ok(a.zip(b)
                .map(|(from, to)| (from, to, r.get::<_, i64>(2), r.get::<_, i64>(3))))
        },
    )?;
    let desktop = match desktop {
        Some((from, to, plays, matched)) => Some(DesktopRange {
            from,
            to,
            plays: plays?,
            matched: matched?,
        }),
        None => None,
    };
    let range: HashMap<i64, (NaiveDate, NaiveDate)> = batches
        .iter()
        .filter_map(|b| Some((b.id?, (b.from, b.to))))
        .collect();
    let mut st = cx.conn.prepare(
        "SELECT id, imported_at, record_count, already_had FROM import_batch
         WHERE kind = 'streaming_history' ORDER BY id DESC",
    )?;
    let imports = st
        .query_map([], |r| {
            let id: i64 = r.get(0)?;
            Ok(ImportRow {
                id,
                imported_at: r.get(1)?,
                from: range.get(&id).map(|x| day(x.0)),
                to: range.get(&id).map(|x| day(x.1)),
                added: r.get(2)?,
                already_had: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let private_excluded = if cx.include_private {
        0
    } else {
        cx.conn.query_row(
            "SELECT IFNULL(SUM(plays), 0) FROM listening_day WHERE private_session = 1",
            [],
            |r| r.get(0),
        )?
    };
    let data = DataCoverage {
        export,
        desktop,
        logger_runs_since: model.runs_since.map(day),
        imports,
        private_excluded,
    };
    covered(
        cx,
        data,
        view_coverage(&model, (first, today), today),
        false,
    )
}

/// API.md §4.1 `ExploreView`: Explore's "Save this view", the builder only.
#[derive(Debug, Clone, Serialize)]
pub struct ExploreView {
    pub id: i64,
    pub name: String,
    pub query: ExploreQuery,
}

/// `list_explore_views`, oldest first like saved filters.
pub fn list_explore_views(conn: &rusqlite::Connection) -> Result<Vec<ExploreView>> {
    let mut st = conn.prepare("SELECT id, name, query_json FROM explore_view ORDER BY id")?;
    let rows = st
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, name, q)| {
            Ok(ExploreView {
                id,
                name,
                query: serde_json::from_str(&q)?,
            })
        })
        .collect()
}

/// `save_explore_view`: the query validated like `explore`, the name like `save_filter`.
pub fn save_explore_view(
    conn: &rusqlite::Connection,
    name: &str,
    query: &ExploreQuery,
) -> Result<ExploreView> {
    let name = clean_line(name, limit::FILTER);
    if name.is_empty() {
        return Err(AppError::validation("Give the view a name."));
    }
    let query = clean_query(query)?;
    conn.execute(
        "INSERT INTO explore_view(name, query_json) VALUES (?1, ?2)",
        rusqlite::params![name, serde_json::to_string(&query)?],
    )?;
    Ok(ExploreView {
        id: conn.last_insert_rowid(),
        name,
        query,
    })
}

pub fn delete_explore_view(conn: &rusqlite::Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM explore_view WHERE id = ?1", [id])?;
    Ok(())
}
