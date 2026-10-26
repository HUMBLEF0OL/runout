//! NFR-04 for Release 2 (M4 WI 9, TSD §8) on the reference set of 500k events: 300k logger-style
//! rows inserted directly (time order, as the logger appends), then a 10-year synthetic export
//! (TSD §4.8 layout, random values, 200k rows) imported on top. Asserts import < 60 s, filter
//! results (`list_plays`, `list_unmatched`) < 200 ms and screen loads (insights, year review,
//! record panel, freshness) < 500 ms ("screen navigation"). M10 WI 5 (design §6): every Insights v3
//! command < 500 ms per header choice, `explore` < 200 ms, lifetime months < 100 ms. Keep the DB with
//! `RUNOUT_PERF_KEEP=1`. Run: `cargo test --release -- --ignored perf_listening --nocapture`.
mod common;

use std::time::{Duration, Instant};

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use rand::RngExt;
use runout_lib::db;
use runout_lib::importer::events::{Event, Source, insert};
use runout_lib::importer::run;
use runout_lib::insights::explore::{ExploreQuery, data_coverage, explore};
use runout_lib::insights::habits::{discovery, habits};
use runout_lib::insights::history::{Measure, history_model, lifetime_months, records, trends};
use runout_lib::insights::plays::{PlayQuery, Tab, item_listening, list_plays, list_unmatched};
use runout_lib::insights::report::report;
use runout_lib::insights::v3::{Ctx, PeriodQuery, Src, daily};
use runout_lib::insights::{Range, collection::collection, freshness, listening, take::take};
use rusqlite::Connection;

const EVENTS: usize = 300_000;
const EXPORT_YEARS: i32 = 10;
const EXPORT_PER_YEAR: usize = 20_000; // a heavy listener: ~55 plays a day
const RUNS: usize = 5;

/// Slowest of `RUNS` runs (or `RUNOUT_PERF_RUNS`) after one warm-up.
fn worst<T>(mut f: impl FnMut() -> T) -> Duration {
    // RUNOUT_PERF_RUNS=0: one cold run, for a quick look
    let runs = std::env::var("RUNOUT_PERF_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(RUNS);
    if runs > 0 {
        f();
    }
    let d = (0..runs.max(1))
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .max()
        .unwrap();
    eprintln!("  {d:?}");
    d
}

struct Track {
    uri: String,
    name: String,
    artist: String,
    album: String,
}

/// 70% the library's own tracks (so plays link), 30% made-up ones (so `list_unmatched` has work).
fn tracks(conn: &Connection, g: &mut common::Gen) -> Vec<Track> {
    let mut st = conn
        .prepare(
            "SELECT t.spotify_uri, t.display_name, a.display_name FROM item t
             JOIN item_link l ON l.from_id = t.id AND l.kind = 'by_artist'
             JOIN item a ON a.id = l.to_id
             WHERE t.type = 'track' AND t.spotify_uri LIKE 'spotify:track:%'
               AND t.display_name IS NOT NULL AND a.display_name IS NOT NULL",
        )
        .unwrap();
    let mut out: Vec<Track> = st
        .query_map([], |r| {
            Ok(Track {
                uri: r.get(0)?,
                name: r.get(1)?,
                artist: r.get(2)?,
                album: String::new(),
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for _ in 0..out.len() * 3 / 7 {
        out.push(Track {
            uri: format!("spotify:track:{}", g.id()),
            name: g.words(2),
            artist: g.words(1),
            album: String::new(),
        });
    }
    for t in &mut out {
        t.album = g.words(2);
    }
    out
}

fn utc(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[test]
#[ignore]
fn perf_listening() {
    let dir = common::temp_dir("perf-listening");
    let mut conn = db::open(&dir.join("library.db")).unwrap();
    common::library(&mut conn, 10_000);
    let mut g = common::Gen::default();
    let pool = tracks(&conn, &mut g);
    let mut rng = rand::rng();
    let start = Utc.with_ymd_and_hms(2016, 1, 1, 0, 0, 0).unwrap();
    let span = 10 * 365 * 86_400;

    // 1. 500k events straight into the table: logger and export, some private, uri-linked.
    let item_of = {
        let mut st = conn
            .prepare("SELECT spotify_uri, id FROM item WHERE spotify_uri IS NOT NULL")
            .unwrap();
        st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect::<std::collections::HashMap<_, _>>()
    };
    let mut times: Vec<i64> = (0..EVENTS).map(|_| rng.random_range(0..span)).collect();
    times.sort_unstable();
    let t = Instant::now();
    let tx = conn.transaction().unwrap();
    for s in times {
        let tr = &pool[rng.random_range(0..pool.len())];
        let at = start + TimeDelta::seconds(s);
        let logger = rng.random_range(0..4) == 0;
        insert(
            &tx,
            &Event {
                source: if logger {
                    Source::Logger
                } else {
                    Source::Export
                },
                ended_at_utc: utc(at),
                ended_at_local: Some(at.format("%Y-%m-%dT%H:%M:%S").to_string()),
                ms_played: rng.random_range(0..300_000),
                track_name: tr.name.clone(),
                artist_name: tr.artist.clone(),
                album_name: tr.album.clone(),
                spotify_track_uri: (!logger).then(|| tr.uri.clone()),
                platform: Some("windows".into()),
                reason_start: Some("trackdone".into()),
                reason_end: Some("trackdone".into()),
                shuffle: Some(rng.random_range(0..2) == 0),
                duration_ms: None,
                skipped: Some(false),
                offline: Some(false),
                private_session: rng.random_range(0..50) == 0,
                item_id: item_of.get(&tr.uri).copied(),
                import_batch_id: None,
            },
        )
        .unwrap();
    }
    tx.commit().unwrap();
    println!("insert {EVENTS} events: {:?}", t.elapsed());

    // 2. A 10-year export in the TSD §4.8 layout (random values), imported on top.
    let export = dir
        .join("my_spotify_data")
        .join("Spotify Extended Streaming History");
    std::fs::create_dir_all(&export).unwrap();
    let mut rows = 0;
    for y in 0..EXPORT_YEARS {
        let year = 2016 + y;
        let from = Utc.with_ymd_and_hms(year, 1, 1, 0, 0, 0).unwrap();
        let mut file: Vec<serde_json::Value> = (0..EXPORT_PER_YEAR)
            .map(|_| {
                let tr = &pool[rng.random_range(0..pool.len())];
                let at = from + TimeDelta::seconds(rng.random_range(0..365 * 86_400));
                serde_json::json!({
                    "ts": utc(at), "platform": "windows", "ms_played": rng.random_range(0..300_000),
                    "conn_country": "ZZ", "ip_addr": "198.51.100.1",
                    "master_metadata_track_name": tr.name,
                    "master_metadata_album_artist_name": tr.artist,
                    "master_metadata_album_album_name": tr.album,
                    "spotify_track_uri": tr.uri,
                    "episode_name": null, "episode_show_name": null, "spotify_episode_uri": null,
                    "audiobook_title": null, "audiobook_uri": null,
                    "audiobook_chapter_uri": null, "audiobook_chapter_title": null,
                    "reason_start": "trackdone", "reason_end": "trackdone",
                    "shuffle": false, "skipped": false, "offline": false,
                    "offline_timestamp": null, "incognito_mode": false
                })
            })
            .collect();
        file.sort_by(|a, b| a["ts"].as_str().cmp(&b["ts"].as_str()));
        // Real files hold ~15k rows each, split by year.
        for (i, chunk) in file.chunks(15_000).enumerate() {
            let name = format!("Streaming_History_Audio_{year}_{i}.json");
            std::fs::write(export.join(name), serde_json::to_vec(chunk).unwrap()).unwrap();
            rows += chunk.len();
        }
    }
    let t = Instant::now();
    let loaded = run::load(&export).unwrap();
    let r = run::run(&mut conn, &loaded).unwrap();
    let import = t.elapsed();
    println!(
        "import {rows} rows / {EXPORT_YEARS} years: {import:?} (added {}, reconciled {})",
        r.added, r.reconciled
    );
    assert!(import < Duration::from_secs(60), "import took {import:?}");
    let total: i64 = conn
        .query_row("SELECT count(*) FROM listening_event", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, (EVENTS + rows) as i64);

    let slow = reads(&conn);
    drop(conn);
    if std::env::var_os("RUNOUT_PERF_KEEP").is_some() {
        println!("kept {}", dir.display());
    } else {
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert!(slow.is_empty(), "over target: {slow:?}");
}

/// The reads alone on a kept reference DB (`RUNOUT_PERF_KEEP=1` prints its folder):
/// `RUNOUT_PERF_DB=<folder>\library.db cargo test --release -- --ignored perf_reads --nocapture`.
#[test]
#[ignore]
fn perf_reads() {
    let Some(p) = std::env::var_os("RUNOUT_PERF_DB") else {
        return;
    };
    let conn = db::open(std::path::Path::new(&p)).unwrap();
    let slow = reads(&conn);
    assert!(slow.is_empty(), "over target: {slow:?}");
}

/// Timed reads; `RUNOUT_PERF_ONLY=<text>` runs only the names containing it.
struct Checks {
    only: Option<String>,
    v: Vec<(String, Duration)>,
}

impl Checks {
    fn run<T>(&mut self, name: impl Into<String>, f: impl FnMut() -> T) {
        let name = name.into();
        if self.only.as_ref().is_none_or(|o| name.contains(o.as_str())) {
            self.v.push((name, worst(f)));
        }
    }
}

/// Every read with its target; returns the names over target.
fn reads(conn: &Connection) -> Vec<String> {
    // 3. Every API.md §4 read < 200 ms over the full table.
    let now = Utc.with_ymd_and_hms(2025, 12, 31, 12, 0, 0).unwrap();
    let busiest: i64 = conn
        .query_row(
            "SELECT item_id FROM listening_event WHERE item_id IS NOT NULL
             GROUP BY item_id ORDER BY count(*) DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let ranges = [Range::Week, Range::Month, Range::Year, Range::All];
    let mut checks = Checks {
        only: std::env::var("RUNOUT_PERF_ONLY").ok(),
        v: Vec::new(),
    };
    for range in ranges {
        checks.run(format!("listening {range:?}"), || {
            listening::listening(conn, range, false, now).unwrap()
        });
        checks.run(format!("collection {range:?}"), || {
            collection(conn, range, now).unwrap()
        });
    }
    checks.run("take", || take(conn, false, now).unwrap());
    checks.run("year_review 2024", || {
        listening::year_review(conn, 2024, false).unwrap()
    });
    for tab in [Tab::All, Tab::Unrated, Tab::Missing] {
        for range in [Range::Month, Range::All] {
            let q = PlayQuery {
                tab,
                range,
                offset: 0,
                limit: 50,
                date: None,
            };
            checks.run(format!("list_plays {tab:?} {range:?}"), || {
                list_plays(conn, &q, false, now).unwrap()
            });
        }
    }
    checks.run("list_unmatched", || list_unmatched(conn, false).unwrap());
    checks.run("get_item_listening (busiest)", || {
        item_listening(conn, busiest, false, now).unwrap()
    });
    let artist: i64 = conn
        .query_row(
            "SELECT to_id FROM item_link WHERE from_id = ?1 AND kind = 'by_artist'",
            [busiest],
            |r| r.get(0),
        )
        .unwrap();
    checks.run("get_item_listening (its artist)", || {
        item_listening(conn, artist, false, now).unwrap()
    });
    checks.run("get_freshness", || freshness(conn).unwrap());

    // 4. Insights v3 (M10 WI 5): each header choice of the period tabs, the whole-history tabs,
    // Explore (filter result: < 200 ms) and lifetime months (< 100 ms).
    let cx = Ctx {
        conn,
        include_private: false,
        tz: &Utc,
        now,
    };
    let q = |v: serde_json::Value| -> PeriodQuery { serde_json::from_value(v).unwrap() };
    let periods = [
        (
            "week prev",
            q(serde_json::json!({"grain": "week", "vs": "prev", "at": null})),
        ),
        (
            "week usual",
            q(serde_json::json!({"grain": "week", "vs": "usual", "at": null})),
        ),
        (
            "month prev",
            q(serde_json::json!({"grain": "month", "vs": "prev", "at": null})),
        ),
        (
            "month last_year",
            q(serde_json::json!({"grain": "month", "vs": "last_year", "at": "2025-06"})),
        ),
        (
            "month usual",
            q(serde_json::json!({"grain": "month", "vs": "usual", "at": null})),
        ),
        (
            "year prev",
            q(serde_json::json!({"grain": "year", "vs": "prev", "at": null})),
        ),
        (
            "year usual",
            q(serde_json::json!({"grain": "year", "vs": "usual", "at": null})),
        ),
        (
            "all",
            q(serde_json::json!({"grain": "all", "vs": "prev", "at": null})),
        ),
        (
            "custom 90 d",
            q(
                serde_json::json!({"grain": "custom", "vs": "prev", "at": null, "from": "2025-09-01", "to": "2025-11-29"}),
            ),
        ),
    ];
    for (name, p) in &periods {
        checks.run(format!("v3 period_report {name}"), || {
            report(&cx, p).unwrap()
        });
        checks.run(format!("v3 discovery {name}"), || {
            discovery(&cx, p).unwrap()
        });
        checks.run(format!("v3 habits {name}"), || habits(&cx, p).unwrap());
    }
    for m in [Measure::Hours, Measure::Artists] {
        checks.run(format!("v3 trends {m:?}"), || trends(&cx, m).unwrap());
    }
    checks.run("v3 records", || records(&cx).unwrap());
    checks.run("v3 data_coverage", || data_coverage(&cx).unwrap());
    let today = now.date_naive();
    let first = runout_lib::insights::coverage::first_day(conn)
        .unwrap()
        .unwrap();
    checks.run("v3 lifetime months", || {
        let model = history_model(&cx, first, today).unwrap();
        let days = daily(&cx, Src::All, (first, today)).unwrap();
        lifetime_months(&cx, Measure::Artists, &days, &model, first, today).unwrap()
    });
    let names: Vec<(String, String)> = conn
        .prepare("SELECT artist_name, track_name FROM listening_event WHERE counts_as_play GROUP BY lower(artist_name) ORDER BY count(*) DESC LIMIT 2")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let explores = [
        (
            "artist 10 y month hours",
            serde_json::json!({"series": [{"kind": "artist", "name": names[0].0}],
            "from": "2016-01-01", "to": "2025-12-31", "grain": "month", "measure": "hours"}),
        ),
        (
            "3 series 10 y week share",
            serde_json::json!({"series": [{"kind": "artist", "name": names[0].0},
            {"kind": "track", "name": names[1].1, "by": names[1].0}, {"kind": "album", "name": "no such album"}],
            "from": "2016-01-01", "to": "2025-12-31", "grain": "week", "measure": "share"}),
        ),
        (
            "filtered 10 y year plays",
            serde_json::json!({"series": [{"kind": "artist", "name": names[1].0}],
            "from": "2016-01-01", "to": "2025-12-31", "grain": "year", "measure": "plays",
            "hours": [20, 21, 22, 23], "weekdays": [6, 7], "devices": ["computer"], "shuffle": true}),
        ),
    ];
    for (name, v) in explores {
        let eq: ExploreQuery = serde_json::from_value(v).unwrap();
        checks.run(format!("list_ explore {name}"), || {
            explore(&cx, &eq).unwrap()
        });
    }

    let mut slow = Vec::new();
    for (name, d) in &checks.v {
        let filter = name.starts_with("list_");
        let limit = Duration::from_millis(if filter {
            200
        } else if name == "v3 lifetime months" {
            100
        } else {
            500
        });
        println!("{name}: worst {d:?} (target {limit:?})");
        if *d >= limit {
            slow.push(name.clone());
        }
    }
    slow
}
