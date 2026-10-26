// TEMP scratch: dump v3 payloads from a kept perf DB for a key-shape comparison with api.ts mocks.
use chrono::{TimeZone, Utc};
use runout_lib::db;
use runout_lib::insights::explore::{
    ExploreQuery, data_coverage, explore, list_explore_views, save_explore_view,
};
use runout_lib::insights::habits::{discovery, habits};
use runout_lib::insights::history::{Measure, records, trends};
use runout_lib::insights::report::report;
use runout_lib::insights::v3::{Ctx, PeriodQuery};
use runout_lib::insights::{Range, listening};
use serde_json::{json, to_value};

#[test]
#[ignore]
fn shape_dump() {
    let conn = db::open(std::path::Path::new(
        &std::env::var("RUNOUT_PERF_DB").unwrap(),
    ))
    .unwrap();
    let now = Utc.with_ymd_and_hms(2025, 12, 31, 12, 0, 0).unwrap();
    let cx = Ctx {
        conn: &conn,
        include_private: false,
        tz: &Utc,
        now,
    };
    let p: PeriodQuery =
        serde_json::from_value(json!({"grain": "month", "vs": "prev", "at": "2025-06"})).unwrap();
    let py: PeriodQuery =
        serde_json::from_value(json!({"grain": "year", "vs": "prev", "at": "2025"})).unwrap();
    let name: String = conn.query_row("SELECT artist_name FROM listening_event WHERE counts_as_play GROUP BY lower(artist_name) ORDER BY count(*) DESC LIMIT 1", [], |r| r.get(0)).unwrap();
    let q: ExploreQuery = serde_json::from_value(json!({"series": [{"kind": "artist", "name": name}], "from": "2025-01-01", "to": "2025-12-31", "grain": "month", "measure": "hours"})).unwrap();
    let _ = save_explore_view(&conn, "shape", &q);
    let out = json!({
        "get_period_report": to_value(report(&cx, &p).unwrap()).unwrap(),
        "get_period_report_year": to_value(report(&cx, &py).unwrap()).unwrap(),
        "get_trends": to_value(trends(&cx, Measure::Hours).unwrap()).unwrap(),
        "get_discovery": to_value(discovery(&cx, &p).unwrap()).unwrap(),
        "get_listening_habits": to_value(habits(&cx, &p).unwrap()).unwrap(),
        "get_records": to_value(records(&cx).unwrap()).unwrap(),
        "explore": to_value(explore(&cx, &q).unwrap()).unwrap(),
        "get_data_coverage": to_value(data_coverage(&cx).unwrap()).unwrap(),
        "get_listening_insights": to_value(listening::listening(&conn, Range::Month, false, now).unwrap()).unwrap(),
        "list_explore_views": to_value(list_explore_views(&conn).unwrap()).unwrap(),
    });
    std::fs::write(
        std::env::var("DUMP_OUT").unwrap(),
        serde_json::to_string_pretty(&out).unwrap(),
    )
    .unwrap();
}
