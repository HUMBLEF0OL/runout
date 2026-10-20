//! M5 WI 3 manual acceptance (Playlist Studio round trip) on a TEST playlist the user made for it.
//! Never point it at a real playlist. Signs in through the browser on every run and keeps those
//! tokens in memory only: it never touches the app's `Runout/spotify` credential, so its token
//! refresh can't rotate the refresh token out from under a running app (check.py). Needs
//! RUNOUT_SPOTIFY_CLIENT_ID set; uses a throwaway cache DB in the temp folder.
//!
//! `cargo run --example studio_roundtrip -- spotify:playlist:<22 chars> [--conflict | --probe]`
//!
//! Loads the draft, cuts 3 tracks and 1 duplicate, prints the diff, applies, waits for Enter, undoes.
//! `--conflict`: pauses after loading the draft so you edit the playlist in Spotify, then expects
//! apply to be blocked with `conflict` (nothing written).
//! `--probe`: moves the last track to the front, prints what the snapshot_id reads and
//! `studio::current_uris` (the whole list, compared with the moved order) say every 3 s for 60 s,
//! then restores the order (is the read Studio decides on fresh? G-30).
use std::sync::Mutex;

use reqwest::Method;
use runout_lib::error::{AppError, Code};
use runout_lib::spotify::{self, Spotify, client, studio};
use serde_json::{Value, json};

fn wait_enter() {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
}

/// Deletes the throwaway cache folder (it holds the playlist's undo snapshot) on every exit,
/// panics included; a failure is reported, not swallowed. Declared before `sp`, so it drops after
/// it: the SQLite connection must close first or Windows keeps the file.
struct RemoveOnExit(std::path::PathBuf);

impl Drop for RemoveOnExit {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.0)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!(
                "Couldn't delete {}: {e}. Delete it by hand.",
                self.0.display()
            );
        }
    }
}

/// Replace the playlist with `uris` (PUT the first 100, POST the rest). Returns the last write's
/// snapshot_id. Used to restore the test playlist when a write or undo fails, and by --probe.
async fn put_all(sp: &Spotify, pl: &str, uris: &[String]) -> Result<String, AppError> {
    let path = format!(
        "/playlists/{}/items",
        pl.rsplit(':').next().unwrap_or_default()
    );
    let mut last = String::new();
    for (i, c) in uris.chunks(100).enumerate() {
        let m = if i == 0 { Method::PUT } else { Method::POST };
        let r = sp.send_json(m, &path, Some(json!({ "uris": c }))).await?;
        last = r["snapshot_id"].as_str().unwrap_or("?").to_string();
    }
    Ok(last)
}

/// Put the playlist back as loaded from this program's own copy (the undo snapshot is deleted
/// with the cache folder on exit), and say whether that worked.
async fn restore(sp: &Spotify, pl: &str, orig: &[String]) {
    println!("Restoring the test playlist from this program's own copy…");
    match put_all(sp, pl, orig).await {
        Ok(s) => println!("restored; write-response snapshot_id {s}"),
        Err(e) => println!("RESTORE FAILED: {e:?}. Fix the test playlist by hand."),
    }
}

/// What each way of reading the playlist says right now (--probe). The track list is read with
/// `studio::current_uris`, the one read apply and undo decide on, so a PASS here covers it (G-30).
async fn probe_row(sp: &Spotify, pl: &str, orig: &[String], moved: &[String]) -> String {
    let id = pl.rsplit(':').next().unwrap_or_default();
    let get = |path: String| async move {
        match sp.get_json::<Value>(&path).await {
            Ok(v) => v,
            Err(e) => json!({ "err": format!("{:?}", e.code) }),
        }
    };
    let snap = |v: &Value| {
        v["snapshot_id"]
            .as_str()
            .map(|s| s[..12.min(s.len())].to_string())
            .unwrap_or_else(|| v["err"].to_string())
    };
    let fields = get(format!("/playlists/{id}?fields=snapshot_id")).await;
    let full = get(format!("/playlists/{id}")).await;
    let market = get(format!(
        "/playlists/{id}?fields=snapshot_id&market=from_token"
    ))
    .await;
    let items = match studio::current_uris(sp, pl).await {
        Ok(l) if l == moved => "MOVED (fresh)".to_string(),
        Ok(l) if l == orig => "orig (stale)".to_string(),
        Ok(l) => format!("other ({} tracks)", l.len()),
        Err(e) => format!("err {:?}", e.code),
    };
    let mine: Vec<Value> = sp
        .paginate("/me/playlists?limit=50", 500)
        .await
        .unwrap_or_default();
    let listed = mine
        .iter()
        .find(|p| p["id"] == id)
        .map(snap)
        .unwrap_or("-".into());
    format!(
        "fields={} full={} market={} me/playlists={} current_uris={}",
        snap(&fields),
        snap(&full),
        snap(&market),
        listed,
        items
    )
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    let conflict = std::env::args().any(|a| a == "--conflict");
    let probe = std::env::args().any(|a| a == "--probe");
    let pl = spotify::check_uri(&arg, &["playlist"]).expect("pass a spotify:playlist:… URI");
    let dir = std::env::temp_dir().join("runout-studio-roundtrip");
    let _cleanup = RemoveOnExit(dir.clone());
    let sp = Spotify::with(
        None, // memory-only tokens, never the app's credential
        None,
        dir.join(runout_lib::cleanup::CACHE_DB),
        client::net_transport(),
    );
    tauri::async_runtime::block_on(sp.connect()).expect("Spotify sign-in in the browser");
    // Empty library: ratings/tags aren't needed for the round trip.
    let db = Mutex::new(runout_lib::db::open_in_memory().unwrap());

    tauri::async_runtime::block_on(async {
        let draft = studio::load_studio_draft(&sp, &db, &pl).await.unwrap();
        println!(
            "loaded {} tracks, snapshot {}",
            draft.tracks.len(),
            draft.snapshot_id
        );
        let orig: Vec<String> = draft.tracks.iter().map(|t| t.uri.clone()).collect();
        if probe {
            // Real but reversible change: move the last track to the front, watch how fast each
            // read path shows it (snapshot ids + Studio's own track-list read), then put the
            // original order back.
            let mut moved = orig.clone();
            moved.rotate_right(1);
            let short = |u: &str| u.rsplit(':').next().unwrap_or_default()[..6].to_string();
            println!(
                "original first track {}, moved-in first track {}",
                short(&orig[0]),
                short(&moved[0])
            );
            println!("before:  {}", probe_row(&sp, &pl, &orig, &moved).await);
            match put_all(&sp, &pl, &moved).await {
                Ok(s) => println!("moved last track to the front; write-response snapshot_id {s}"),
                Err(e) => {
                    println!("FAIL: the move failed partway: {e:?}");
                    return restore(&sp, &pl, &orig).await;
                }
            }
            for t in 0..21 {
                println!(
                    "+{:>2} s:  {}",
                    t * 3,
                    probe_row(&sp, &pl, &orig, &moved).await
                );
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
            return restore(&sp, &pl, &orig).await;
        }
        let dupe = draft.tracks.iter().position(|t| t.dupe_of.is_some());
        if dupe.is_none() {
            println!("no duplicate in this playlist: add one copy of a track to test dedupe");
        }
        // Never cut either copy of the duplicated song: dedupe removes exactly one.
        let twin = dupe.map(|d| (draft.tracks[d].uri.clone(), draft.tracks[d].dupe_of.clone()));
        let cut: Vec<usize> = (0..draft.tracks.len())
            .filter(|i| {
                let u = &draft.tracks[*i].uri;
                twin.as_ref()
                    .is_none_or(|(a, b)| u != a && Some(u) != b.as_ref())
            })
            .take(3)
            .chain(dupe)
            .collect();
        for i in &cut {
            let t = &draft.tracks[*i];
            let why = if t.dupe_of.is_some() {
                "duplicate"
            } else {
                "cut"
            };
            println!("  {why}: {} — {}", t.name, t.artist);
        }
        let keep: Vec<String> = draft
            .tracks
            .iter()
            .enumerate()
            .filter(|(i, _)| !cut.contains(i))
            .map(|(_, t)| t.uri.clone())
            .collect();
        let diff = studio::preview_studio_apply(&sp, &draft, &keep)
            .await
            .unwrap();
        println!("diff: {diff:?}");
        if conflict {
            println!(
                "Now change the playlist in Spotify (remove or move any track), then press Enter."
            );
            wait_enter();
            // Spotify's app pushes an edit to the server a moment later; apply can only see what
            // the Web API sees. Show it, and wait (up to 60 s) until the API reports the edit.
            for t in 0..=20 {
                let now = studio::current_uris(&sp, &pl).await.unwrap();
                if now != orig {
                    println!(
                        "API sees your edit after ~{} s ({} → {} tracks).",
                        t * 3,
                        orig.len(),
                        now.len()
                    );
                    break;
                }
                if t == 20 {
                    println!(
                        "API still shows the old list after 60 s: was the edit made on this playlist?"
                    );
                    return;
                }
                if t == 0 {
                    println!("API doesn't show your edit yet; re-checking every 3 s…");
                }
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
            match studio::apply_studio_draft(&sp, &draft, &keep).await {
                Err(e) if e.code == Code::Conflict => {
                    println!("PASS: apply blocked with conflict; nothing was written.")
                }
                Err(e) => println!("FAIL: apply failed with another error: {e:?}"),
                Ok(_) => {
                    println!("FAIL: apply went through.");
                    restore(&sp, &pl, &orig).await;
                }
            }
            return;
        }
        let applied = match studio::apply_studio_draft(&sp, &draft, &keep).await {
            Ok(a) => a,
            Err(e) => {
                println!("FAIL: apply failed: {e:?}");
                return restore(&sp, &pl, &orig).await;
            }
        };
        println!(
            "applied: undo_id {} (expires {}). Check the playlist in Spotify, then press Enter to undo.",
            applied.undo_id, applied.expires_at
        );
        // Diagnostics for undo's conflict check: the track lists it will accept (expected_json).
        let expected: Option<String> =
            rusqlite::Connection::open(dir.join(runout_lib::cleanup::CACHE_DB))
                .and_then(|c| {
                    c.query_row(
                        "SELECT expected_json FROM playlist_snapshot ORDER BY id DESC LIMIT 1",
                        [],
                        |r| r.get(0),
                    )
                })
                .ok();
        let lens: Vec<usize> = expected
            .and_then(|j| serde_json::from_str::<Vec<Vec<String>>>(&j).ok())
            .unwrap_or_default()
            .iter()
            .map(Vec::len)
            .collect();
        println!(
            "undo accepts track lists of length {lens:?} (expected {}).",
            keep.len()
        );
        wait_enter();
        if let Err(e) = studio::undo_last_apply(&sp, &pl).await {
            println!("FAIL: undo refused: {e:?}");
            restore(&sp, &pl, &orig).await;
        }
        let after = studio::load_studio_draft(&sp, &db, &pl).await.unwrap();
        let same = after
            .tracks
            .iter()
            .map(|t| &t.uri)
            .eq(draft.tracks.iter().map(|t| &t.uri));
        println!(
            "undone: {} tracks, identical to the original: {same}",
            after.tracks.len()
        );
    });
    drop(sp); // closes the cache before `_cleanup` deletes the folder
}
