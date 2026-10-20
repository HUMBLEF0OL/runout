//! M5 WI 3 manual acceptance (Playlist Studio round trip) on a TEST playlist the user made for it.
//! Never point it at a real playlist. Signs in through the browser on every run and keeps those
//! tokens in memory only: it never touches the app's `Runout/spotify` credential, so its token
//! refresh can't rotate the refresh token out from under a running app (check.py). Needs
//! RUNOUT_SPOTIFY_CLIENT_ID set; uses a throwaway cache DB in the temp folder.
//!
//! `cargo run --example studio_roundtrip -- spotify:playlist:<22 chars> [--conflict]`
//!
//! Loads the draft, cuts 3 tracks and 1 duplicate, prints the diff, applies, waits for Enter, undoes.
//! `--conflict`: pauses after loading the draft so you edit the playlist in Spotify, then expects
//! apply to be blocked with `conflict` (nothing written).
use std::sync::Mutex;

use runout_lib::error::Code;
use runout_lib::spotify::{self, Spotify, client, studio};

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

async fn snap_now(sp: &Spotify, pl: &str) -> String {
    let id = pl.rsplit(':').next().unwrap_or_default();
    let v: serde_json::Value = sp
        .get_json(&format!("/playlists/{id}?fields=snapshot_id"))
        .await
        .unwrap_or_default();
    v["snapshot_id"].as_str().unwrap_or("?").to_string()
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    let conflict = std::env::args().any(|a| a == "--conflict");
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
            match studio::apply_studio_draft(&sp, &draft, &keep).await {
                Err(e) if e.code == Code::Conflict => {
                    println!("PASS: apply blocked with conflict; nothing was written.")
                }
                Err(e) => println!("FAIL: apply failed with another error: {e:?}"),
                Ok(_) => {
                    println!("FAIL: apply went through; undoing it.");
                    let undo = studio::undo_last_apply(&sp, &pl).await;
                    println!("undo: {undo:?}");
                }
            }
            return;
        }
        let applied = studio::apply_studio_draft(&sp, &draft, &keep)
            .await
            .unwrap();
        println!(
            "applied: undo_id {} (expires {}). Check the playlist in Spotify, then press Enter to undo.",
            applied.undo_id, applied.expires_at
        );
        // Diagnostics for the undo conflict check: Spotify's GET snapshot_id after our write.
        let recorded: Option<String> =
            rusqlite::Connection::open(dir.join(runout_lib::cleanup::CACHE_DB))
                .and_then(|c| {
                    c.query_row(
                    "SELECT applied_snapshot_id FROM playlist_snapshot ORDER BY id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                })
                .ok()
                .flatten();
        println!("write-response snapshot_id (recorded): {recorded:?}");
        println!("GET snapshot_id now: {}", snap_now(&sp, &pl).await);
        std::thread::sleep(std::time::Duration::from_secs(3));
        println!("GET snapshot_id +3 s: {}", snap_now(&sp, &pl).await);
        wait_enter();
        println!("GET snapshot_id before undo: {}", snap_now(&sp, &pl).await);
        studio::undo_last_apply(&sp, &pl).await.unwrap();
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
