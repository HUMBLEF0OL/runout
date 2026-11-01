//! M4 acceptance on a real export, against a throwaway DB (TSD §4.8, FR-030):
//! `cargo run --release --example real_import -- <export.zip|folder> <temp library.db>`
//! Prints counts only, plus 20 matched plays for a manual spot-check. Paths come from argv;
//! nothing real is written into the repo.
use std::net::IpAddr;
use std::path::Path;

use runout_lib::{db, importer::run};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (export, lib) = (Path::new(&a[1]), Path::new(&a[2]));
    let mut c = db::open(lib).expect("open temp library");

    let first = run::run(&mut c, &run::load(export).unwrap()).unwrap();
    println!(
        "first import: added {} duplicates {} reconciled {} took {} ms",
        first.added, first.duplicates, first.reconciled, first.took_ms
    );
    let second = run::run(&mut c, &run::load(export).unwrap()).unwrap();
    println!("second import: added {}", second.added);

    // No IP / user agent / username anywhere: no such column, and no TEXT value that is an IP.
    let mut leaks = 0;
    let tables: Vec<String> = c
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE '%fts%'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for t in &tables {
        let cols: Vec<String> = c
            .prepare(&format!("SELECT name FROM pragma_table_info('{t}')"))
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        for col in cols {
            let l = col.to_lowercase();
            if l.starts_with("ip_addr") || l.starts_with("user_agent") || l == "username" {
                leaks += 1;
                println!("forbidden column {t}.{col}");
            }
            let mut st = c
                .prepare(&format!(
                    "SELECT \"{col}\" FROM \"{t}\" WHERE typeof(\"{col}\") = 'text'"
                ))
                .unwrap();
            let ips = st
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .filter(|v| v.as_ref().is_ok_and(|v| v.trim().parse::<IpAddr>().is_ok()))
                .count();
            if ips > 0 {
                leaks += ips;
                println!("{ips} IP-like values in {t}.{col}");
            }
        }
    }
    println!("forbidden-field check: {leaks} hits");

    let (events, matched): (i64, i64) = c
        .query_row(
            "SELECT count(*), count(item_id) FROM listening_event WHERE source = 'export'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    println!("export events {events}, matched to a record {matched}");
    let mut st = c
        .prepare(
            "SELECT e.track_name, e.artist_name, i.display_name, i.spotify_uri = e.spotify_track_uri
             FROM listening_event e JOIN item i ON i.id = e.item_id
             GROUP BY e.item_id ORDER BY random() LIMIT 20",
        )
        .unwrap();
    println!("20 matched plays (event track — artist  →  record [same URI?]):");
    for r in st
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<bool>>(3)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
    {
        println!("  {:?} — {:?}  →  {:?} [{:?}]", r.0, r.1, r.2, r.3);
    }
}
