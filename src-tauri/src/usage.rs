//! Local usage counters and success metrics (TSD §4.12, FR-045, BRD §15). Never leave the PC,
//! never exported (export.rs skips `usage_event`).
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    AppOpen,
    PageVisit,
    Capture,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::AppOpen => "app_open",
            Self::PageVisit => "page_visit",
            Self::Capture => "capture",
        }
    }
}

/// Insert one `usage_event`. `capture`: `detail` = `'pc' | 'phone'`, `duration_ms` = open/arrival → save.
pub fn record(
    conn: &Connection,
    kind: Kind,
    detail: Option<&str>,
    duration_ms: Option<i64>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO usage_event(kind, detail, duration_ms) VALUES (?1, ?2, ?3)",
        // TSD §7: every duration is bounded here (bogus input, clock changes, days in the queue)
        // so the BRD §15 median stays meaningful.
        params![
            kind.as_str(),
            detail,
            duration_ms.map(|ms| ms.clamp(0, 3_600_000))
        ],
    )?;
    Ok(())
}

/// `record_page_visit`: `page` is the route's first segment (`home`, `search`, `rediscover`, …).
pub fn record_page_visit(conn: &Connection, page: &str) -> Result<()> {
    let ok = (1..=40).contains(&page.len())
        && page
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !ok {
        return Err(AppError::validation("Unknown page."));
    }
    record(conn, Kind::PageVisit, Some(page), None)
}

/// API.md `SuccessMetrics` (BRD §15).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SuccessMetrics {
    pub capture_median_s: f64,
    pub days_opened_this_week: i64,
    pub searches_this_week: i64,
    pub rediscover_visits_this_week: i64,
    pub moved_after_14d_pct: i64,
    pub explored_rated_pct: i64,
}

pub fn get_success_metrics(conn: &Connection) -> Result<SuccessMetrics> {
    metrics_at(conn, &crate::db::now())
}

/// "This week" = the 7 days up to `now` (rolling); days opened = distinct local dates in the
/// last 7 local calendar days including today (so at most 7). Capture median over 30 days.
/// Moved after 14 d (BRD §15) = backlog items added ≥ 14 days ago whose status 14 days after
/// adding (from `status_history`) wasn't `planned`. Explored rated = explored items with a
/// rating or review.
fn metrics_at(conn: &Connection, now: &str) -> Result<SuccessMetrics> {
    const ISO: &str = "'%Y-%m-%dT%H:%M:%SZ'";
    let mut st = conn.prepare(&format!(
        "SELECT duration_ms FROM usage_event WHERE kind = 'capture' AND duration_ms IS NOT NULL
           AND at >= strftime({ISO}, ?1, '-30 days') ORDER BY duration_ms"
    ))?;
    let ms: Vec<i64> = st
        .query_map([now], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let median_ms = match ms.len() {
        0 => 0.0,
        n if n % 2 == 1 => ms[n / 2] as f64,
        n => (ms[n / 2 - 1] as f64 + ms[n / 2] as f64) / 2.0,
    };
    let sql = format!(
        "WITH wk AS (SELECT * FROM usage_event WHERE at > strftime({ISO}, ?1, '-7 days') AND at <= ?1)
         SELECT
           (SELECT count(DISTINCT date(at, 'localtime')) FROM usage_event WHERE kind = 'app_open'
              AND date(at, 'localtime') > date(?1, 'localtime', '-7 days') AND at <= ?1),
           (SELECT count(*) FROM wk WHERE kind = 'page_visit' AND detail = 'search'),
           (SELECT count(*) FROM wk WHERE kind = 'page_visit' AND detail = 'rediscover'),
           (SELECT CAST(round(100.0 * total(moved) / max(count(*), 1)) AS INT) FROM (
              SELECT (
                (COALESCE((SELECT h.to_status FROM status_history h WHERE h.item_id = i.id
                                 AND h.at <= strftime({ISO}, i.added_at, '+14 days')
                               ORDER BY h.at DESC, h.id DESC LIMIT 1), 'planned') <> 'planned')
                OR
                EXISTS (SELECT 1 FROM backlog_history bh WHERE bh.item_id = i.id
                          AND bh.in_backlog = 0
                          AND bh.at <= strftime({ISO}, i.added_at, '+14 days'))
              ) AS moved
              FROM item i WHERE i.added_at <= strftime({ISO}, ?1, '-14 days')
                -- backlog items: first status planned, and in the backlog or moved on from planned (G-16)
                AND (SELECT h.to_status FROM status_history h WHERE h.item_id = i.id
                      ORDER BY h.at, h.id LIMIT 1) = 'planned'
                AND (i.in_backlog = 1 OR i.status <> 'planned'
                     OR EXISTS (SELECT 1 FROM backlog_history bh WHERE bh.item_id = i.id AND bh.in_backlog = 1)))),
           (SELECT CAST(round(100.0 * total(rating IS NOT NULL OR trim(COALESCE(review, '')) <> '')
                              / max(count(*), 1)) AS INT)
              FROM item WHERE status = 'explored')"
    );
    let r = conn.query_row(&sql, [now], |r| {
        Ok(SuccessMetrics {
            capture_median_s: (median_ms / 100.0).round() / 10.0,
            days_opened_this_week: r.get(0)?,
            searches_this_week: r.get(1)?,
            rediscover_visits_this_week: r.get(2)?,
            moved_after_14d_pct: r.get(3)?,
            explored_rated_pct: r.get(4)?,
        })
    })?;
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_bounded_for_every_caller() {
        let c = crate::db::open_in_memory().unwrap();
        for ms in [-5, i64::MAX] {
            record(&c, Kind::Capture, Some("phone"), Some(ms)).unwrap();
        }
        let got: Vec<i64> = c
            .prepare("SELECT duration_ms FROM usage_event ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(got, [0, 3_600_000], "clock went back / queued for days");
    }
    use crate::db;

    const NOW: &str = "2026-10-18T12:00:00Z";

    fn ev(c: &Connection, kind: &str, at: &str, detail: Option<&str>, ms: Option<i64>) {
        c.execute(
            "INSERT INTO usage_event(kind, at, detail, duration_ms) VALUES (?1, ?2, ?3, ?4)",
            params![kind, at, detail, ms],
        )
        .unwrap();
    }

    /// "This week" = the last 7 local calendar days including today: hourly opens over the
    /// past 8 days span 8 local dates in a rolling 168 h window, but only 7 count.
    #[test]
    fn days_opened_is_at_most_7_local_dates() {
        let c = db::open_in_memory().unwrap();
        for h in 0..=200 {
            c.execute(
                "INSERT INTO usage_event(kind, at) VALUES ('app_open', strftime('%Y-%m-%dT%H:%M:%SZ', ?1, ?2))",
                params![NOW, format!("-{h} hours")],
            )
            .unwrap();
        }
        assert_eq!(metrics_at(&c, NOW).unwrap().days_opened_this_week, 7);
    }

    /// BRD §15 counts backlog items only: items added straight as explored, or never in the
    /// backlog (auto-created artists still planned), are left out. Exploring takes an item out of
    /// the backlog (status.rs), so a moved item still counts.
    #[test]
    fn moved_after_14d_counts_backlog_items_only() {
        let c = db::open_in_memory().unwrap();
        c.execute_batch(
            "INSERT INTO item(id, type, added_at, status, in_backlog) VALUES
                 (1,'album','2026-09-18T00:00:00Z','in_progress',1),
                 (2,'album','2026-09-18T00:00:00Z','planned',1),
                 (3,'album','2026-09-18T00:00:00Z','explored',0),
                 (4,'artist','2026-09-18T00:00:00Z','planned',0),
                 (5,'album','2026-09-18T00:00:00Z','explored',0);
             INSERT INTO status_history(item_id, from_status, to_status, at) VALUES
                 (1,NULL,'planned','2026-09-18T00:00:00Z'),(1,'planned','in_progress','2026-09-20T00:00:00Z'),
                 (2,NULL,'planned','2026-09-18T00:00:00Z'),
                 (3,NULL,'explored','2026-09-18T00:00:00Z'),
                 (4,NULL,'planned','2026-09-18T00:00:00Z'),
                 (5,NULL,'planned','2026-09-18T00:00:00Z'),(5,'planned','explored','2026-09-21T00:00:00Z');",
        )
        .unwrap();
        assert_eq!(metrics_at(&c, NOW).unwrap().moved_after_14d_pct, 67); // 1 and 5 of 1, 2, 5
    }

    /// G-16: a planned item removed from the backlog within 14 days counts as moved.
    #[test]
    fn moved_after_14d_sees_items_leaving_backlog_while_planned() {
        let c = db::open_in_memory().unwrap();
        c.execute_batch(
            "INSERT INTO item(id, type, added_at, status, in_backlog) VALUES
                 (1,'album','2026-09-18T00:00:00Z','planned',1),
                 (2,'album','2026-09-18T00:00:00Z','planned',1);
             INSERT INTO status_history(item_id, from_status, to_status, at) VALUES
                 (1,NULL,'planned','2026-09-18T00:00:00Z'),
                 (2,NULL,'planned','2026-09-18T00:00:00Z');
             -- item 1 leaves the backlog on day 5 while staying planned
             UPDATE item SET in_backlog = 0 WHERE id = 1;
             UPDATE backlog_history SET at = '2026-09-23T00:00:00Z' WHERE item_id = 1 AND in_backlog = 0;",
        )
        .unwrap();
        // Item 1 moved (left backlog on day 5), item 2 still in backlog planned. 1 of 2 = 50%.
        assert_eq!(metrics_at(&c, NOW).unwrap().moved_after_14d_pct, 50);
    }

    #[test]
    fn page_visit_validated() {
        let c = db::open_in_memory().unwrap();
        record_page_visit(&c, "rediscover").unwrap();
        for bad in ["", "Home", "a/b", "x'; --", &"a".repeat(41)] {
            assert!(record_page_visit(&c, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn success_metrics_per_brd_15() {
        let c = db::open_in_memory().unwrap();
        assert_eq!(metrics_at(&c, NOW).unwrap().capture_median_s, 0.0);
        // opened on 3 distinct days this week (noon UTC: distinct local days in any zone) + 1 too old
        for at in [
            "2026-10-13T12:00:00Z",
            "2026-10-15T12:00:00Z",
            "2026-10-15T12:05:00Z",
            "2026-10-18T11:00:00Z",
            "2026-10-01T12:00:00Z",
        ] {
            ev(&c, "app_open", at, None, None);
        }
        ev(
            &c,
            "page_visit",
            "2026-10-17T10:00:00Z",
            Some("search"),
            None,
        );
        ev(
            &c,
            "page_visit",
            "2026-10-16T10:00:00Z",
            Some("search"),
            None,
        );
        ev(
            &c,
            "page_visit",
            "2026-10-02T10:00:00Z",
            Some("search"),
            None,
        );
        ev(
            &c,
            "page_visit",
            "2026-10-17T10:00:00Z",
            Some("rediscover"),
            None,
        );
        for ms in [4000, 9000, 6000, 7000] {
            ev(&c, "capture", "2026-10-10T10:00:00Z", Some("pc"), Some(ms));
        }
        ev(
            &c,
            "capture",
            "2026-08-01T10:00:00Z",
            Some("pc"),
            Some(90_000),
        ); // > 30 days

        // 4 items added 30 days ago: moved by day 14, moved on day 20 (too late), never moved, explored+rated
        c.execute_batch(
            "INSERT INTO item(id, type, added_at) VALUES (1,'album','2026-09-18T00:00:00Z'),(2,'album','2026-09-18T00:00:00Z'),
                 (3,'album','2026-09-18T00:00:00Z'),(4,'album','2026-09-18T00:00:00Z'),(5,'album','2026-10-15T00:00:00Z');
             INSERT INTO status_history(item_id, from_status, to_status, at) VALUES
                 (1,NULL,'planned','2026-09-18T00:00:00Z'),(1,'planned','in_progress','2026-09-25T00:00:00Z'),
                 (2,NULL,'planned','2026-09-18T00:00:00Z'),(2,'planned','explored','2026-10-08T00:00:00Z'),
                 (3,NULL,'planned','2026-09-18T00:00:00Z'),
                 (4,NULL,'explored','2026-09-18T00:00:00Z'),
                 (5,NULL,'in_progress','2026-10-15T00:00:00Z');
             UPDATE item SET status = 'explored', rating = 4 WHERE id = 4;
             UPDATE item SET status = 'explored', review = '  ' WHERE id = 2;",
        )
        .unwrap();
        let m = metrics_at(&c, NOW).unwrap();
        assert_eq!(
            m,
            SuccessMetrics {
                capture_median_s: 6.5,
                days_opened_this_week: 3,
                searches_this_week: 2,
                rediscover_visits_this_week: 1,
                moved_after_14d_pct: 33, // item 1 of backlog items 1–3; 4 added as explored, 5 too new
                explored_rated_pct: 50,  // 4 rated; 2 has a blank review
            }
        );
    }
}
