//! M10 WI 5: the seven Insights v3 commands over one synthetic history, every figure hand-computed
//! below (structure only, invented names). Zone UTC, today Mon 19 Oct 2026.
//!
//! | id | source | local end            | ms      | artist | track | end / skip     |
//! |----|--------|----------------------|---------|--------|-------|----------------|
//! | A1 | export | 2026-08-10 10:00     | 200 000 | Aster  | One   | trackdone      |
//! | A2 | export | 2026-08-11 10:00     | 100 000 | Aster  | Two   | fwdbtn, skip   |
//! | S1 | export | 2026-09-02 10:00     | 300 000 | Birch  | Three | trackdone (clickrow) |
//! | S2 | export | 2026-09-03 22:00     |   5 000 | Birch  | Three | fwdbtn, skip   |
//! | S4 | export | 2026-09-04 22:00     |   4 000 | Birch  | Three | fwdbtn, skip   |
//! | S5 | export | 2026-09-05 22:00     |   3 000 | Birch  | Three | fwdbtn, skip   |
//! | S3 | export | 2026-10-03 10:00     |  60 000 | Aster  | One   | trackdone      |
//! | L1 | logger | 2026-09-02 10:00:20  | 300 000 | Birch  | Three | twin of S1 (reconciled) |
//! | L2 | logger | 2026-09-20 20:00     | 120 000 | Cedar  | Four  |                |
//! | L3 | logger | 2026-10-10 21:00     | 240 000 | Cedar  | Four  |                |
//! | L4 | logger | 2026-10-12 21:00     |  60 000 | Dahl   | Five  |                |
//!
//! Export covers 10 Aug – 3 Oct (all devices); the desktop log ran on its play dates only (no
//! `logger_run` rows): 2 Sep, 20 Sep, 10 Oct, 12 Oct.
use chrono::{TimeZone, Utc};
use rusqlite::Connection;
use serde_json::{Value, json};

use super::explore::{
    ExploreQuery, data_coverage, delete_explore_view, explore, list_explore_views,
    save_explore_view,
};
use super::habits::{discovery, habits};
use super::history::{Measure, records, trends};
use super::report::report;
use super::v3::{Ctx, PeriodQuery};
use crate::db;
use crate::error::Code;
use crate::importer::events::{Event, Source, insert};

#[allow(clippy::too_many_arguments)] // one call per table row reads best
fn put(
    c: &Connection,
    source: Source,
    local: &str,
    ms: i64,
    artist: &str,
    track: &str,
    end: Option<&str>,
    start: Option<&str>,
) -> i64 {
    let export = source == Source::Export;
    insert(
        c,
        &Event {
            source,
            ended_at_utc: format!("{local}Z"),
            ended_at_local: Some(local.into()),
            ms_played: ms,
            track_name: track.into(),
            artist_name: artist.into(),
            album_name: format!("{artist} LP"),
            spotify_track_uri: export.then(|| format!("spotify:track:Synthetic{track}000000000")),
            platform: Some("windows".into()),
            reason_start: start.map(Into::into),
            reason_end: end.map(Into::into),
            shuffle: None,
            duration_ms: None,
            skipped: export.then(|| end == Some("fwdbtn")),
            offline: export.then_some(false),
            private_session: false,
            item_id: None,
            import_batch_id: None,
        },
    )
    .unwrap()
    .unwrap()
}

fn history() -> Connection {
    let c = db::open_in_memory().unwrap();
    use Source::*;
    let td = Some("trackdone");
    let fwd = Some("fwdbtn");
    put(
        &c,
        Export,
        "2026-08-10T10:00:00",
        200_000,
        "Aster",
        "One",
        td,
        td,
    );
    put(
        &c,
        Export,
        "2026-08-11T10:00:00",
        100_000,
        "Aster",
        "Two",
        fwd,
        td,
    );
    let s1 = put(
        &c,
        Export,
        "2026-09-02T10:00:00",
        300_000,
        "Birch",
        "Three",
        td,
        Some("clickrow"),
    );
    put(
        &c,
        Export,
        "2026-09-03T22:00:00",
        5_000,
        "Birch",
        "Three",
        fwd,
        td,
    );
    put(
        &c,
        Export,
        "2026-09-04T22:00:00",
        4_000,
        "Birch",
        "Three",
        fwd,
        td,
    );
    put(
        &c,
        Export,
        "2026-09-05T22:00:00",
        3_000,
        "Birch",
        "Three",
        fwd,
        td,
    );
    put(
        &c,
        Export,
        "2026-10-03T10:00:00",
        60_000,
        "Aster",
        "One",
        td,
        td,
    );
    let l1 = put(
        &c,
        Logger,
        "2026-09-02T10:00:20",
        300_000,
        "Birch",
        "Three",
        None,
        None,
    );
    put(
        &c,
        Logger,
        "2026-09-20T20:00:00",
        120_000,
        "Cedar",
        "Four",
        None,
        None,
    );
    put(
        &c,
        Logger,
        "2026-10-10T21:00:00",
        240_000,
        "Cedar",
        "Four",
        None,
        None,
    );
    put(
        &c,
        Logger,
        "2026-10-12T21:00:00",
        60_000,
        "Dahl",
        "Five",
        None,
        None,
    );
    c.execute(
        "UPDATE listening_event SET reconciled_with = ?1 WHERE id = ?2",
        [s1, l1],
    )
    .unwrap();
    c.execute(
        "INSERT INTO item(type, display_name, status) VALUES ('artist', 'Birch', 'planned')",
        [],
    )
    .unwrap();
    c
}

fn cx(c: &Connection) -> Ctx<'_, Utc> {
    Ctx {
        conn: c,
        include_private: false,
        tz: &Utc,
        now: Utc.with_ymd_and_hms(2026, 10, 19, 12, 0, 0).unwrap(),
    }
}

fn period(grain: &str, vs: &str, at: Option<&str>) -> PeriodQuery {
    serde_json::from_value(json!({"grain": grain, "vs": vs, "at": at})).unwrap()
}

fn j<T: serde::Serialize>(v: T) -> Value {
    serde_json::to_value(v).unwrap()
}

fn texts(lines: &Value) -> Vec<String> {
    lines
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            l["parts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["text"].as_str().unwrap())
                .collect()
        })
        .collect()
}

#[test]
fn september_report_compares_export_only() {
    let c = history();
    let r = j(report(&cx(&c), &period("month", "prev", Some("2026-09"))).unwrap());
    // the desktop log starts 2 Sep, after August began: export only on both sides
    assert_eq!(
        r["coverage"],
        json!({"compared_sources": ["E"], "desktop_only_since": null, "gaps": [], "no_compare_reason": null})
    );
    assert_eq!(r["sources"], json!(["U", "S", "E"]));
    let d = &r["data"];
    assert_eq!(
        d["period"],
        json!({"grain": "month", "vs": "prev", "from": "2026-09-01", "to": "2026-09-30", "vs_from": "2026-08-01",
               "vs_to": "2026-08-31", "prev_at": "2026-08", "next_at": "2026-10", "days": 30})
    );
    let t = &d["tiles"];
    // value: S1 + S2 + S4 + S5 + L2 (L1 reconciled); now: export rows; before: A1 + A2
    assert_eq!(
        t["ms"],
        json!({"value": 432_000.0, "now": 312_000.0, "before": 300_000.0})
    );
    assert_eq!(t["plays"], json!({"value": 2.0, "now": 1.0, "before": 2.0}));
    assert_eq!(
        t["artists"],
        json!({"value": 2.0, "now": 1.0, "before": 1.0})
    );
    // Birch (first play S1, export) and Cedar (L2, desktop); like for like counts Birch
    assert_eq!(
        t["new_artists"],
        json!({"value": 2.0, "now": 1.0, "before": 1.0})
    );
    assert_eq!(
        t["days_listened"],
        json!({"value": 2.0, "now": 1.0, "before": 2.0})
    );
    assert_eq!(t["streak"], json!(0)); // 30 Sep is a covered day without a play
    assert_eq!(
        t["finished_pct"],
        json!({"value": 25.0, "now": 25.0, "before": 50.0})
    ); // 1 of 4 / 1 of 2
    let tl = d["timeline"].as_array().unwrap();
    assert_eq!(tl.len(), 30);
    assert_eq!(
        tl[1],
        json!({"key": "2026-09-02", "ms": 300_000, "before_ms": 0, "coverage": "all_devices"})
    );
    assert_eq!(tl[9]["before_ms"], json!(200_000)); // 10 Sep ↔ 10 Aug
    assert_eq!(tl[19]["ms"], json!(0)); // L2 is desktop: not in the export-only line
    assert_eq!(
        texts(&d["lines"]),
        vec![
            "Birch is new and already your #1 artist. You first played them on 2 Sep 2026.",
            "Your busiest hour was 10:00: 69% of your time.", // 300 000 of 432 000
            "2 Sep 2026 was your biggest day ever: 5 min.",
        ]
    );
    assert_eq!(d["top_new"]["name"], json!("Birch"));
    assert_eq!(d["top_new"]["status"], json!("planned"));
    assert!(d["top_new"]["item_id"].is_number());
    assert_eq!(
        (
            &d["top_new"]["first_played_local"],
            &d["top_new"]["ms"],
            &d["top_new"]["rank"],
            &d["top_new"]["share_pct"]
        ),
        (
            &json!("2026-09-02T10:00"),
            &json!(312_000),
            &json!(1),
            &json!(72.2)
        )
    );
    assert_eq!(
        (&d["also_new"], &d["also_new_more"]),
        (&json!(["Cedar"]), &json!(0))
    );
    assert_eq!(d["clock"]["hours"][10], json!(69.4));
    assert_eq!(d["ratio"]["tracks"]["value"], json!(2.0));
    assert_eq!(d["ratio"]["tracks_per_artist"]["value"], json!(1.0));
    // fingerprint: 2 of 30 days, all time on new artists, 2 artists, nothing played before
    assert_eq!(
        d["fingerprint"]["now"],
        json!({"regularity": 7.0, "discovery": 100.0, "focus": 100.0, "variety": 85.0, "repeat": 0.0})
    );
    let top = &d["most_played"]["artists"];
    assert_eq!(top[0]["name"], json!("Birch"));
    assert_eq!(top[0]["is_new"], json!(true)); // not in August's export ranking
    assert_eq!(top[0]["spark"].as_array().unwrap().len(), 12);
}

#[test]
fn a_week_past_the_export_compares_this_pc_only() {
    let c = history();
    let r = j(report(&cx(&c), &period("week", "prev", Some("2026-10-14"))).unwrap());
    assert_eq!(r["coverage"]["compared_sources"], json!(["S"]));
    assert_eq!(r["coverage"]["desktop_only_since"], json!("2026-10-04"));
    let d = &r["data"];
    assert_eq!(
        (
            &d["period"]["from"],
            &d["period"]["to"],
            &d["period"]["days"]
        ),
        (&json!("2026-10-12"), &json!("2026-10-18"), &json!(1))
    );
    assert_eq!(
        d["tiles"]["ms"],
        json!({"value": 60_000.0, "now": 60_000.0, "before": 240_000.0})
    );
    assert_eq!(
        d["tiles"]["days_listened"],
        json!({"value": 1.0, "now": 1.0, "before": 1.0})
    );
    // this-PC dates only: 2 Sep (the reconciled twin is the desktop's own record), 20 Sep, 10 and
    // 12 Oct; the dates between are no data and break nothing
    assert_eq!(d["tiles"]["streak"], json!(4));
    assert_eq!(
        texts(&d["lines"]),
        vec![
            "You listened 75% less on this PC than the week before.",
            "Dahl is new and already your #1 artist. You first played them on 12 Oct 2026.",
            "Your busiest hour was 21:00: 100% of your time.",
        ]
    );
    // October against September: the log only started on 2 Sep, so nothing is compared
    let r = j(report(&cx(&c), &period("month", "prev", None)).unwrap());
    assert_eq!(r["coverage"]["compared_sources"], json!([]));
    assert_eq!(
        r["coverage"]["no_compare_reason"],
        json!(
            "Plays on this PC are only logged from 2 Sep 2026, so there's nothing like for like to compare with."
        )
    );
    assert_eq!(
        r["data"]["tiles"]["ms"],
        json!({"value": 360_000.0, "now": null, "before": null})
    );
    assert_eq!(r["data"]["period"]["next_at"], json!(null));
    // all time, and a bad period
    let r = j(report(&cx(&c), &period("all", "prev", None)).unwrap());
    assert_eq!(
        r["coverage"]["no_compare_reason"],
        json!("All time has nothing before it to compare with.")
    );
    assert_eq!(r["data"]["tiles"]["plays"]["value"], json!(7.0));
    assert_eq!(r["data"]["timeline"].as_array().unwrap().len(), 3); // Aug, Sep, Oct
    let e = report(&cx(&c), &period("month", "prev", Some("2026-9"))).unwrap_err();
    assert_eq!(e.code, Code::Validation);
}

#[test]
fn trends_and_records_use_all_device_dates() {
    let c = history();
    let t = j(trends(&cx(&c), Measure::Plays).unwrap());
    assert_eq!(
        t["data"]["months"],
        json!([
            {"month": "2026-08", "value": 2.0, "coverage": "all_devices"},
            {"month": "2026-09", "value": 2.0, "coverage": "all_devices"},
            // 1–3 Oct all devices, 10 and 12 this PC, the other 14 days no data
            {"month": "2026-10", "value": 3.0, "coverage": "no_data"}
        ])
    );
    assert_eq!(t["coverage"]["desktop_only_since"], json!("2026-10-04"));
    let cal = &t["data"]["calendar"][0];
    assert_eq!(cal["year"], json!(2026));
    let m = cal["minutes"].as_array().unwrap();
    assert_eq!(m.len(), 365);
    assert_eq!(
        (&m[0], &m[221], &m[282]),
        (&json!(null), &json!(3), &json!(null))
    ); // 1 Jan, 10 Aug, 10 Oct
    assert_eq!(
        cal["biggest_day"],
        json!({"date": "2026-09-02", "ms": 300_000})
    );
    assert_eq!(
        cal["longest_streak"],
        json!({"days": 2, "from": "2026-08-10", "to": "2026-08-11"})
    );
    // 55 all-device dates (10 Aug – 3 Oct), 5 with a play (L2 on 20 Sep counts: every source)
    assert_eq!(cal["days_without_music"], json!(50));
    assert_eq!(t["data"]["fingerprint_by_year"][0]["year"], json!(2026));
    // one-month labels (Aster, Birch, Cedar + Dahl) are all under 3 months: they merge into one era
    // named after the middle month's lead
    assert_eq!(
        t["data"]["eras"],
        json!([{"from": "2026-08", "to": "2026-10", "artists": ["Birch"]}])
    );

    let r = j(records(&cx(&c)).unwrap());
    let d = &r["data"];
    assert_eq!(r["sources"], json!(["S", "E"]));
    assert_eq!(
        (&d["since"], &d["plays_total"]),
        (&json!("2026-08-10"), &json!(7))
    );
    assert_eq!(
        d["first_play"],
        json!({"track": "One", "artist": "Aster", "at_local": "2026-08-10T10:00"})
    );
    assert_eq!(
        d["longest_streak"],
        json!({"days": 2, "from": "2026-08-10", "to": "2026-08-11"})
    );
    assert_eq!(
        d["biggest_day"],
        json!({"date": "2026-09-02", "ms": 300_000})
    );
    assert_eq!(d["biggest_year"], json!({"year": 2026, "ms": 792_000}));
    assert_eq!(
        d["most_artists_month"],
        json!({"month": "2026-10", "artists": 3})
    );
    // 7 plays in the last 90 days: (5 000 − 7) × 90 / 7 = 64 195.7 → 64 196
    assert_eq!(
        d["next_milestone"],
        json!({"plays": 5000, "eta_days": 64_196})
    );
    assert_eq!(d["top_artist"]["name"], json!("Aster")); // 360 000 ms, ties Cedar, first by name
    assert_eq!(d["on_this_day"], json!([]));
}

#[test]
fn discovery_and_habits_for_september() {
    let c = history();
    let q = period("month", "prev", Some("2026-09"));
    let d = j(discovery(&cx(&c), &q).unwrap());
    assert_eq!(d["sources"], json!(["S", "E"]));
    let d = &d["data"];
    assert_eq!(
        d["new_artists"],
        json!({"value": 2.0, "now": 1.0, "before": 1.0})
    );
    assert_eq!(d["days_per_new"], json!(15.0)); // 30 covered days / 2
    assert_eq!(
        d["stick_rate"],
        json!({"pct": null, "best_pct": null, "best_year": null})
    ); // windows still open
    assert_eq!(
        (&d["artists_known"], &d["known_since"]),
        (&json!(3), &json!("2026-08-10"))
    );
    assert_eq!(
        d["starts"],
        json!([{"kind": "followed", "pct": 75.0}, {"kind": "picked", "pct": 25.0}])
    );
    assert_eq!(
        d["log"],
        json!([
            {"artist": "Cedar", "item_id": null, "first_played_local": "2026-09-20T20:00", "first_track": "Four", "plays_since": 2, "now": "in_rotation"},
            {"artist": "Birch", "item_id": d["log"][1]["item_id"], "first_played_local": "2026-09-02T10:00", "first_track": "Three", "plays_since": 1, "now": "in_rotation"}
        ])
    );
    let sep = d["per_month"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["month"] == "2026-09")
        .unwrap();
    assert_eq!(
        sep,
        &json!({"month": "2026-09", "stuck": 0, "dropped": 0, "open": 2, "coverage": "all_devices"})
    );

    let h = j(habits(&cx(&c), &q).unwrap());
    let h = &h["data"];
    assert_eq!(h["export_plays"], json!(1));
    assert_eq!(
        h["ends"],
        json!([{"kind": "finished", "pct": 25.0}, {"kind": "skipped", "pct": 75.0}])
    );
    assert_eq!(h["skip_rate"], json!(75.0)); // 3 of 4 flagged; desktop rows have no flag (G-45)
    assert_eq!(
        (&h["skips_by_hour"][22], &h["skips_by_hour"][10]),
        (&json!(100.0), &json!(0.0))
    );
    assert_eq!(
        h["quick_bails"],
        json!([{"track": "Three", "artist": "Birch", "item_id": null, "times": 3}])
    );
    assert_eq!(
        h["devices"],
        json!([{"kind": "computer", "pct": 50.0}, {"kind": "this_pc", "pct": 50.0}])
    );
    assert_eq!(
        h["finished"],
        json!({"pct": 25.0, "how_far_pct": null, "how_far_coverage_pct": 0.0})
    );
    assert_eq!(h["shuffle"]["pct"], json!(null));
    // two sessions (S1, L2), 5 and 2 min long: buckets [5, 15) and [0, 5)
    assert_eq!(
        h["sessions"],
        json!({"count": 2, "typical_ms": 210_000, "longest": {"ms": 300_000, "start_local": "2026-09-02T09:55"}, "spread": [1, 1, 0, 0, 0, 0, 0, 0, 0, 0]})
    );
}

#[test]
fn explore_series_and_validation() {
    let c = history();
    let q = |v: Value| -> ExploreQuery { serde_json::from_value(v).unwrap() };
    let base = json!({"series": [{"kind": "artist", "name": "aster"}, {"kind": "track", "name": "Nobody"}],
                      "from": "2026-08-01", "to": "2026-10-31", "grain": "month", "measure": "plays"});
    let r = j(explore(&cx(&c), &q(base.clone())).unwrap());
    let d = &r["data"];
    assert_eq!(d["buckets"], json!(["2026-08", "2026-09", "2026-10"]));
    assert_eq!(
        d["series"][0],
        json!({"kind": "artist", "name": "aster", "by": "", "values": [2.0, 0.0, 1.0], "total": 3.0,
               "peak": {"bucket": "2026-08", "value": 2.0}, "first_played_local": "2026-08-10T10:00"})
    );
    // a name with no plays is a line of zeros, not an error
    assert_eq!(
        (&d["series"][1]["values"], &d["series"][1]["peak"]),
        (&json!([0.0, 0.0, 0.0]), &json!(null))
    );
    assert_eq!(d["export_only"], json!(false));
    // a track or album without `by` matches any artist, case-folded, and reports who it is by
    let mut v = base.clone();
    v["series"] = json!([{"kind": "track", "name": "THREE"}, {"kind": "album", "name": "cedar lp"},
                         {"kind": "track", "name": "four", "by": "cedar"}]);
    let s = &j(explore(&cx(&c), &q(v)).unwrap())["data"]["series"];
    assert_eq!(
        (&s[0]["values"], &s[0]["by"]),
        (&json!([0.0, 1.0, 0.0]), &json!("Birch"))
    );
    assert_eq!(
        (&s[1]["values"], &s[1]["by"]),
        (&json!([0.0, 1.0, 1.0]), &json!("Cedar"))
    );
    assert_eq!(
        (&s[2]["values"], &s[2]["by"]),
        (&json!([0.0, 1.0, 1.0]), &json!("cedar"))
    );
    // share of each month's time: Aug 100%, Oct 60 000 of 360 000
    let mut v = base.clone();
    v["measure"] = json!("share");
    assert_eq!(
        j(explore(&cx(&c), &q(v)).unwrap())["data"]["series"][0]["values"],
        json!([100.0, 0.0, 16.7])
    );
    // with an hour filter the share is of that hour's time: Oct at 10:00 is only S3
    let mut v = base.clone();
    v["measure"] = json!("share");
    v["hours"] = json!([10]);
    assert_eq!(
        j(explore(&cx(&c), &q(v)).unwrap())["data"]["series"][0]["values"],
        json!([100.0, 0.0, 100.0])
    );
    // a device filter is export only: up to the export's end, desktop plays left out
    let mut v = base.clone();
    v["series"] = json!([{"kind": "artist", "name": "Cedar"}]);
    v["devices"] = json!(["computer"]);
    let r = j(explore(&cx(&c), &q(v)).unwrap());
    assert_eq!(
        (&r["data"]["export_only"], &r["data"]["series"][0]["total"]),
        (&json!(true), &json!(0.0))
    );
    // weeks start on Monday
    let mut v = base.clone();
    v["grain"] = json!("week");
    v["from"] = json!("2026-08-12");
    v["to"] = json!("2026-08-20");
    assert_eq!(
        j(explore(&cx(&c), &q(v)).unwrap())["data"]["buckets"],
        json!(["2026-08-10", "2026-08-17"])
    );
    for bad in [
        json!({"series": []}),
        json!({"series": [{"kind": "artist", "name": "a"}, {"kind": "artist", "name": "b"}, {"kind": "artist", "name": "c"}, {"kind": "artist", "name": "d"}]}),
        json!({"series": [{"kind": "artist", "name": "  "}]}),
        json!({"from": "2026-11-01"}),
        json!({"hours": [24]}),
        json!({"weekdays": [0]}),
    ] {
        let mut v = base.clone();
        for (k, x) in bad.as_object().unwrap() {
            v[k] = x.clone();
        }
        assert_eq!(
            explore(&cx(&c), &q(v.clone())).unwrap_err().code,
            Code::Validation,
            "{v}"
        );
    }
}

#[test]
fn explore_views_validate_and_round_trip() {
    let c = history();
    let q = |v: Value| -> ExploreQuery { serde_json::from_value(v).unwrap() };
    let good = json!({"series": [{"kind": "album", "name": " Aster  LP ", "by": "Aster"}],
                      "from": "2026-08-01", "to": "2026-10-31", "grain": "week", "measure": "hours", "hours": [21]});
    let v = save_explore_view(&c, "  Late  Aster ", &q(good.clone())).unwrap();
    assert_eq!(v.name, "Late Aster");
    // the builder only, names cleaned, unset options left out (the UI's `?` fields)
    let stored: String = c
        .query_row("SELECT query_json FROM explore_view", [], |r| r.get(0))
        .unwrap();
    let stored: Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(
        stored,
        json!({"series": [{"kind": "album", "name": "Aster LP", "by": "Aster"}],
               "from": "2026-08-01", "to": "2026-10-31", "grain": "week", "measure": "hours", "hours": [21]})
    );
    assert_eq!(
        j(list_explore_views(&c).unwrap()),
        json!([{"id": v.id, "name": "Late Aster", "query": stored}])
    );
    // validated like `explore`; the name like `save_filter`
    let mut bad = good.clone();
    bad["hours"] = json!([24]);
    assert_eq!(
        save_explore_view(&c, "x", &q(bad)).unwrap_err().code,
        Code::Validation
    );
    assert_eq!(
        save_explore_view(&c, "   ", &q(good)).unwrap_err().code,
        Code::Validation
    );
    delete_explore_view(&c, v.id).unwrap();
    assert_eq!(j(list_explore_views(&c).unwrap()), json!([]));
}

#[test]
fn data_coverage_per_source() {
    let c = history();
    let r = j(data_coverage(&cx(&c)).unwrap());
    let d = &r["data"];
    assert_eq!(
        d["export"],
        json!({"from": "2026-08-10", "to": "2026-10-03", "plays": 4})
    );
    assert_eq!(
        d["desktop"],
        json!({"from": "2026-09-02", "to": "2026-10-12", "plays": 4, "matched": 1})
    );
    assert_eq!(
        (
            &d["logger_runs_since"],
            &d["imports"],
            &d["private_excluded"]
        ),
        (&json!(null), &json!([]), &json!(0))
    );
    // no data after the export: 4–9 Oct, 11 Oct, 13–19 Oct
    assert_eq!(
        r["coverage"]["gaps"],
        json!([{"from": "2026-10-04", "to": "2026-10-09"}, {"from": "2026-10-11", "to": "2026-10-11"}, {"from": "2026-10-13", "to": "2026-10-19"}])
    );
}

#[test]
fn all_time_tags_from_the_rollup_match_the_event_count() {
    let c = history();
    c.execute_batch(
        "INSERT INTO item(id, type, display_name, status) VALUES (900, 'track', 'Three', 'planned');
         INSERT INTO tag(id, name) VALUES (1, 'night');
         INSERT INTO item_tag(item_id, tag_id) VALUES (900, 1);
         UPDATE listening_event SET item_id = 900 WHERE track_name = 'Three';",
    )
    .unwrap();
    let all = j(report(&cx(&c), &period("all", "prev", None)).unwrap());
    let mut q = period("custom", "prev", None);
    q.from = Some("2026-08-10".into());
    q.to = Some("2026-10-19".into());
    let custom = j(report(&cx(&c), &q).unwrap());
    // Birch's Three: 312 000 of 1 092 000 ms is tagged
    assert_eq!(
        all["data"]["tags"],
        json!({"rows": [{"tag": "night", "share_pct": 100.0}], "covered_pct": 28.6})
    );
    assert_eq!(all["data"]["tags"], custom["data"]["tags"]);
    // all time's totals come from the rollups: the same as counting every event
    for k in ["ms", "plays", "artists", "days_listened", "finished_pct"] {
        assert_eq!(
            all["data"]["tiles"][k]["value"], custom["data"]["tiles"][k]["value"],
            "{k}"
        );
    }
    for k in ["tracks", "albums", "albums_per_artist"] {
        assert_eq!(
            all["data"]["ratio"][k]["value"], custom["data"]["ratio"][k]["value"],
            "{k}"
        );
    }
}

#[test]
fn start_to_finish_counts_sessions_with_every_listed_track() {
    let c = history();
    // album 901 lists tracks 900 (Three, played in S1's session) and 902 (Four, played in L2's)
    c.execute_batch(
        "INSERT INTO item(id, type, display_name, status) VALUES (900, 'track', 'Three', 'planned'),
           (902, 'track', 'Four', 'planned'), (901, 'album', 'Mixed LP', 'planned'), (903, 'album', 'Solo LP', 'planned');
         INSERT INTO item_link(from_id, to_id, kind) VALUES (900, 901, 'on_album'), (902, 901, 'on_album'), (900, 903, 'on_album');
         UPDATE listening_event SET item_id = 900 WHERE track_name = 'Three';
         UPDATE listening_event SET item_id = 902 WHERE track_name = 'Four';",
    )
    .unwrap();
    let h = j(habits(&cx(&c), &period("month", "prev", Some("2026-09"))).unwrap());
    // Mixed LP's two tracks never share a session; Solo LP's one track is heard in S1's
    assert_eq!(
        h["data"]["start_to_finish"],
        json!([{"item_id": 903, "name": "Solo LP", "by": "", "times": 1}])
    );
}

#[test]
fn an_artist_leaving_the_top_10_after_six_months_gets_a_line() {
    let c = history();
    for m in 3..=8 {
        put(
            &c,
            Source::Export,
            &format!("2026-{m:02}-01T09:00:00"),
            60_000,
            "Elm",
            "Six",
            Some("trackdone"),
            None,
        );
    }
    let r = j(report(&cx(&c), &period("month", "prev", Some("2026-09"))).unwrap());
    assert!(
        texts(&r["data"]["lines"])
            .contains(&"Elm left your top 10 after 6 months in it.".to_owned())
    );
    // five months is under the threshold
    c.execute(
        // unfiltered: fixture edit
        "DELETE FROM listening_event WHERE artist_name = 'Elm' AND ended_at_utc < '2026-04-01'",
        [],
    )
    .unwrap();
    let r = j(report(&cx(&c), &period("month", "prev", Some("2026-09"))).unwrap());
    assert!(
        !texts(&r["data"]["lines"])
            .iter()
            .any(|l| l.contains("left your top 10"))
    );
}

#[test]
fn repeat_counts_plays_of_tracks_heard_before_the_window() {
    let c = history();
    let d = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    let months = [
        (d("2026-08-01"), d("2026-08-31")),
        (d("2026-09-01"), d("2026-09-30")),
        (d("2026-10-01"), d("2026-10-31")),
    ];
    // October: S3 (One, heard in August) and L3 (Four, heard in September); Dahl's Five is new
    let all = super::v3::Src::All;
    assert_eq!(
        super::report::repeat_plays(&cx(&c), all, &months).unwrap(),
        vec![0, 0, 2]
    );
    assert_eq!(
        super::report::repeat_plays(&cx(&c), all, &months[2..]).unwrap(),
        vec![2]
    );
}

/// A library file reads stretches of two months or more on threads (`v3::scan_parts`, `par_rows`,
/// NFR-04): every figure is the same as one pass in memory.
#[test]
fn threads_give_the_same_figures() {
    let mem = history();
    let dir = std::env::temp_dir().join(format!("runout-v3-threads-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("library.db");
    let _ = std::fs::remove_file(&path);
    mem.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    let file = db::open(&path).unwrap();
    for (g, vs, at) in [
        ("week", "prev", None),
        ("month", "prev", None),
        ("month", "usual", None),
        ("month", "last_year", Some("2026-09")),
        ("year", "prev", None),
        ("all", "prev", None),
    ] {
        let p = period(g, vs, at);
        assert_eq!(
            j(report(&cx(&mem), &p).unwrap()),
            j(report(&cx(&file), &p).unwrap()),
            "report {g} {vs}"
        );
        assert_eq!(
            j(discovery(&cx(&mem), &p).unwrap()),
            j(discovery(&cx(&file), &p).unwrap()),
            "discovery {g} {vs}"
        );
        assert_eq!(
            j(habits(&cx(&mem), &p).unwrap()),
            j(habits(&cx(&file), &p).unwrap()),
            "habits {g} {vs}"
        );
    }
    for m in [Measure::Hours, Measure::Artists] {
        assert_eq!(
            j(trends(&cx(&mem), m).unwrap()),
            j(trends(&cx(&file), m).unwrap())
        );
    }
    drop(file);
    let _ = std::fs::remove_dir_all(&dir);
}

/// On repeat (≥ 5 plays of a track in a day) in How you listen: named after its track, read back
/// from the first play (`habits` keeps no track name per play, NFR-04).
#[test]
fn habits_names_a_repeat() {
    let c = db::open_in_memory().unwrap();
    for i in 0..5 {
        put(
            &c,
            Source::Logger,
            &format!("2026-10-12T1{i}:00:00"),
            60_000,
            "Elm",
            "Six",
            None,
            None,
        );
    }
    let h = j(habits(&cx(&c), &period("month", "prev", None)).unwrap());
    assert_eq!(
        h["data"]["deep_dives"],
        json!([{"kind": "repeat", "name": "Six", "by": "Elm", "start_local": "2026-10-12T09:59",
                "end_local": "2026-10-12T14:00", "tracks": 1, "ms": 300_000, "times": 5}])
    );
}
