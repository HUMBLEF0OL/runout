//! NFR-04 on the 10k reference library (TSD §8). Run: `cargo test --release -- --ignored perf`.
//! Numbers are printed (use `--nocapture`) and recorded in docs/kb/perf-integrity.md.
mod common;

use std::time::{Duration, Instant};

use runout_lib::library::filters::{ItemFilter, Sort, list_items};
use runout_lib::library::{ItemType, Status, dashboard, search::search};
use runout_lib::{backup, db};

const N: usize = 10_000;
const RUNS: usize = 5;

/// Slowest of `RUNS` runs after one warm-up (the app's queries also run on a warm connection).
fn worst<T>(mut f: impl FnMut() -> T) -> (Duration, T) {
    f();
    let mut max = Duration::ZERO;
    let mut out = None;
    for _ in 0..RUNS {
        let t = Instant::now();
        let v = f();
        max = max.max(t.elapsed());
        out = Some(v);
    }
    (max, out.unwrap())
}

#[test]
#[ignore]
fn perf_10k() {
    let dir = common::temp_dir("perf");
    let path = dir.join("library.db");
    let mut conn = db::open(&path).unwrap();
    let t = Instant::now();
    let special = common::library(&mut conn, N);
    let (tags, links): (i64, i64) = conn
        .query_row(
            "SELECT (SELECT count(*) FROM item_tag), (SELECT count(*) FROM item_link)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    println!(
        "generate: {N} items, {tags} item_tags, {links} item_links in {:?}",
        t.elapsed()
    );

    // Search < 200 ms, incl. a word that appears only in one review.
    let tag7: String = conn
        .query_row("SELECT name FROM tag WHERE id = 7", [], |r| r.get(0))
        .unwrap();
    let common_word = tag7.split(' ').next().unwrap().to_string();
    for (q, kind) in [
        (common::REVIEW_ONLY_WORD, None),
        (common_word.as_str(), None),
        ("ba", None),
        ("ko ri", Some(ItemType::Track)),
        ("mel", Some(ItemType::Album)),
    ] {
        let (d, hits) = worst(|| search(&conn, q, None, kind).unwrap());
        println!("search {q:?} {kind:?}: {} hits, worst {d:?}", hits.len());
        assert!(d < Duration::from_millis(200), "search {q:?} took {d:?}");
        if q == common::REVIEW_ONLY_WORD {
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].item.id, special);
            assert_eq!(hits[0].field, "review");
        }
    }

    // List page (50 rows) < 100 ms over a few filters/sorts.
    let filters = [
        ("default", ItemFilter::default()),
        (
            "track planned by name",
            ItemFilter {
                r#type: Some(ItemType::Track),
                status: Some(vec![Status::Planned]),
                sort: Some(Sort::Name),
                ..Default::default()
            },
        ),
        (
            "tag + min_rating by rating",
            ItemFilter {
                tags: Some(vec![tag7]),
                min_rating: Some(3.0),
                sort: Some(Sort::Rating),
                ..Default::default()
            },
        ),
        (
            "backlog order, page 20",
            ItemFilter {
                in_backlog: Some(true),
                sort: Some(Sort::BacklogOrder),
                offset: Some(1000),
                ..Default::default()
            },
        ),
        (
            "modified desc",
            ItemFilter {
                sort: Some(Sort::ModifiedAt),
                ..Default::default()
            },
        ),
    ];
    for (name, f) in filters {
        let (d, page) = worst(|| list_items(&conn, &f).unwrap());
        println!(
            "list {name}: {} rows of {}, worst {d:?}",
            page.rows.len(),
            page.total
        );
        assert!(d < Duration::from_millis(100), "list {name} took {d:?}");
    }
    drop(conn);

    // App-start equivalent: open + migrate (no-op) + counts + dashboard on a cold connection.
    let t = Instant::now();
    let conn = db::open(&path).unwrap();
    let counts = dashboard::get_counts(&conn).unwrap();
    let dash = dashboard::get_dashboard(&conn).unwrap();
    let start = t.elapsed();
    println!(
        "start: open+migrate+counts+dashboard {start:?} ({} items, {} in progress)",
        counts.library,
        dash.in_progress.len()
    );
    assert_eq!(counts.library, N as i64);
    assert!(start < Duration::from_millis(500), "start took {start:?}");

    // Backup size vs the NFR-11 estimate (10–30 MB).
    let db_size = std::fs::metadata(&path).unwrap().len();
    let t = Instant::now();
    let b = backup::backup_now(&conn, &dir.join("backups"), 50).unwrap();
    println!(
        "backup: library.db {:.1} MB → zip {:.1} MB in {:?}",
        db_size as f64 / 1e6,
        b.size_bytes as f64 / 1e6,
        t.elapsed()
    );
    drop(conn);
    let _ = std::fs::remove_dir_all(&dir);
}
