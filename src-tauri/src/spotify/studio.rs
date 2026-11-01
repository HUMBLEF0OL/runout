//! Playlist Studio (M5 WI 3, TSD §5.2): own/collab playlists, draft with `item` ratings/tags only
//! (D27), diff preview, apply with a track-content conflict check and a 30-day undo snapshot in
//! the cache DB (never `snapshot_id`: GET serves a stale one after a write, TSD §5.2). The command layer has already checked the connection and the URIs.
use std::collections::HashMap;
use std::sync::Mutex;

use reqwest::Method;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Spotify, cache, uri_id};
use crate::error::{AppError, Code, Result};

/// Spotify takes at most 100 URIs per add/replace call.
const CHUNK: usize = 100;
/// Ceiling on playlist size read into a draft. A longer playlist is refused, never truncated: an
/// apply would delete everything past the ceiling and the undo snapshot couldn't bring it back.
const MAX_ITEMS: usize = 10_000;
/// How long after a write with unknown outcome undo won't trust a read showing the list before it
/// (the server may still be processing it, TSD §5.2). Requests time out after 20 s (`net.rs`).
/// ponytail: fixed bound, not proof; a write Spotify finishes later than this can still land.
const UNSURE_WAIT: std::time::Duration = std::time::Duration::from_secs(120);

/// API.md `list_spotify_playlists` row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpotifyPlaylist {
    pub uri: String,
    pub name: String,
    pub tracks: i64,
    pub owned: bool,
    pub collaborative: bool,
}

/// API.md `Draft` (the UI sends it back to preview/apply).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Draft {
    pub playlist_uri: String,
    pub snapshot_id: String,
    pub tracks: Vec<DraftTrack>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftTrack {
    pub uri: String,
    pub name: String,
    pub artist: String,
    pub rating: Option<f64>,
    pub tags: Vec<String>,
    pub dupe_of: Option<String>,
}

/// API.md `preview_studio_apply` result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StudioDiff {
    pub removed: i64,
    pub reordered: i64,
    pub added: i64,
}

/// API.md `apply_studio_draft` result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Applied {
    pub undo_id: i64,
    pub expires_at: String,
}

/// API.md `get_last_apply` record: `cache::Snapshot` (`{undo_id, applied_at, expires_at}`).
pub use super::cache::Snapshot as ApplyRecord;

/// Diff of `orig` → `keep` (both URI lists, playlist order), as multisets: `common` = Σ min(copies
/// in each), `removed` = orig − common, `added` = keep − common, `reordered` = common − LCS(orig,
/// keep): the fewest kept entries that must move. Removing an earlier copy of a duplicate is not a
/// move (live, 20 Oct 2026: a removal-only edit showed 7). LCS by Hunt–Szymanski: each keep entry
/// becomes its URI's orig positions in descending order; the longest strictly increasing run of
/// that sequence is the LCS (O((n + matches) log n), fine for 10,000 entries without many copies).
pub fn diff(orig: &[String], keep: &[String]) -> StudioDiff {
    let mut pos: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, u) in orig.iter().enumerate() {
        pos.entry(u).or_default().push(i);
    }
    let mut used: HashMap<&str, usize> = HashMap::new();
    let mut common = 0;
    // tails[k] = smallest orig position ending an increasing run of length k + 1.
    let mut tails: Vec<usize> = Vec::new();
    for u in keep {
        let Some(ps) = pos.get(u.as_str()) else {
            continue;
        };
        let n = used.entry(u).or_default();
        if *n < ps.len() {
            *n += 1;
            common += 1;
        }
        for &p in ps.iter().rev() {
            let k = tails.partition_point(|&t| t < p);
            if k == tails.len() {
                tails.push(p);
            } else {
                tails[k] = p;
            }
        }
    }
    StudioDiff {
        removed: (orig.len() - common) as i64,
        reordered: (common - tails.len()) as i64,
        added: (keep.len() - common) as i64,
    }
}

/// `dupe_of` for each row: the URI of the first earlier row with the same URI, or the same
/// name + artist (case-insensitive; the same song under another release).
pub fn dupes(tracks: &[(String, String, String)]) -> Vec<Option<String>> {
    let mut seen: HashMap<String, String> = HashMap::new();
    tracks
        .iter()
        .map(|(uri, name, artist)| {
            let by_name = format!("{}\u{1f}{}", name.to_lowercase(), artist.to_lowercase());
            let hit = seen.get(uri).or_else(|| seen.get(&by_name)).cloned();
            seen.entry(uri.clone()).or_insert_with(|| uri.clone());
            seen.entry(by_name).or_insert_with(|| uri.clone());
            hit
        })
        .collect()
}

/// Playlist objects carry `items` (Feb 2026 Dev Mode naming) or the older `tracks`; entries carry
/// `item` or `track`. Read either.
pub(crate) fn either<'a>(v: &'a Value, new: &str, old: &str) -> &'a Value {
    if v[new].is_null() { &v[old] } else { &v[new] }
}

/// Current `snapshot_id` of a playlist (the draft's, informational: nothing is decided on it).
async fn snapshot_id(sp: &Spotify, playlist_uri: &str) -> Result<String> {
    let v: Value = sp
        .get_json(&format!(
            "/playlists/{}?fields=snapshot_id",
            uri_id(playlist_uri)
        ))
        .await?;
    v["snapshot_id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| AppError::internal("Spotify sent a playlist without a snapshot id."))
}

/// Every entry as `(uri, name, artist)`. Local files and episodes can't be put back by URI, so a
/// playlist holding them is refused rather than silently emptied of them on apply.
async fn items(sp: &Spotify, playlist_uri: &str) -> Result<Vec<(String, String, String)>> {
    let raw: Vec<Value> = sp
        .paginate(
            // documented max 50 (kb/spotify.md)
            &format!("/playlists/{}/items?limit=50", uri_id(playlist_uri)),
            MAX_ITEMS + 1,
        )
        .await?;
    if raw.len() > MAX_ITEMS {
        return Err(AppError::validation(
            "Studio can edit playlists of up to 10,000 tracks; this one is longer.",
        ));
    }
    raw.iter()
        .map(|e| {
            let t = either(e, "item", "track");
            let uri = t["uri"].as_str().unwrap_or_default();
            if e["is_local"] == true || super::check_uri(uri, &["track"]).is_err() {
                return Err(AppError::validation(
                    "Studio can only edit playlists of Spotify tracks; this one has local files or episodes.",
                ));
            }
            let artist = t["artists"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|a| a["name"].as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Ok((
                uri.to_string(),
                t["name"].as_str().unwrap_or_default().to_string(),
                artist,
            ))
        })
        .collect()
}

/// The playlist's current track URIs, in order. The one read every Studio check compares against
/// (`GET /playlists/{id}/items`), so the read path can be swapped here alone (`studio_roundtrip
/// --probe` checks this same function).
pub async fn current_uris(sp: &Spotify, playlist_uri: &str) -> Result<Vec<String>> {
    Ok(items(sp, playlist_uri)
        .await?
        .into_iter()
        .map(|t| t.0)
        .collect())
}

/// Replace the playlist's items (`from` now) with `uris`: PUT replaces with the first chunk, POST
/// appends the rest. Every write that lands sets `landed` and records the list the playlist now
/// holds (the cumulative prefix) as snapshot `undo_id`'s expected list at once, so after a write
/// fails partway undo (or a retried undo) compares against the app's own last write. Only a 4xx
/// refusal proves a failed write didn't land; any other failure (timeout, reset, 5xx, unreadable
/// 2xx) may land even after an immediate re-read showed the old list, so `landed` is set and both
/// the lists before and after it are expected (no re-read decides it, TSD §5.2).
/// Callers hold `sp.studio_lock` so no other apply/undo's chunks land in between (check.py).
async fn replace(
    sp: &Spotify,
    playlist_uri: &str,
    from: &[String],
    uris: &[String],
    undo_id: i64,
    landed: &mut bool,
) -> Result<()> {
    let path = format!("/playlists/{}/items", uri_id(playlist_uri));
    let first = uris.chunks(CHUNK).next().unwrap_or_default();
    let rest = uris.chunks(CHUNK).skip(1).map(|c| (Method::POST, c));
    for (i, (method, c)) in std::iter::once((Method::PUT, first))
        .chain(rest)
        .enumerate()
    {
        let before = if i == 0 { from } else { &uris[..i * CHUNK] };
        let after = &uris[..i * CHUNK + c.len()];
        let sent = sp
            .send_json(method, &path, Some(json!({ "uris": c })))
            .await;
        if let Err(e) = sent {
            if !super::client::refused(&e) {
                *landed = true;
                let at = Some(chrono::Utc::now());
                let _ = sp
                    .with_cache(|db| cache::set_expected(db, undo_id, &json!([before, after]), at));
            }
            // `spotify_busy` means nothing was written (API.md §5); after chunk 0 the PUT landed.
            if i > 0 && e.code == Code::SpotifyBusy {
                return Err(AppError::internal(
                    "Spotify got busy partway through, so the playlist was only partly changed. Undo puts it back.",
                )
                .with_detail("HTTP 429 after the first chunk"));
            }
            return Err(e);
        }
        *landed = true;
        sp.with_cache(|db| cache::set_expected(db, undo_id, &json!([after]), None))?;
    }
    Ok(())
}

pub async fn list_spotify_playlists(sp: &Spotify) -> Result<Vec<SpotifyPlaylist>> {
    let me: Value = sp.get_json("/me").await?;
    let me = me["id"].as_str().unwrap_or_default().to_string();
    let all: Vec<Value> = sp.paginate("/me/playlists?limit=50", MAX_ITEMS).await?;
    Ok(all
        .iter()
        .filter(|p| p.is_object())
        .filter_map(|p| {
            let owned = !me.is_empty() && p["owner"]["id"].as_str() == Some(me.as_str());
            let collaborative = p["collaborative"] == true;
            (owned || collaborative).then(|| SpotifyPlaylist {
                uri: p["uri"].as_str().unwrap_or_default().to_string(),
                name: p["name"].as_str().unwrap_or_default().to_string(),
                tracks: either(p, "items", "tracks")["total"].as_i64().unwrap_or(0),
                owned,
                collaborative,
            })
        })
        .collect())
}

/// API.md `find_cross_playlist_dupes` row: a track of the checked playlist and the user's other
/// own/collab playlists that also hold it (G-24).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrossDupe {
    pub uri: String,
    pub playlists: Vec<PlaylistRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlaylistRef {
    pub uri: String,
    pub name: String,
}

/// Spotify track URIs of a playlist, in order; local files and episodes are passed over (only
/// compared here, never written back).
async fn track_uris(sp: &Spotify, playlist_uri: &str) -> Result<Vec<String>> {
    let raw: Vec<Value> = sp
        .paginate(
            &format!("/playlists/{}/items?limit=50", uri_id(playlist_uri)),
            MAX_ITEMS,
        )
        .await?;
    Ok(raw
        .iter()
        .filter(|e| e["is_local"] != true)
        .filter_map(|e| either(e, "item", "track")["uri"].as_str())
        .filter(|u| super::check_uri(u, &["track"]).is_ok())
        .map(str::to_string)
        .collect())
}

/// `find_cross_playlist_dupes` (G-24, BRD R3-1): on the user's explicit "Check other playlists",
/// one items read per other own/collab playlist (429 waits via the client). Matches by URI. Rows
/// follow the checked playlist's order, once per URI, only tracks found elsewhere. A playlist that
/// went away meanwhile (404) is skipped. Reads playlists only, never listening data (D27).
pub async fn find_cross_playlist_dupes(sp: &Spotify, playlist_uri: &str) -> Result<Vec<CrossDupe>> {
    let mine = track_uris(sp, playlist_uri).await?;
    let mut out: Vec<CrossDupe> = vec![];
    let mut at: HashMap<String, usize> = HashMap::new();
    for u in &mine {
        if !at.contains_key(u) {
            at.insert(u.clone(), out.len());
            out.push(CrossDupe {
                uri: u.clone(),
                playlists: vec![],
            });
        }
    }
    for p in list_spotify_playlists(sp).await? {
        if p.uri == playlist_uri {
            continue;
        }
        let theirs = match track_uris(sp, &p.uri).await {
            Err(e) if e.code == Code::NotFound => continue,
            r => r?,
        };
        let mut seen = std::collections::HashSet::new();
        for u in theirs {
            if let Some(&i) = at.get(&u)
                && seen.insert(u)
            {
                out[i].playlists.push(PlaylistRef {
                    uri: p.uri.clone(),
                    name: p.name.clone(),
                });
            }
        }
    }
    out.retain(|d| !d.playlists.is_empty());
    Ok(out)
}

pub async fn load_studio_draft(
    sp: &Spotify,
    db: &Mutex<Connection>,
    playlist_uri: &str,
) -> Result<Draft> {
    let snapshot_id = snapshot_id(sp, playlist_uri).await?;
    let rows = items(sp, playlist_uri).await?;
    let dupe = dupes(&rows);
    let c = db.lock().unwrap();
    // D27: only the user's own rating and tags, joined by URI.
    let mut rating = c.prepare("SELECT id, rating FROM item WHERE spotify_uri = ?1")?;
    let mut tags = c.prepare(
        "SELECT t.name FROM item_tag it JOIN tag t ON t.id = it.tag_id WHERE it.item_id = ?1 ORDER BY t.name",
    )?;
    let mut tracks = Vec::with_capacity(rows.len());
    for ((uri, name, artist), dupe_of) in rows.into_iter().zip(dupe) {
        let hit: Option<(i64, Option<f64>)> = rating
            .query_row([&uri], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let tag_list = match hit {
            Some((id, _)) => tags
                .query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<String>>>()?,
            None => Vec::new(),
        };
        tracks.push(DraftTrack {
            uri,
            name,
            artist,
            rating: hit.and_then(|h| h.1),
            tags: tag_list,
            dupe_of,
        });
    }
    Ok(Draft {
        playlist_uri: playlist_uri.to_string(),
        snapshot_id,
        tracks,
    })
}

pub async fn preview_studio_apply(
    _sp: &Spotify,
    draft: &Draft,
    keep: &[String],
) -> Result<StudioDiff> {
    let orig: Vec<String> = draft.tracks.iter().map(|t| t.uri.clone()).collect();
    Ok(diff(&orig, keep))
}

/// Read the current items (the undo snapshot) and compare them with the draft's tracks (order and
/// multiplicity): different → `conflict`, nothing written. Never `snapshot_id`: a stale GET id
/// would hide an edit and a content-neutral bump would block (TSD §5.2; check.py keeps the order
/// read → compare → snapshot → write). Then the snapshot is stored first and the write runs. A
/// first write Spotify refused (4xx) changed nothing, so its snapshot is dropped (no undo is
/// offered); once anything (maybe) landed, it is the playlist's only undo and older ones are dropped.
pub async fn apply_studio_draft(sp: &Spotify, draft: &Draft, keep: &[String]) -> Result<Applied> {
    // G-43: Spotify's playlist ceiling; refused before any read or write.
    if keep.len() > MAX_ITEMS {
        return Err(AppError::validation(
            "Studio can apply up to 10,000 tracks; this draft keeps more.",
        ));
    }
    let _one = sp.studio_lock.lock().await;
    let current = current_uris(sp, &draft.playlist_uri).await?;
    if !current.iter().eq(draft.tracks.iter().map(|t| &t.uri)) {
        return Err(AppError::new(
            Code::Conflict,
            "This playlist changed on Spotify. Reload the draft.",
        ));
    }
    // The last apply's write may still land (unknown outcome < UNSURE_WAIT ago): it would cut this
    // apply's result. Wait; nothing written, the old snapshot (its only undo) is kept.
    let last =
        sp.with_cache(|c| cache::last_snapshot(c, &draft.playlist_uri, chrono::Utc::now()))?;
    if let Some((s, ..)) = last {
        let left = wait_left(sp.with_cache(|c| cache::unsure_since(c, s.undo_id))?);
        if let Some(left) = left {
            return Err(busy(left, "Apply"));
        }
    }
    let snap = sp.with_cache(|c| {
        cache::put_snapshot(
            c,
            &draft.playlist_uri,
            &draft.snapshot_id,
            &json!(current),
            chrono::Utc::now(),
        )
    })?;
    let mut landed = false;
    let wrote = replace(
        sp,
        &draft.playlist_uri,
        &current,
        keep,
        snap.undo_id,
        &mut landed,
    )
    .await;
    let tidy = sp.with_cache(|c| {
        if landed {
            cache::drop_older_snapshots(c, &draft.playlist_uri, snap.undo_id)
        } else {
            cache::delete_snapshot(c, snap.undo_id)
        }
    });
    wrote?;
    tidy?;
    Ok(Applied {
        undo_id: snap.undo_id,
        expires_at: snap.expires_at,
    })
}

/// Time left of UNSURE_WAIT since a write with unknown outcome at `unsure`, if any.
fn wait_left(unsure: Option<chrono::DateTime<chrono::Utc>>) -> Option<std::time::Duration> {
    let waited = (chrono::Utc::now() - unsure?).to_std().unwrap_or_default();
    UNSURE_WAIT.checked_sub(waited).filter(|l| !l.is_zero())
}

fn busy(left: std::time::Duration, what: &str) -> AppError {
    let mut e = AppError::new(
        Code::SpotifyBusy,
        format!(
            "Spotify may still be finishing the last change to this playlist. Try {what} again in a couple of minutes."
        ),
    );
    e.retry_after_s = Some(left.as_secs().max(1));
    e
}

pub async fn get_last_apply(sp: &Spotify, playlist_uri: &str) -> Result<Option<ApplyRecord>> {
    sp.with_cache(|c| Ok(cache::last_snapshot(c, playlist_uri, chrono::Utc::now())?.map(|s| s.0)))
}

/// Put the snapshot's items back, then drop the snapshot (a failed write keeps it for a retry,
/// which compares against the undo's own last write). Finding the list from before a write with
/// unknown outcome less than UNSURE_WAIT after it → `spotify_busy`, nothing written, kept. The playlist's tracks aren't what Runout
/// last wrote (edited on Spotify since) → `conflict`, nothing overwritten, and the snapshot is
/// dropped: the undo would erase that edit.
pub async fn undo_last_apply(sp: &Spotify, playlist_uri: &str) -> Result<()> {
    let _one = sp.studio_lock.lock().await;
    let (snap, expected, items) = sp
        .with_cache(|c| cache::last_snapshot(c, playlist_uri, chrono::Utc::now()))?
        .ok_or_else(|| AppError::not_found("There's no recent change to undo."))?;
    let expected: Vec<Vec<String>> = serde_json::from_value(expected)?;
    // A playlist that now holds local files/episodes or is over the ceiling was edited on Spotify
    // since: same as any other edit (conflict, dropped), not a `validation` that keeps a dead undo.
    let now = match current_uris(sp, playlist_uri).await {
        // Deleted or unfollowed (404): the undo can never go through, so it's dropped too.
        Err(e) if matches!(e.code, Code::Validation | Code::NotFound) => None,
        r => Some(r?),
    };
    let Some(now) = now.filter(|n| expected.contains(n)) else {
        sp.with_cache(|c| cache::delete_snapshot(c, snap.undo_id))?;
        return Err(AppError::new(
            Code::Conflict,
            "This playlist changed on Spotify since you applied, so undo would erase those changes. The undo was dropped.",
        ));
    };
    // The list from before a write with unknown outcome: that write may still land after this
    // read, and would cut the restored playlist with no undo left. Wait, keep the snapshot.
    let left = wait_left(sp.with_cache(|c| cache::unsure_since(c, snap.undo_id))?);
    if let Some(left) = left.filter(|_| expected.len() == 2 && now == expected[0]) {
        return Err(busy(left, "Undo"));
    }
    let uris: Vec<String> = serde_json::from_value(items)?;
    replace(sp, playlist_uri, &now, &uris, snap.undo_id, &mut false).await?;
    sp.with_cache(|c| cache::delete_snapshot(c, snap.undo_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spotify::client::{API, Body};
    use crate::spotify::testing::*;

    const PL: &str = "spotify:playlist:PPPPPPPPPPPPPPPPPPPPPP";

    fn t(n: u8) -> String {
        format!("spotify:track:{}", (n as char).to_string().repeat(22))
    }
    fn v(s: &str) -> Vec<String> {
        s.bytes().map(t).collect()
    }
    fn page(ts: &[(String, &str, &str)]) -> String {
        let items: Vec<Value> = ts
            .iter()
            .map(|(u, n, a)| {
                json!({"is_local": false, "item": {"uri": u, "name": n, "artists": [{"name": a}]}})
            })
            .collect();
        json!({"items": items, "next": null}).to_string()
    }
    /// A page holding exactly `uris`.
    fn pg(uris: &[String]) -> String {
        page(
            &uris
                .iter()
                .map(|u| (u.clone(), "n", "a"))
                .collect::<Vec<_>>(),
        )
    }
    fn draft(s: &str, snap: &str) -> Draft {
        draft_of(&v(s), snap)
    }
    fn draft_of(uris: &[String], snap: &str) -> Draft {
        Draft {
            playlist_uri: PL.into(),
            snapshot_id: snap.into(),
            tracks: uris
                .iter()
                .cloned()
                .map(|uri| DraftTrack {
                    uri,
                    name: "n".into(),
                    artist: "a".into(),
                    rating: None,
                    tags: vec![],
                    dupe_of: None,
                })
                .collect(),
        }
    }

    #[test]
    fn diff_table() {
        // (orig, keep, removed, reordered, added)
        let cases = [
            ("ABCD", "ABCD", 0, 0, 0),
            ("ABCD", "ABD", 1, 0, 0),
            ("ABCD", "", 4, 0, 0),
            ("ABCD", "BACD", 0, 1, 0), // a real swap: one entry moves
            ("ABCD", "DCB", 1, 2, 0),
            ("AABC", "ABC", 1, 0, 0), // one copy of a duplicate cut
            ("XAX", "AX", 1, 0, 0),   // the earlier copy cut, the later kept
            ("XAXBX", "AB", 3, 0, 0), // pure removals with duplicates
            ("XABXCX", "ABXC", 2, 0, 0),
            ("XAX", "XXA", 0, 1, 0),
            ("ABC", "ABCE", 0, 0, 1),
            ("ABC", "EABCA", 0, 0, 2),
            ("ABC", "EBA", 1, 1, 1),
            ("", "", 0, 0, 0),
        ];
        for (o, k, r, ro, a) in cases {
            let d = diff(&v(o), &v(k));
            assert_eq!((d.removed, d.reordered, d.added), (r, ro, a), "{o} -> {k}");
        }
    }

    #[test]
    fn dupes_point_at_the_first_copy() {
        let row = |u: &str, n: &str, a: &str| (t(u.as_bytes()[0]), n.to_string(), a.to_string());
        let rows = [
            row("A", "Song", "Band"),
            row("B", "Other", "Band"),
            row("A", "Song", "Band"),    // same URI
            row("C", "song", "BAND"),    // same song, another release
            row("D", "Song", "Someone"), // same title, other artist: not a dupe
        ];
        assert_eq!(
            dupes(&rows),
            [None, None, Some(t(b'A')), Some(t(b'A')), None]
        );
    }

    #[test]
    fn draft_joins_only_rating_and_tags_and_marks_dupes() {
        let d = temp_dir("studio-draft");
        let db = crate::db::open(&d.join("library.db")).unwrap();
        db.execute_batch(&format!(
            "INSERT INTO item(id, type, display_name, spotify_uri, rating, review)
               VALUES (1, 'track', 'Mine', '{}', 2.0, 'private words');
             INSERT INTO tag(id, name) VALUES (1, 'late night'), (2, 'jazz');
             INSERT INTO item_tag VALUES (1, 2), (1, 1);",
            t(b'A')
        ))
        .unwrap();
        let stub = Stub::new(&[
            (200, r#"{"snapshot_id":"s1"}"#),
            (
                200,
                &page(&[
                    (t(b'A'), "Song", "Band"),
                    (t(b'B'), "Other", "Band"),
                    (t(b'A'), "Song", "Band"),
                ]),
            ),
        ]);
        let sp = connected(&d, &stub);
        let dr = block(load_studio_draft(&sp, &Mutex::new(db), PL)).unwrap();
        assert_eq!(dr.snapshot_id, "s1");
        assert_eq!(dr.tracks.len(), 3);
        assert_eq!(dr.tracks[0].rating, Some(2.0));
        assert_eq!(dr.tracks[0].tags, ["jazz", "late night"]);
        assert_eq!((dr.tracks[1].rating, dr.tracks[1].tags.len()), (None, 0));
        assert_eq!(dr.tracks[2].dupe_of, Some(t(b'A')));
        assert!(!serde_json::to_string(&dr).unwrap().contains("private"));
        assert!(
            stub.calls()[1]
                .1
                .starts_with(&format!("{API}/playlists/PPPPPPPPPPPPPPPPPPPPPP/items"))
        );
    }

    #[test]
    fn local_files_are_refused() {
        let d = temp_dir("studio-local");
        let local = json!({"items": [{"is_local": true, "item": {"uri": "spotify:local:a:b:c:1", "name": "x", "artists": []}}], "next": null});
        let stub = Stub::new(&[(200, r#"{"snapshot_id":"s1"}"#), (200, &local.to_string())]);
        let sp = connected(&d, &stub);
        let db = Mutex::new(crate::db::open_in_memory().unwrap());
        let e = block(load_studio_draft(&sp, &db, PL)).unwrap_err();
        assert_eq!(e.code, Code::Validation);
    }

    /// G-43: keeping more than the 10,000-track ceiling is refused before anything reaches Spotify.
    #[test]
    fn keep_over_the_ceiling_is_refused() {
        let d = temp_dir("studio-ceiling");
        let stub = Stub::new(&[]);
        let sp = connected(&d, &stub);
        let keep = vec![t(b'a'); MAX_ITEMS + 1];
        let e = block(apply_studio_draft(&sp, &draft_of(&[t(b'a')], "s1"), &keep)).unwrap_err();
        assert_eq!(e.code, Code::Validation);
        assert!(stub.calls().is_empty(), "nothing read or written");
    }

    fn many(n: u8) -> Vec<String> {
        (0..n)
            .map(|i| format!("spotify:track:{:0>22}", i))
            .collect()
    }
    fn body_uris(b: &Option<Body>) -> Vec<String> {
        match b {
            Some(Body::Json(b)) => serde_json::from_value(b["uris"].clone()).unwrap(),
            _ => panic!("json body"),
        }
    }
    /// UNSURE_WAIT has passed since the last write with unknown outcome.
    fn settle(sp: &Spotify) {
        sp.with_cache(|c| {
            Ok(c.execute(
                "UPDATE playlist_snapshot SET unsure_at = '2000-01-01T00:00:00Z'",
                [],
            )?)
        })
        .unwrap();
    }
    fn no_snapshot_get(stub: &Stub) {
        assert!(
            stub.calls().iter().all(|c| !c.1.contains("snapshot_id")),
            "apply/undo must not decide on snapshot_id"
        );
    }

    /// Live, 20 Oct 2026: the PUT answered a new snapshot_id but GET kept serving the pre-write
    /// one for 30 s+. Apply and undo never read snapshot_id; the items equal Runout's write, so
    /// undo restores instead of a false `conflict`.
    #[test]
    fn stale_snapshot_id_after_the_write_does_not_block_undo() {
        let d = temp_dir("studio-stale-get");
        let stub = Stub::new(&[
            (200, &pg(&v("AB"))),
            (200, r#"{"snapshot_id":"AAAACb"}"#), // PUT answer: the new id
            (200, &pg(&v("B"))),                  // undo's read: holds exactly what Runout wrote
            (200, r#"{"snapshot_id":"AAAACc"}"#),
        ]);
        let sp = connected(&d, &stub);
        block(apply_studio_draft(&sp, &draft("AB", "AAAACJ"), &v("B"))).unwrap();
        block(undo_last_apply(&sp, PL)).unwrap();
        no_snapshot_get(&stub);
        let reqs = stub.requests.lock().unwrap();
        assert_eq!(reqs[3].method, Method::PUT);
        assert_eq!(body_uris(&reqs[3].body), v("AB"));
        drop(reqs);
        assert_eq!(block(get_last_apply(&sp, PL)).unwrap(), None);
    }

    /// Edited on Spotify after the apply (items differ from Runout's write): undo refuses
    /// instead of erasing the edit, and drops the snapshot.
    #[test]
    fn undo_after_a_later_edit_is_a_conflict() {
        let d = temp_dir("studio-undo-conflict");
        let stub = Stub::new(&[
            (200, &pg(&v("AB"))),
            (200, r#"{"snapshot_id":"s2"}"#),
            (200, &pg(&v("BC"))), // someone added C since
        ]);
        let sp = connected(&d, &stub);
        block(apply_studio_draft(&sp, &draft("AB", "s1"), &v("B"))).unwrap();
        let e = block(undo_last_apply(&sp, PL)).unwrap_err();
        assert_eq!(e.code, Code::Conflict);
        assert_eq!(stub.calls().len(), 3, "no write after the conflict");
        assert_eq!(
            block(get_last_apply(&sp, PL)).unwrap(),
            None,
            "dead undo kept"
        );
    }

    /// The draft's snapshot_id still "matches" (a stale GET would say so) but the tracks changed
    /// since the draft: conflict, nothing written. Order counts too.
    #[test]
    fn changed_items_are_a_conflict_whatever_the_snapshot_id() {
        for (name, have, drafted) in [
            ("studio-conflict", "AB", "ABC"),
            ("studio-conflict-order", "BA", "AB"),
        ] {
            let stub = Stub::new(&[(200, &pg(&v(have)))]);
            let d = temp_dir(name);
            let sp = connected(&d, &stub);
            let e = block(apply_studio_draft(&sp, &draft(drafted, "s1"), &v("A"))).unwrap_err();
            assert_eq!(e.code, Code::Conflict, "{have} vs {drafted}");
            assert_eq!(stub.calls().len(), 1, "nothing written");
            assert_eq!(block(get_last_apply(&sp, PL)).unwrap(), None);
        }
    }

    /// The snapshot_id moved since the draft (a content-neutral bump) but the tracks are the
    /// same: apply goes ahead.
    #[test]
    fn moved_snapshot_id_with_same_items_applies() {
        let d = temp_dir("studio-ok");
        let stub = Stub::new(&[(200, &pg(&v("AB"))), (200, r#"{"snapshot_id":"s9"}"#)]);
        let sp = connected(&d, &stub);
        let a = block(apply_studio_draft(&sp, &draft("AB", "s1"), &v("B"))).unwrap();
        assert_eq!(stub.calls()[1].0, "PUT");
        no_snapshot_get(&stub);
        assert_eq!(
            a.undo_id,
            block(get_last_apply(&sp, PL)).unwrap().unwrap().undo_id
        );
    }

    /// A write that fails partway: the snapshot was stored first and expects the landed prefix,
    /// so undo restores everything (PUT 100, POST 50) and then drops the snapshot.
    #[test]
    fn snapshot_is_stored_before_the_write_and_undo_restores_it() {
        let d = temp_dir("studio-apply");
        let orig = many(150);
        let keep: Vec<String> = orig.iter().rev().cloned().collect();
        let stub = Stub::new(&[
            (200, &pg(&orig)),
            (200, r#"{"snapshot_id":"s2"}"#), // PUT lands
            (500, ""),                        // POST fails
        ]);
        let sp = connected(&d, &stub);
        let e = block(apply_studio_draft(&sp, &draft_of(&orig, "s1"), &keep)).unwrap_err();
        assert_eq!(e.code, Code::Internal);
        let rec = block(get_last_apply(&sp, PL))
            .unwrap()
            .expect("snapshot first");

        settle(&sp); // the failed POST (500) may land; undo waits UNSURE_WAIT for it
        stub.push(200, &pg(&keep[..100])); // undo's read: our own partial write
        stub.push(200, r#"{"snapshot_id":"s3"}"#);
        stub.push(201, r#"{"snapshot_id":"s4"}"#);
        block(undo_last_apply(&sp, PL)).unwrap();
        let reqs = stub.requests.lock().unwrap();
        let writes: Vec<_> = reqs.iter().skip(4).collect();
        assert_eq!(writes.len(), 2);
        assert_eq!(
            (writes[0].method.clone(), writes[1].method.clone()),
            (Method::PUT, Method::POST)
        );
        assert_eq!(body_uris(&writes[0].body), orig[..100]);
        assert_eq!(body_uris(&writes[1].body), orig[100..]);
        drop(reqs);
        assert!(rec.undo_id > 0);
        assert_eq!(block(get_last_apply(&sp, PL)).unwrap(), None);
        assert_eq!(
            block(undo_last_apply(&sp, PL)).unwrap_err().code,
            Code::NotFound
        );
    }

    /// Spotify refuses the first write (4xx): nothing changed, so no undo is offered.
    #[test]
    fn refused_first_write_drops_the_snapshot() {
        for (name, status) in [("studio-first-400", 400), ("studio-first-403", 403)] {
            let stub = Stub::new(&[(200, &pg(&v("AB"))), (status, "")]);
            let d = temp_dir(name);
            let sp = connected(&d, &stub);
            assert!(block(apply_studio_draft(&sp, &draft("AB", "s1"), &v("A"))).is_err());
            assert_eq!(block(get_last_apply(&sp, PL)).unwrap(), None, "{status}");
            assert_eq!(stub.calls().len(), 2, "no re-read after a refusal");
        }
    }

    /// An undo that fails after its PUT landed: the retry expects that PUT's list, not the
    /// apply's, so it goes through instead of a false `conflict`.
    #[test]
    fn partial_undo_can_be_retried() {
        let d = temp_dir("studio-undo-retry");
        let orig = many(150);
        let stub = Stub::new(&[
            (200, &pg(&orig)),
            (200, r#"{"snapshot_id":"s2"}"#), // apply PUT
            (200, &pg(&v("A"))),              // undo's read
            (200, r#"{"snapshot_id":"s5"}"#), // undo PUT lands
            (500, ""),                        // undo POST fails
        ]);
        let sp = connected(&d, &stub);
        block(apply_studio_draft(&sp, &draft_of(&orig, "s1"), &v("A"))).unwrap();
        assert!(block(undo_last_apply(&sp, PL)).is_err());
        assert!(block(get_last_apply(&sp, PL)).unwrap().is_some());
        settle(&sp);
        stub.push(200, &pg(&orig[..100])); // retry's read: the undo's own PUT
        stub.push(200, r#"{"snapshot_id":"s6"}"#);
        stub.push(201, r#"{"snapshot_id":"s7"}"#);
        block(undo_last_apply(&sp, PL)).unwrap();
        assert_eq!(block(get_last_apply(&sp, PL)).unwrap(), None);
    }

    /// The first write's answer is lost (502) but it ran: the snapshot stays and expects the
    /// written list, so undo goes through.
    #[test]
    fn unanswered_first_write_that_landed_keeps_the_undo() {
        let d = temp_dir("studio-lost-answer");
        let stub = Stub::new(&[
            (200, &pg(&v("AB"))),
            (502, ""), // PUT: gateway error, but it ran
        ]);
        let sp = connected(&d, &stub);
        assert!(block(apply_studio_draft(&sp, &draft("AB", "s1"), &v("A"))).is_err());
        assert!(block(get_last_apply(&sp, PL)).unwrap().is_some());
        stub.push(200, &pg(&v("A"))); // undo's read
        stub.push(200, r#"{"snapshot_id":"s3"}"#);
        block(undo_last_apply(&sp, PL)).unwrap();
        assert_eq!(block(get_last_apply(&sp, PL)).unwrap(), None);
    }

    /// A 502/timeout only means the client stopped waiting: the PUT may land after any re-read
    /// (the server may still be processing it, TSD §5.2). So no re-read decides it: the snapshot stays
    /// and expects the list before that write or the list it sent; a later undo that finds either
    /// restores, and anything else (someone's edit) is still a conflict.
    #[test]
    fn unknown_write_outcome_keeps_the_undo_and_undo_recognises_it() {
        let orig = many(150);
        let keep: Vec<String> = orig.iter().rev().cloned().collect();
        let unknown = |name: &str, found: &[String]| {
            let stub = Stub::new(&[
                (200, &pg(&orig)),
                (502, ""), // PUT: unknown, maybe still landing
            ]);
            let d = temp_dir(name);
            let sp = connected(&d, &stub);
            assert!(block(apply_studio_draft(&sp, &draft_of(&orig, "s1"), &keep)).is_err());
            assert!(
                block(get_last_apply(&sp, PL)).unwrap().is_some(),
                "undo dropped"
            );
            assert_eq!(
                stub.calls().len(),
                2,
                "no re-read decides a maybe-landed write"
            );
            stub.push(200, &pg(found));
            stub.push(200, r#"{"snapshot_id":"s3"}"#);
            stub.push(201, r#"{"snapshot_id":"s4"}"#);
            block(undo_last_apply(&sp, PL)).map(|_| block(get_last_apply(&sp, PL)).unwrap())
        };
        assert_eq!(
            unknown("studio-unknown-landed", &keep[..100]).unwrap(),
            None
        );
        assert_eq!(
            unknown("studio-unknown-edit", &v("Z")).unwrap_err().code,
            Code::Conflict
        );
    }

    /// Undo finds the list from before a write with unknown outcome: that write may still land,
    /// so within UNSURE_WAIT undo writes nothing and keeps the snapshot (`spotify_busy`); after
    /// it, the read is trusted and undo restores. Same for a later POST chunk (audit r3).
    #[test]
    fn undo_waits_while_an_unanswered_write_may_still_land() {
        let orig = many(250);
        let keep: Vec<String> = orig.iter().rev().cloned().collect();
        // (name, apply answers after the read, list undo finds = that write's `before`)
        let first: &[(u16, &str)] = &[(502, "")];
        let later: &[(u16, &str)] = &[(200, r#"{"snapshot_id":"s2"}"#), (502, "")];
        for (name, answers, before) in [
            ("studio-wait-put", first, orig.clone()),
            ("studio-wait-post", later, keep[..100].to_vec()),
        ] {
            let stub = Stub::new(&[(200, &pg(&orig))]);
            for (st, b) in answers {
                stub.push(*st, b);
            }
            let d = temp_dir(name);
            let sp = connected(&d, &stub);
            assert!(block(apply_studio_draft(&sp, &draft_of(&orig, "s1"), &keep)).is_err());
            let calls = stub.calls().len();
            stub.push(200, &pg(&before));
            let e = block(undo_last_apply(&sp, PL)).unwrap_err();
            assert_eq!(e.code, Code::SpotifyBusy, "{name}");
            assert!(
                e.retry_after_s
                    .is_some_and(|s| s > 0 && s <= UNSURE_WAIT.as_secs())
            );
            assert_eq!(stub.calls().len(), calls + 1, "{name}: nothing written");
            assert!(
                block(get_last_apply(&sp, PL)).unwrap().is_some(),
                "{name}: undo dropped"
            );

            // UNSURE_WAIT later: the read is trusted, undo restores all 250 and drops the snapshot.
            settle(&sp);
            stub.push(200, &pg(&before));
            stub.push(200, r#"{"snapshot_id":"s4"}"#);
            stub.push(201, r#"{"snapshot_id":"s5"}"#);
            stub.push(201, r#"{"snapshot_id":"s6"}"#);
            block(undo_last_apply(&sp, PL)).unwrap();
            assert_eq!(block(get_last_apply(&sp, PL)).unwrap(), None, "{name}");
        }
    }

    /// Apply after an apply whose write has unknown outcome: within UNSURE_WAIT that write may
    /// still land and cut the new result, so apply writes nothing and keeps the old snapshot
    /// (`spotify_busy`); after it, apply runs.
    #[test]
    fn apply_waits_while_an_unanswered_write_may_still_land() {
        let orig = v("ABC");
        let stub = Stub::new(&[(200, &pg(&orig)), (502, "")]);
        let d = temp_dir("studio-apply-wait");
        let sp = connected(&d, &stub);
        assert!(block(apply_studio_draft(&sp, &draft_of(&orig, "s1"), &v("AB"))).is_err());
        let first = block(get_last_apply(&sp, PL)).unwrap().unwrap();
        let calls = stub.calls().len();
        stub.push(200, &pg(&orig));
        let e = block(apply_studio_draft(&sp, &draft_of(&orig, "s1"), &v("C"))).unwrap_err();
        assert_eq!(e.code, Code::SpotifyBusy);
        assert!(
            e.retry_after_s
                .is_some_and(|s| s > 0 && s <= UNSURE_WAIT.as_secs())
        );
        assert_eq!(stub.calls().len(), calls + 1, "nothing written");
        assert_eq!(
            block(get_last_apply(&sp, PL)).unwrap(),
            Some(first),
            "old undo kept"
        );

        settle(&sp);
        stub.push(200, &pg(&orig));
        stub.push(200, r#"{"snapshot_id":"s2"}"#);
        let done = block(apply_studio_draft(&sp, &draft_of(&orig, "s1"), &v("C"))).unwrap();
        assert_eq!(
            block(get_last_apply(&sp, PL)).unwrap().map(|s| s.undo_id),
            Some(done.undo_id)
        );
    }

    /// A later chunk's answer is lost (502) but it ran: undo finds the whole written list (the
    /// `after` of chunk 3, not chunk 1's) and restores instead of a false `conflict`.
    #[test]
    fn unknown_later_chunk_that_landed_keeps_the_undo() {
        let orig = many(250);
        let keep: Vec<String> = orig.iter().rev().cloned().collect();
        let stub = Stub::new(&[
            (200, &pg(&orig)),
            (200, r#"{"snapshot_id":"s2"}"#), // PUT 100
            (201, r#"{"snapshot_id":"s3"}"#), // POST 100
            (502, ""),                        // POST 50: unknown, but it ran
        ]);
        let d = temp_dir("studio-unknown-later");
        let sp = connected(&d, &stub);
        assert!(block(apply_studio_draft(&sp, &draft_of(&orig, "s1"), &keep)).is_err());
        stub.push(200, &pg(&keep)); // undo's read: all 250 of our own writes
        stub.push(200, r#"{"snapshot_id":"s4"}"#);
        stub.push(201, r#"{"snapshot_id":"s5"}"#);
        stub.push(201, r#"{"snapshot_id":"s6"}"#);
        block(undo_last_apply(&sp, PL)).unwrap();
        assert_eq!(block(get_last_apply(&sp, PL)).unwrap(), None);
    }

    /// After an apply the user adds an episode or a local file on Spotify (or grows it past the
    /// ceiling): that's an edit, so undo is a `conflict` and drops the snapshot (no dead Undo).
    #[test]
    fn undo_on_a_playlist_studio_cant_read_is_a_conflict() {
        let local = json!({"items": [{"is_local": true, "item": {"uri": "spotify:local:a:b:c:1", "name": "x", "artists": []}}], "next": null});
        let episode = json!({"items": [{"is_local": false, "item": {"uri": format!("spotify:episode:{}", "E".repeat(22)), "name": "x", "artists": []}}], "next": null});
        for (name, page) in [
            ("studio-undo-local", local),
            ("studio-undo-episode", episode),
        ] {
            let stub = Stub::new(&[(200, &pg(&v("AB"))), (200, r#"{"snapshot_id":"s2"}"#)]);
            let d = temp_dir(name);
            let sp = connected(&d, &stub);
            block(apply_studio_draft(&sp, &draft("AB", "s1"), &v("B"))).unwrap();
            stub.push(200, &page.to_string());
            assert_eq!(
                block(undo_last_apply(&sp, PL)).unwrap_err().code,
                Code::Conflict
            );
            assert_eq!(stub.calls().len(), 3, "nothing written");
            assert_eq!(
                block(get_last_apply(&sp, PL)).unwrap(),
                None,
                "dead undo kept"
            );
        }
    }

    /// A 429 the client can't wait out on a later chunk: the PUT already cut the playlist, so the
    /// error must not read as `spotify_busy` (nothing written) for apply or undo (audit r6).
    #[test]
    fn busy_later_chunk_is_not_spotify_busy() {
        let busy = || crate::spotify::client::HttpResp {
            status: 429,
            retry_after: Some(120),
            body: String::new(),
        };
        let orig = many(150);
        let keep: Vec<String> = orig.iter().rev().cloned().collect();
        let stub = Stub::new(&[(200, &pg(&orig)), (200, r#"{"snapshot_id":"s2"}"#)]);
        stub.push_resp(busy());
        let d = temp_dir("studio-busy-later");
        let sp = connected(&d, &stub);
        let e = block(apply_studio_draft(&sp, &draft_of(&orig, "s1"), &keep)).unwrap_err();
        assert_ne!(e.code, Code::SpotifyBusy, "apply");
        assert!(block(get_last_apply(&sp, PL)).unwrap().is_some());

        stub.push(200, &pg(&keep[..100])); // undo's read: the landed PUT
        stub.push(200, r#"{"snapshot_id":"s3"}"#);
        stub.push_resp(busy());
        let e = block(undo_last_apply(&sp, PL)).unwrap_err();
        assert_ne!(e.code, Code::SpotifyBusy, "undo");
        assert!(block(get_last_apply(&sp, PL)).unwrap().is_some(), "kept");
    }

    /// The playlist was deleted or unfollowed (404 on the read): the undo can never go through,
    /// so it's dropped (`conflict`) instead of a `not_found` that leaves a dead Undo banner.
    #[test]
    fn undo_on_a_deleted_playlist_drops_the_snapshot() {
        let stub = Stub::new(&[(200, &pg(&v("AB"))), (200, r#"{"snapshot_id":"s2"}"#)]);
        let d = temp_dir("studio-undo-404");
        let sp = connected(&d, &stub);
        block(apply_studio_draft(&sp, &draft("AB", "s1"), &v("B"))).unwrap();
        stub.push(404, "");
        assert_eq!(
            block(undo_last_apply(&sp, PL)).unwrap_err().code,
            Code::Conflict
        );
        assert_eq!(block(get_last_apply(&sp, PL)).unwrap(), None);
    }

    /// Apply, apply, undo: the older apply's snapshot doesn't come back as a dead Undo (one
    /// undo per playlist, as the mock); undo restores the second apply's starting point.
    #[test]
    fn second_apply_replaces_the_first_undo() {
        let d = temp_dir("studio-two-applies");
        let stub = Stub::new(&[
            (200, &pg(&v("AB"))),
            (200, r#"{"snapshot_id":"s2"}"#), // apply 1 PUT
            (200, &pg(&v("A"))),
            (200, r#"{"snapshot_id":"s3"}"#), // apply 2 PUT
            (200, &pg(&v("B"))),              // undo's read
            (200, r#"{"snapshot_id":"s4"}"#), // undo PUT
        ]);
        let sp = connected(&d, &stub);
        block(apply_studio_draft(&sp, &draft("AB", "s1"), &v("A"))).unwrap();
        block(apply_studio_draft(&sp, &draft("A", "s2"), &v("B"))).unwrap();
        block(undo_last_apply(&sp, PL)).unwrap();
        assert_eq!(body_uris(&stub.requests.lock().unwrap()[5].body), v("A"));
        assert_eq!(block(get_last_apply(&sp, PL)).unwrap(), None);
    }

    /// Disconnect waits for a running apply: the playlist is never left cut short with its undo
    /// snapshot deleted underneath it.
    #[test]
    fn disconnect_waits_for_a_running_apply() {
        let d = temp_dir("studio-disconnect");
        let mut stub = Stub::new(&[
            (200, &pg(&many(150))),
            (200, r#"{"snapshot_id":"s2"}"#), // PUT
            (201, r#"{"snapshot_id":"s3"}"#), // POST
        ]);
        stub.delay_ms = 100;
        let sp = std::sync::Arc::new(connected(&d, &stub));
        let sp2 = sp.clone();
        let keep: Vec<String> = many(150).into_iter().rev().collect();
        let job = tauri::async_runtime::spawn(async move {
            apply_studio_draft(&sp2, &draft_of(&many(150), "s1"), &keep).await
        });
        std::thread::sleep(std::time::Duration::from_millis(150)); // apply is in flight
        block(sp.disconnect()).unwrap();
        block(job).unwrap().unwrap();
        let methods: Vec<String> = stub.calls().into_iter().map(|c| c.0).collect();
        assert_eq!(methods, ["GET", "PUT", "POST"], "every chunk landed");
        assert!(!d.join(crate::cleanup::CACHE_DB).exists());
    }

    /// Longer than the ceiling: refused, never truncated (an apply would delete the rest).
    #[test]
    fn playlist_over_the_ceiling_is_refused() {
        let d = temp_dir("studio-cap");
        let rows: Vec<Value> = (0..=MAX_ITEMS)
            .map(|i| json!({"item": {"uri": format!("spotify:track:{:0>22}", i), "name": "n", "artists": []}}))
            .collect();
        let body = json!({"items": rows, "next": null}).to_string();
        let stub = Stub::new(&[(200, r#"{"snapshot_id":"s1"}"#), (200, &body)]);
        let sp = connected(&d, &stub);
        let db = Mutex::new(crate::db::open_in_memory().unwrap());
        let e = block(load_studio_draft(&sp, &db, PL)).unwrap_err();
        assert_eq!(e.code, Code::Validation);
    }

    /// One apply/undo at a time: while another holds the Studio lock, nothing is sent.
    #[test]
    fn apply_waits_for_a_running_apply_or_undo() {
        let d = temp_dir("studio-lock");
        let stub = Stub::new(&[(200, &pg(&v("A")))]);
        let sp = std::sync::Arc::new(connected(&d, &stub));
        let held = block(sp.studio_lock.lock());
        let sp2 = sp.clone();
        let job = tauri::async_runtime::spawn(async move {
            apply_studio_draft(&sp2, &draft("AB", "s1"), &v("A")).await
        });
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(stub.calls().is_empty(), "apply ran while the lock was held");
        drop(held);
        assert_eq!(block(job).unwrap().unwrap_err().code, Code::Conflict);
    }

    #[test]
    fn cross_playlist_dupes_name_the_other_playlists() {
        let d = temp_dir("studio-cross");
        let pl = |c: char, name: &str| {
            json!({"uri": format!("spotify:playlist:{}", c.to_string().repeat(22)), "name": name,
                   "owner": {"id": "me1"}, "collaborative": false, "items": {"total": 1}})
        };
        let mine = format!("spotify:playlist:{}", "A".repeat(22));
        let lists = json!({"items": [pl('A', "Mine"), pl('B', "Road"), pl('C', "Gone"), pl('D', "Gym")], "next": null});
        let stub = Stub::new(&[
            (200, &pg(&v("xyxz"))), // the checked playlist: x twice
            (200, r#"{"id":"me1"}"#),
            (200, &lists.to_string()),
            (200, &pg(&v("qxx"))), // Road: x (twice, named once)
            (404, "{}"),           // Gone: skipped
            (200, &pg(&v("zx"))),  // Gym: z, x
        ]);
        let sp = connected(&d, &stub);
        let got = block(find_cross_playlist_dupes(&sp, &mine)).unwrap();
        let names = |i: usize| {
            got[i]
                .playlists
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(got.len(), 2, "y is nowhere else");
        assert_eq!(
            (got[0].uri.clone(), names(0)),
            (t(b'x'), vec!["Road", "Gym"])
        );
        assert_eq!((got[1].uri.clone(), names(1)), (t(b'z'), vec!["Gym"]));
        assert_eq!(
            stub.calls().len(),
            6,
            "one items read per other playlist, none for the checked one twice"
        );
        assert!(stub.calls().iter().all(|(m, _)| m == "GET"));
    }

    #[test]
    fn lists_owned_and_collaborative_only() {
        let d = temp_dir("studio-list");
        let pl = |id: &str, owner: &str, collab: bool| {
            json!({"uri": format!("spotify:playlist:{}", id.repeat(22)), "name": id,
                   "owner": {"id": owner}, "collaborative": collab, "items": {"total": 7}})
        };
        let body = json!({"items": [pl("A", "me1", false), pl("B", "other", false), pl("C", "other", true), null], "next": null});
        let stub = Stub::new(&[(200, r#"{"id":"me1"}"#), (200, &body.to_string())]);
        let sp = connected(&d, &stub);
        let ls = block(list_spotify_playlists(&sp)).unwrap();
        assert_eq!(
            ls.iter()
                .map(|p| (p.name.as_str(), p.owned, p.collaborative, p.tracks))
                .collect::<Vec<_>>(),
            [("A", true, false, 7), ("C", false, true, 7)]
        );
    }
}
