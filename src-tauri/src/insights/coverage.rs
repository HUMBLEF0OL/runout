//! Coverage (M10 design §4.4, FR-048, API.md §4.1): which sources cover which local dates, and the
//! one comparison rule every "vs before" in the app goes through (Home's `PeriodStats` included, G-46).
//!
//! - **All devices:** from the first to the last export play's date (the export covers every device).
//! - **This PC only:** a date after the export's last one (any date when there is no export) the
//!   desktop logger ran: a `logger_run` interval touches it, or it holds a desktop play (that is how
//!   dates before `logger_run` existed are known, design §4.4).
//! - **No data:** everything else, dates before the export's first one included. Skipped by
//!   streaks and Regularity, never shown as silence.
//!
//! Sources U, S, E only (FR-031): `listening_event` and `logger_run`, no cache DB handle.
use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeDelta, TimeZone, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

pub use super::terms::{CoverageClass, Span};
use crate::error::Result;

/// API.md §4.1 `CoverageGap`: local dates, inclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gap {
    pub from: String,
    pub to: String,
}

/// API.md §4.1 `Coverage`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Coverage {
    pub compared_sources: Vec<&'static str>,
    pub desktop_only_since: Option<String>,
    pub gaps: Vec<Gap>,
    pub no_compare_reason: Option<String>,
}

/// API.md §4.1: one export batch's plays and their local date range (Your data's import history).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub id: Option<i64>,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub plays: i64,
}

/// What the stored data covers, loaded for the local dates `window` (the periods on screen).
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// First event's local date, any source: listening starts here.
    pub first: Option<NaiveDate>,
    /// First .. last export event's local date: all devices.
    pub export: Option<Span>,
    /// First local date `logger_run` recorded (0008); earlier desktop coverage is inferred from plays.
    pub runs_since: Option<NaiveDate>,
    /// First date the desktop logger ran (a run or a desktop play).
    pub pc_from: Option<NaiveDate>,
    window: Span,
    /// Dates in `window` the logger ran.
    pc_days: BTreeSet<NaiveDate>,
}

/// An event's local date (UTC when the local time is unknown or unparseable, G-29).
const LOCAL_DAY: &str = "substr(COALESCE(ended_at_local, substr(ended_at_utc, 1, 19)), 1, 10), substr(ended_at_utc, 1, 10)";

fn date(local: &str, utc: &str) -> Option<NaiveDate> {
    let p = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    p(local).or_else(|| p(utc))
}

fn day(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// "3 Oct 2026", as the UI writes a date.
fn show(d: NaiveDate) -> String {
    d.format("%-d %b %Y").to_string()
}

/// One event's local date: the first (`ASC`) or last (`DESC`) by end time where `filter` holds.
fn edge(conn: &Connection, filter: &str, order: &str) -> Result<Option<NaiveDate>> {
    // Export rows have no partial index: without the hint the bundled SQLite scans and sorts every
    // row (600 ms at 500k events, kb/perf-integrity.md); walking the time index stops at the first.
    let hint = if filter.contains("'export'") {
        "INDEXED BY listening_event_ended"
    } else {
        ""
    };
    Ok(conn
        .query_row(
            // unfiltered: coverage counts every stored row (private and reconciled plays still show a source covered the date)
            &format!(
                "SELECT {LOCAL_DAY} FROM listening_event {hint} WHERE {filter} ORDER BY ended_at_utc {order} LIMIT 1"
            ),
            [],
            |r| Ok(date(&r.get::<_, String>(0)?, &r.get::<_, String>(1)?)),
        )
        .optional()?
        .flatten())
}

fn utc_time(s: &str) -> Option<DateTime<Utc>> {
    s.get(..19)
        .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
        .map(|t| t.and_utc())
}

/// The first event's local date, any source (listening starts here).
pub fn first_day(conn: &Connection) -> Result<Option<NaiveDate>> {
    Ok([
        edge(conn, "source = 'export'", "ASC")?,
        edge(conn, "source = 'logger'", "ASC")?,
    ]
    .into_iter()
    .flatten()
    .min())
}

impl Model {
    /// Coverage for the local dates `from..=to` (clipped to the first event). `tz` turns
    /// `logger_run`'s UTC times into local dates (`chrono::Local` in the app); `now` closes a run
    /// still open.
    pub fn load<Tz: TimeZone>(
        conn: &Connection,
        tz: &Tz,
        from: NaiveDate,
        to: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<Model> {
        // Every lookup below sits on an index: `listening_event_ended`, 0008's `listening_event_logger`
        // (desktop rows only) and `logger_run_start`.
        let export_first = edge(conn, "source = 'export'", "ASC")?;
        let export_last = edge(conn, "source = 'export'", "DESC")?;
        let pc_play = edge(conn, "source = 'logger'", "ASC")?;
        let local = |s: &str| utc_time(s).map(|t| t.with_timezone(tz).date_naive());
        let runs_since = conn
            .query_row("SELECT MIN(start_utc) FROM logger_run", [], |r| {
                r.get::<_, Option<String>>(0)
            })?
            .and_then(|s| local(&s));
        let first = [export_first, pc_play].into_iter().flatten().min();
        let pc_from = [runs_since, pc_play].into_iter().flatten().min();
        let window = (from.max(first.unwrap_or(from)), to);

        let mut pc_days = BTreeSet::new();
        if window.0 <= window.1 {
            let (a, b) = (day(window.0), day(window.1));
            // ponytail: desktop-play dates count at all times, not only before `runs_since`: a logged
            // play proves the logger ran that day, and a crash-closed run may end before its last play.
            let mut st = conn.prepare(&format!(
                // unfiltered: any desktop row (private, reconciled) shows the logger ran that day
                "SELECT DISTINCT {LOCAL_DAY} FROM listening_event WHERE source = 'logger'
                   AND ended_at_utc >= date(?1, '-1 day') AND ended_at_utc < date(?2, '+2 day')"
            ))?;
            for r in st.query_map([&a, &b], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })? {
                let (l, u) = r?;
                pc_days.extend(date(&l, &u));
            }
            let mut st = conn.prepare(
                "SELECT start_utc, end_utc FROM logger_run
                 WHERE start_utc < date(?2, '+2 day') AND (end_utc IS NULL OR end_utc >= date(?1, '-1 day'))",
            )?;
            for r in st.query_map([&a, &b], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })? {
                let (s, e) = r?;
                let Some(s) = local(&s) else { continue };
                let e = e
                    .and_then(|e| local(&e))
                    .unwrap_or(now.with_timezone(tz).date_naive());
                pc_days.extend(
                    s.max(window.0)
                        .iter_days()
                        .take_while(|d| *d <= e.min(window.1)),
                );
            }
        }
        // Design §4.4: "This PC only" is past the export; a date before its first one is no data.
        let export_from = export_first.unwrap_or(NaiveDate::MIN);
        pc_days.retain(|d| *d >= window.0 && *d <= window.1 && *d >= export_from);
        Ok(Model {
            first,
            export: export_first.zip(export_last),
            runs_since,
            pc_from,
            window,
            pc_days,
        })
    }

    /// The coverage class of a local date in the loaded window (design §4.4).
    pub fn class(&self, d: NaiveDate) -> CoverageClass {
        if self.export.is_some_and(|(a, b)| a <= d && d <= b) {
            CoverageClass::AllDevices
        } else if self.pc_days.contains(&d) {
            CoverageClass::ThisPc
        } else {
            CoverageClass::NoData
        }
    }

    /// The desktop logger ran on `d` (a date in the loaded window on or after the export's first).
    /// A period past the last export counts plays on this PC only; then this is its coverage.
    pub fn ran(&self, d: NaiveDate) -> bool {
        self.pc_days.contains(&d)
    }

    /// The no-data stretches in `from..=to`, from the first event up to `today` (before it there is
    /// nothing to hatch: listening hadn't started; after it nothing has happened yet).
    pub fn gaps(&self, from: NaiveDate, to: NaiveDate, today: NaiveDate) -> Vec<Gap> {
        let Some(first) = self.first else {
            return vec![];
        };
        let mut out: Vec<(NaiveDate, NaiveDate)> = vec![];
        let to = to.min(today).min(self.window.1);
        for d in from
            .max(first)
            .max(self.window.0)
            .iter_days()
            .take_while(|d| *d <= to)
        {
            if self.class(d) != CoverageClass::NoData {
                continue;
            }
            match out.last_mut() {
                Some((_, b)) if *b + TimeDelta::days(1) == d => *b = d,
                _ => out.push((d, d)),
            }
        }
        out.into_iter()
            .map(|(a, b)| Gap {
                from: day(a),
                to: day(b),
            })
            .collect()
    }

    /// The desktop log covers `s`: it ran by its first day and on at least one of its days.
    fn pc_covers(&self, s: Span) -> bool {
        self.pc_from.is_some_and(|p| p <= s.0) && self.pc_days.range(s.0..=s.1).next().is_some()
    }

    /// FR-048, the one comparison rule (API.md §4.1): `cur` against `vs` (`None` = all time, nothing
    /// to compare). A comparison uses only the sources that cover the current and every earlier
    /// period: export and desktop (`['S','E']`) or export only (`['E']`) while `cur` ends on or
    /// before the export's last date; desktop only (`['S']`, `desktop_only_since` set) once it reaches
    /// past it, or when there is no export. When no source covers both sides the delta is hidden
    /// and `no_compare_reason` says why. The loaded window must hold `cur` and `vs`.
    pub fn compare(&self, cur: Span, vs: Option<&[Span]>, today: NaiveDate) -> Coverage {
        let end = cur.1.min(today);
        let desktop_only_since = self
            .export
            .filter(|(_, last)| end > *last)
            .map(|(_, last)| day(last + TimeDelta::days(1)));
        let start = vs
            .into_iter()
            .flatten()
            .map(|s| s.0)
            .fold(cur.0, NaiveDate::min);
        let gaps = self.gaps(start, end, today);
        let hidden = |reason: String| Coverage {
            compared_sources: vec![],
            desktop_only_since: desktop_only_since.clone(),
            gaps: gaps.clone(),
            no_compare_reason: Some(reason),
        };
        let Some(vs) = vs.filter(|v| !v.is_empty()) else {
            return hidden("All time has nothing before it to compare with.".into());
        };
        match self.first {
            None => return hidden("There's no listening to compare with yet.".into()),
            Some(f) if vs.iter().all(|s| s.1 < f) => {
                return hidden(format!("There's no listening before {}.", show(f)));
            }
            _ => {}
        }
        let pc = std::iter::once(&cur).chain(vs).all(|s| self.pc_covers(*s));
        let compared_sources = if desktop_only_since.is_some() || self.export.is_none() {
            if !pc {
                let earliest = vs.iter().map(|s| s.0).min().unwrap_or(cur.0);
                return hidden(match self.pc_from {
                    Some(p) if earliest < p => format!(
                        "Plays on this PC are only logged from {}, so there's nothing like for like to compare with.",
                        show(p)
                    ),
                    Some(_) => "The desktop log wasn't running in the period compared, so there's nothing like for like to compare with.".into(),
                    None => "Plays on this PC aren't logged, so there's nothing like for like to compare with.".into(),
                });
            }
            vec!["S"]
        } else if pc {
            vec!["S", "E"]
        } else {
            vec!["E"]
        };
        Coverage {
            compared_sources,
            desktop_only_since,
            gaps,
            no_compare_reason: None,
        }
    }
}

/// The events a comparison counts (FR-048), as a CTE named `cmp` with `base`'s columns (`local`
/// included); `None` when nothing is compared. Desktop only = every desktop play, reconciled ones
/// too: they are the desktop log's own record of a date the export also covers, so both sides count
/// the same way.
pub(crate) fn compared(cov: &Coverage, include_private: bool) -> Option<String> {
    let filter = match cov.compared_sources.as_slice() {
        [] => return None,
        ["S"] => "e.source = 'logger'",
        ["E"] => "e.source = 'export'",
        _ => "e.reconciled_with IS NULL",
    };
    let private = if include_private {
        ""
    } else {
        " AND e.private_session = 0"
    };
    Some(format!(
        // unfiltered: base's private filter by hand; the source filter decides reconciled rows (above)
        "WITH cmp AS NOT MATERIALIZED (SELECT e.*, COALESCE(e.ended_at_local, substr(e.ended_at_utc, 1, 19)) AS local
           FROM listening_event e WHERE {filter}{private})"
    ))
}

//// Export batches and their plays' local date range, oldest first (Your data, design §4.4). One
/// unsorted pass (NFR-04: SQL groups by batch with a sort of every export row).
pub fn export_batches(conn: &Connection) -> Result<Vec<Batch>> {
    let mut st = conn.prepare(
        // unfiltered: the import history lists every stored export row
        "SELECT import_batch_id, substr(COALESCE(ended_at_local, substr(ended_at_utc, 1, 19)), 1, 10),
                counts_as_play FROM listening_event WHERE source = 'export'",
    )?;
    let mut by: std::collections::HashMap<Option<i64>, (String, String, i64)> =
        std::collections::HashMap::new();
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        let (id, l, play) = (
            r.get::<_, Option<i64>>(0)?,
            super::text(r, 1)?,
            r.get::<_, i64>(2)?,
        );
        match by.get_mut(&id) {
            Some(b) => {
                if l < b.0.as_str() {
                    b.0 = l.to_owned();
                }
                if l > b.1.as_str() {
                    b.1 = l.to_owned();
                }
                b.2 += play;
            }
            None => {
                by.insert(id, (l.to_owned(), l.to_owned(), play));
            }
        }
    }
    let mut groups: Vec<(Option<i64>, (String, String, i64))> = by.into_iter().collect();
    groups.sort_by(|a, b| a.1.0.cmp(&b.1.0).then_with(|| a.0.cmp(&b.0)));
    let mut out = vec![];
    for (id, (a, b, plays)) in groups {
        if let (Some(from), Some(to)) = (date(&a, ""), date(&b, "")) {
            out.push(Batch {
                id,
                from,
                to,
                plays,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{Ev, ev, put};
    use super::super::terms::streaks;
    use super::*;
    use crate::db;
    use crate::importer::events::Source;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn now() -> DateTime<Utc> {
        d("2026-10-19").and_hms_opt(12, 0, 0).unwrap().and_utc()
    }
    fn desktop(utc: &str) -> Ev<'_> {
        Ev {
            source: Source::Logger,
            ..ev(utc, 60_000, "Song One", None)
        }
    }

    /// Export 1–10 Oct; logger runs 12–13 Oct and 15 Oct (still open is not tested here); no data on 11 and 14 Oct.
    fn db() -> Connection {
        let c = db::open_in_memory().unwrap();
        for t in [
            "2026-10-01T10:00:00Z",
            "2026-10-09T10:00:00Z",
            "2026-10-10T10:00:00Z",
        ] {
            put(&c, ev(t, 60_000, "Song One", None));
        }
        put(&c, desktop("2026-10-12T10:00:00Z"));
        put(&c, desktop("2026-10-13T10:00:00Z"));
        c.execute_batch(
            "INSERT INTO logger_run(start_utc, end_utc, reason) VALUES
               ('2026-10-12T08:00:00Z', '2026-10-13T22:00:00Z', 'off'),
               ('2026-10-15T08:00:00Z', '2026-10-15T09:00:00Z', 'exit');",
        )
        .unwrap();
        c
    }

    #[test]
    fn dates_take_their_coverage_class() {
        let c = db();
        let m = Model::load(&c, &Utc, d("2026-09-01"), d("2026-10-19"), now()).unwrap();
        use CoverageClass::*;
        assert_eq!(m.first, Some(d("2026-10-01")));
        assert_eq!(m.export, Some((d("2026-10-01"), d("2026-10-10"))));
        assert_eq!(m.class(d("2026-10-05")), AllDevices); // no play, but the export covers it
        assert_eq!(m.class(d("2026-10-11")), NoData);
        assert_eq!(m.class(d("2026-10-12")), ThisPc);
        assert_eq!(m.class(d("2026-10-15")), ThisPc); // logger ran, no play
        assert_eq!(m.class(d("2026-10-16")), NoData);
        assert_eq!(
            m.gaps(d("2026-09-01"), d("2026-10-30"), d("2026-10-19")),
            [
                ("2026-10-11", "2026-10-11"),
                ("2026-10-14", "2026-10-14"),
                ("2026-10-16", "2026-10-19")
            ]
            .map(|(a, b)| Gap {
                from: a.into(),
                to: b.into()
            })
        );
    }

    /// Design §4.4: a no-data date breaks nothing and counts nothing; a covered date without a play breaks.
    #[test]
    fn a_no_data_day_does_not_break_a_streak() {
        let c = db();
        let m = Model::load(&c, &Utc, d("2026-10-01"), d("2026-10-19"), now()).unwrap();
        let played: BTreeSet<_> = ["2026-10-09", "2026-10-10", "2026-10-12", "2026-10-13"]
            .map(d)
            .into();
        // 9, 10, (11 no data), 12, 13 = 4 days
        let (best, current) = streaks(d("2026-10-01"), d("2026-10-13"), &played, |x| m.class(x));
        assert_eq!((best.unwrap().days, current), (4, 4));
        // 15 Oct: the logger ran, nothing played → the run is broken
        let (_, current) = streaks(d("2026-10-01"), d("2026-10-15"), &played, |x| m.class(x));
        assert_eq!(current, 0);
    }

    /// Design §4.4: before `logger_run` existed, the logger counts as running on dates with a desktop play.
    #[test]
    fn pre_logger_run_dates_count_desktop_play_dates() {
        let c = db();
        put(&c, desktop("2026-10-11T10:00:00Z")); // past the export, before any run
        put(&c, desktop("2026-09-20T10:00:00Z")); // before the export: no data (design §4.4)
        let m = Model::load(&c, &Utc, d("2026-09-01"), d("2026-10-19"), now()).unwrap();
        assert_eq!(m.runs_since, Some(d("2026-10-12")));
        assert_eq!(m.pc_from, Some(d("2026-09-20")));
        assert_eq!(m.first, Some(d("2026-09-20")));
        assert_eq!(m.class(d("2026-10-11")), CoverageClass::ThisPc);
        assert_eq!(m.class(d("2026-09-20")), CoverageClass::NoData);
        assert_eq!(m.class(d("2026-09-21")), CoverageClass::NoData);
    }

    #[test]
    fn crossing_the_export_end_compares_desktop_only() {
        let c = db();
        put(&c, desktop("2026-10-01T11:00:00Z")); // the logger ran in the earlier week …
        put(&c, desktop("2026-10-09T11:00:00Z")); // … and inside the export (a reconciled twin, say)
        let m = Model::load(&c, &Utc, d("2026-09-01"), d("2026-10-19"), now()).unwrap();
        let cov = m.compare(
            (d("2026-10-08"), d("2026-10-14")),
            Some(&[(d("2026-10-01"), d("2026-10-07"))]),
            d("2026-10-19"),
        );
        assert_eq!(cov.compared_sources, ["S"]);
        assert_eq!(cov.desktop_only_since.as_deref(), Some("2026-10-11"));
        assert_eq!(cov.no_compare_reason, None);
        // inside the export: export (and the desktop log where it ran by the earlier period)
        let inside = |vs: Span| {
            m.compare(
                (d("2026-10-08"), d("2026-10-10")),
                Some(&[vs]),
                d("2026-10-19"),
            )
        };
        assert_eq!(
            inside((d("2026-10-01"), d("2026-10-01"))).compared_sources,
            ["S", "E"]
        );
        assert_eq!(
            inside((d("2026-10-03"), d("2026-10-04"))).compared_sources,
            ["E"]
        );
        assert_eq!(
            inside((d("2026-10-03"), d("2026-10-04"))).desktop_only_since,
            None
        );
    }

    #[test]
    fn no_logger_run_in_the_earlier_period_hides_the_delta() {
        let c = db();
        let m = Model::load(&c, &Utc, d("2026-09-01"), d("2026-10-19"), now()).unwrap();
        let cur = (d("2026-10-08"), d("2026-10-14"));
        let cov = m.compare(
            cur,
            Some(&[(d("2026-10-01"), d("2026-10-07"))]),
            d("2026-10-19"),
        );
        assert_eq!(cov.compared_sources, Vec::<&str>::new());
        assert_eq!(cov.desktop_only_since.as_deref(), Some("2026-10-11"));
        assert_eq!(
            cov.no_compare_reason.as_deref(),
            Some(
                "Plays on this PC are only logged from 12 Oct 2026, so there's nothing like for like to compare with."
            )
        );
        // the log began earlier but was off for the whole earlier period
        put(&c, desktop("2026-09-02T10:00:00Z"));
        let m = Model::load(&c, &Utc, d("2026-09-01"), d("2026-10-19"), now()).unwrap();
        let cov = m.compare(
            cur,
            Some(&[(d("2026-10-01"), d("2026-10-07"))]),
            d("2026-10-19"),
        );
        assert!(
            cov.no_compare_reason
                .unwrap()
                .starts_with("The desktop log wasn't running")
        );
        // all time and nothing before: hidden too
        assert!(
            m.compare(cur, None, d("2026-10-19"))
                .no_compare_reason
                .is_some()
        );
        let before = m.compare(
            cur,
            Some(&[(d("2025-01-01"), d("2025-01-07"))]),
            d("2026-10-19"),
        );
        assert_eq!(
            before.no_compare_reason.as_deref(),
            Some("There's no listening before 2 Sep 2026.")
        );
    }

    #[test]
    fn an_open_run_reaches_today() {
        let c = db();
        c.execute(
            "INSERT INTO logger_run(start_utc) VALUES ('2026-10-17T08:00:00Z')",
            [],
        )
        .unwrap();
        let m = Model::load(&c, &Utc, d("2026-10-01"), d("2026-10-19"), now()).unwrap();
        assert_eq!(
            ["2026-10-16", "2026-10-17", "2026-10-19"].map(|x| m.class(d(x))),
            [
                CoverageClass::NoData,
                CoverageClass::ThisPc,
                CoverageClass::ThisPc
            ]
        );
    }

    #[test]
    fn batches_list_their_date_ranges() {
        let c = db();
        c.execute_batch(
            "INSERT INTO import_batch(id, kind, checksum) VALUES (1, 'streaming_history', 'a');
             UPDATE listening_event SET import_batch_id = 1 WHERE source = 'export' AND ended_at_utc < '2026-10-05';",
        )
        .unwrap();
        let b = export_batches(&c).unwrap();
        assert_eq!(
            b,
            [
                Batch {
                    id: Some(1),
                    from: d("2026-10-01"),
                    to: d("2026-10-01"),
                    plays: 1
                },
                Batch {
                    id: None,
                    from: d("2026-10-09"),
                    to: d("2026-10-10"),
                    plays: 2
                },
            ]
        );
    }
}
