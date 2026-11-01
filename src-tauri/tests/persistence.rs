//! M3 acceptance "All data persists across restart": write every kind of user data, close the
//! file, reopen it, and read back exactly what was there. Synthetic values only.
mod common;

use runout_lib::db;
use runout_lib::library::filters::{self, ItemFilter};
use runout_lib::library::items::{self, ItemPatch, NewItem};
use runout_lib::library::{Status, backlog, dashboard, search, tags};
use serde_json::{Value, json, to_value};

fn new(v: Value) -> NewItem {
    serde_json::from_value(v).unwrap()
}

/// Everything the UI reads back, as JSON for whole-value comparison.
fn snapshot(c: &rusqlite::Connection, ids: &[i64]) -> Value {
    json!({
        "items": ids.iter().map(|&id| to_value(items::get(c, id).unwrap()).unwrap()).collect::<Vec<_>>(),
        "filters": to_value(filters::list_saved_filters(c).unwrap()).unwrap(),
        "tags": to_value(tags::list_tags(c).unwrap()).unwrap(),
        "counts": to_value(dashboard::get_counts(c).unwrap()).unwrap(),
        "search": to_value(search::search(c, "zephyr", None, None).unwrap()).unwrap(),
        "search_notes": to_value(search::search(c, "quillwort", None, None).unwrap()).unwrap(),
    })
}

#[test]
fn all_data_persists_across_restart() {
    let dir = common::temp_dir("persist");
    let path = dir.join("library.db");
    let mut c = db::open(&path).unwrap();

    let album = items::create(
        &mut c,
        new(json!({
            "type": "album", "display_name": "Zephyr Sessions", "provenance": "user",
            "spotify_uri": "spotify:album:AAAAAAAAAAAAAAAAAAAAAA",
            "tags": ["Ambient", "late night"], "priority": "high", "in_backlog": true,
            "recommended_by": "a friend", "artist_name": "The Synthetic Band",
        })),
    )
    .unwrap()
    .summary
    .id;
    let track = items::create(
        &mut c,
        new(json!({
            "type": "track", "display_name": "Zephyr Theme", "provenance": "promoted",
            "promoted_fields": ["display_name"], "in_backlog": true,
            "tags": ["ambient"], "artist_name": "The Synthetic Band", "album_id": album,
            "notes": "quillwort intro",
        })),
    )
    .unwrap()
    .summary
    .id;
    let patch: ItemPatch = serde_json::from_value(json!({
        "rating": 4.5, "review": "Slow build, worth it.", "notes": "zephyr notes",
        "favourite": true, "revisit": true,
    }))
    .unwrap();
    items::update(&mut c, album, patch).unwrap();
    runout_lib::library::status::set_status(&mut c, album, Status::InProgress, Some("started"))
        .unwrap();
    backlog::reorder_backlog(&mut c, &[track, album]).unwrap();
    let def: ItemFilter =
        serde_json::from_value(json!({"tags": ["ambient"], "favourite": true})).unwrap();
    filters::save_filter(&c, "Favourite ambient", def).unwrap();

    let before = snapshot(&c, &[album, track]);
    // sanity: the snapshot really holds what was written
    let a = &before["items"][0];
    assert_eq!(a["rating"], 4.5);
    assert_eq!(a["favourite"], true);
    assert_eq!(a["revisit"], true);
    assert_eq!(a["review"], "Slow build, worth it.");
    assert_eq!(a["status"], "in_progress");
    assert!(!a["history"].as_array().unwrap().is_empty());
    assert!(!a["track_list"].as_array().unwrap().is_empty());
    let t = &before["items"][1];
    assert_eq!(t["album_name"], "Zephyr Sessions");
    assert_eq!(t["artist_names"][0], "The Synthetic Band");
    assert_eq!(t["provenance"]["display_name"], "promoted");
    assert!(t["backlog_order"].as_i64() < a["backlog_order"].as_i64());
    assert_eq!(before["filters"].as_array().unwrap().len(), 1);
    assert!(!before["search"].as_array().unwrap().is_empty());
    assert!(!before["search_notes"].as_array().unwrap().is_empty());

    drop(c); // "quit"
    let c = db::open(&path).unwrap(); // "restart"
    assert_eq!(snapshot(&c, &[album, track]), before);
    drop(c);
}
