//! Runout Inbox (FR-046, D33, TSD §5.3): a private playlist the user adds to from any device. A
//! check imports each new track as a "name needed" item (`added_via = inbox`, URI only, promoted:
//! the user put it there, TSD §4.2), collapses album batches into one album item, then removes the
//! handled entries. The playlist id and options live in settings (the user's own pointer); the
//! followed-artists baseline is Spotify data, so it lives in `spotify_cache.db`.
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use reqwest::Method;
use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::{Value, json};

use super::auth::FOLLOW_SCOPE;
use super::{Spotify, cache, check_uri, studio::either};
use crate::error::{AppError, Result};
use crate::library::{ItemType, items};
use crate::settings::Inbox;
use crate::state::AppState;

pub const NAME: &str = "Runout Inbox";
const DESCRIPTION: &str =
    "Add songs or albums here from any device; Runout moves them into your backlog.";
/// Background check period (TSD §5.3); also once at start.
pub const EVERY: Duration = Duration::from_secs(15 * 60);
/// `DELETE /playlists/{id}/items` takes at most 100 objects (kb/spotify.md).
const CHUNK: usize = 100;
/// Tracks of one album added in the same minute covering this share of it → one album item.
const ALBUM_SHARE: f64 = 0.8;
/// Reads stop here (a playlist, saved albums or followed artists list longer than this is cut).
const MAX_READ: usize = 10_000;
/// Cache key of the followed-artist baseline (expires like any cache entry: then re-baselined).
const FOLLOWED: &str = "runout:inbox:followed";

/// API.md `get_inbox` / `set_inbox_options` / `create_inbox`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InboxStatus {
    pub state: &'static str,
    pub playlist_uri: Option<String>,
    pub last_check: Option<String>,
    pub imported_last: i64,
    pub saved_albums: bool,
    pub followed_artists: bool,
    /// Followed artists is on but the sign-in lacks `user-follow-read`: Reconnect Spotify.
    pub needs_reconnect: bool,
}

/// API.md `check_inbox` and the `inbox-imported` payload. `imported` = new items (any type),
/// `albums` = how many of those are albums, `skipped` = already in the library.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Checked {
    pub imported: i64,
    pub albums: i64,
    pub skipped: i64,
}

fn inbox(st: &AppState) -> Inbox {
    st.settings.lock().unwrap().inbox.clone()
}

/// Change the inbox settings and save them (settings lock only; never with the db lock).
fn update(st: &AppState, f: impl FnOnce(&mut Inbox)) -> Result<()> {
    st.edit_settings(|s| {
        f(&mut s.inbox);
        Ok(())
    })
    .map(|_| ())
}

fn now() -> String {
    crate::db::now()
}

pub fn status(st: &AppState, sp: &Spotify) -> InboxStatus {
    let i = inbox(st);
    InboxStatus {
        state: match (&i.playlist_id, i.missing) {
            (None, _) => "none",
            (Some(_), true) => "missing",
            (Some(_), false) => "ready",
        },
        playlist_uri: i.playlist_id.map(|id| format!("spotify:playlist:{id}")),
        last_check: i.last_check,
        imported_last: i.imported_last,
        saved_albums: i.saved_albums,
        followed_artists: i.followed_artists,
        needs_reconnect: i.followed_artists
            && sp.tokens().is_some_and(|t| !t.has_scope(FOLLOW_SCOPE)),
    }
}

/// `create_inbox`: a new private playlist, unless a ready one is recorded (never a second one).
pub async fn create(st: &AppState, sp: &Spotify) -> Result<InboxStatus> {
    let _one = sp.inbox_lock.lock().await;
    if status(st, sp).state == "ready" {
        return Ok(status(st, sp));
    }
    let pl = sp
        .send_json(
            Method::POST,
            "/me/playlists",
            Some(json!({ "name": NAME, "public": false, "description": DESCRIPTION })),
        )
        .await?;
    let uri = check_uri(pl["uri"].as_str().unwrap_or_default(), &["playlist"])
        .map_err(|_| AppError::internal("Spotify didn't return the new playlist."))?;
    let id = super::uri_id(&uri).to_string();
    update(st, |i| {
        i.playlist_id = Some(id);
        i.missing = false;
    })?;
    Ok(status(st, sp))
}

/// After a successful `connect_spotify`: create the Inbox if none was ever recorded. Best effort:
/// on failure Settings shows `none` and offers Create Inbox.
pub async fn ensure_on_connect(st: &AppState, sp: &Spotify) {
    if inbox(st).playlist_id.is_none() {
        let _ = create(st, sp).await;
    }
}

/// `set_inbox_options` (connected only, like every R3 command but status). Saved albums counts from the moment it is turned on; turning followed
/// artists off drops the baseline, so turning it on again re-baselines (imports nothing).
pub async fn set_options(
    st: &AppState,
    sp: &Spotify,
    saved_albums: bool,
    followed_artists: bool,
) -> Result<InboxStatus> {
    sp.require_connected()?;
    let _one = sp.inbox_lock.lock().await; // a running check can't write the baseline back
    let was = inbox(st);
    if was.followed_artists && !followed_artists {
        let _ = sp.with_cache(|c| {
            c.execute("DELETE FROM cache_entry WHERE uri = ?1", [FOLLOWED])?;
            Ok(())
        });
    }
    update(st, |i| {
        if saved_albums && !i.saved_albums {
            i.albums_after = Some(now());
        }
        i.saved_albums = saved_albums;
        i.followed_artists = followed_artists;
    })?;
    Ok(status(st, sp))
}

/// Walk a paging object (`wrap` = the key holding it, e.g. `artists`) calling `f` per entry until
/// it returns false or [`MAX_READ`] entries were seen.
async fn each(
    sp: &Spotify,
    first: &str,
    wrap: Option<&str>,
    mut f: impl FnMut(&Value) -> bool,
) -> Result<()> {
    let (mut next, mut n) = (Some(first.to_string()), 0);
    while let Some(path) = next.take() {
        let page: Value = sp.get_json(&path).await?;
        let p = wrap.map_or(&page, |k| &page[k]);
        for it in p["items"].as_array().into_iter().flatten() {
            n += 1;
            if !f(it) || n >= MAX_READ {
                return Ok(());
            }
        }
        next = p["next"].as_str().map(str::to_string);
    }
    Ok(())
}

/// One item to import: kind, URI and, for an Inbox track, `(album URI, added_at)` with `added_at`
/// in the stored UTC format (see [`import`]); `None` when either is unknown.
type Add = (ItemType, String, Option<(String, String)>);

/// An entry's `added_at` as `YYYY-MM-DDTHH:MM:SSZ` (`item.created_at`'s format); `None` if absent
/// (very old playlist entries) or unreadable.
fn added_at(e: &Value) -> Option<String> {
    let t = chrono::DateTime::parse_from_rfc3339(e["added_at"].as_str()?).ok()?;
    Some(crate::insights::utc(t.with_timezone(&chrono::Utc)))
}

/// What one Inbox read asks for: the new items, and the track URIs to remove, grouped as planned
/// (an album batch is one group) so [`chunks`] never splits a batch across two DELETEs.
#[derive(Debug, Default, PartialEq)]
struct Plan {
    add: Vec<Add>,
    remove: Vec<Vec<String>>,
}

/// Removal chunks of ≤ [`CHUNK`] URIs, a group never split unless it alone is over [`CHUNK`]. If
/// the second DELETE failed after the first removed part of a batch, the next check would see
/// under 80 % of the album and import the rest as stray tracks.
fn chunks(groups: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = vec![];
    let mut cur: Vec<String> = vec![];
    for g in groups {
        if !cur.is_empty() && cur.len() + g.len() > CHUNK {
            out.push(std::mem::take(&mut cur));
        }
        if g.len() > CHUNK {
            out.extend(g.chunks(CHUNK).map(<[String]>::to_vec));
        } else {
            cur.extend(g.iter().cloned());
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Playlist entries → plan. Only Spotify tracks are handled (local files and episodes stay in
/// the Inbox). Tracks of one album added within the same minute covering ≥ [`ALBUM_SHARE`] of the
/// album's `total_tracks` (≥ 2 tracks) become one album item.
// ponytail: album size from the entry's `album.total_tracks`, not a track-list read (TSD §5.3).
fn plan(entries: &[Value]) -> Plan {
    let mut groups: Vec<((String, String), Vec<String>, i64)> = vec![];
    let mut at: HashMap<(String, String), usize> = HashMap::new();
    let mut seen = HashSet::new(); // a URI is planned (and removed) once, in its first group
    let mut when: HashMap<String, String> = HashMap::new();
    let mut p = Plan::default();
    for e in entries {
        let t = either(e, "item", "track");
        let Ok(uri) = check_uri(t["uri"].as_str().unwrap_or_default(), &["track"]) else {
            continue;
        };
        if e["is_local"] == true {
            continue;
        }
        if !seen.insert(uri.clone()) {
            continue;
        }
        if let Some(a) = added_at(e) {
            when.insert(uri.clone(), a);
        }
        let album = t["album"]["uri"].as_str().unwrap_or_default().to_string();
        let minute: String = e["added_at"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(16)
            .collect();
        let key = (album, minute);
        let i = *at.entry(key.clone()).or_insert_with(|| {
            groups.push((
                key,
                vec![],
                t["album"]["total_tracks"].as_i64().unwrap_or(0),
            ));
            groups.len() - 1
        });
        groups[i].1.push(uri);
    }
    for ((album, _), tracks, total) in groups {
        let batch = total >= 2
            && tracks.len() >= 2
            && tracks.len() as f64 >= ALBUM_SHARE * total as f64
            && check_uri(&album, &["album"]).is_ok();
        let al = check_uri(&album, &["album"]).ok();
        if batch {
            p.add.push((ItemType::Album, album, None));
        } else {
            p.add.extend(tracks.iter().map(|u| {
                let cover = al.clone().zip(when.get(u).cloned());
                (ItemType::Track, u.clone(), cover)
            }));
        }
        p.remove.push(tracks);
    }
    p
}

/// Insert each item not already in the library (one transaction). A track that was already in the
/// Inbox when its album was imported from it (entry `added_at` ≤ the album item's `created_at`, the
/// album `added_via = inbox`) counts as skipped: a leftover of that batch (a batch over 100 tracks
/// spans DELETEs, or a removal failed). A track added later is imported (FR-046); so is one with
/// no `added_at`.
// ponytail: second-resolution timestamps from two clocks (Spotify's, this PC's). Clock skew, or a
// track added in the same check's minute after the batch, can tip a deliberate add into "skipped"
// (or a leftover into a stray track); the upgrade is storing the batch's entry times with the album.
fn import(st: &AppState, add: &[Add], out: &mut Checked) -> Result<()> {
    let mut db = st.db.lock().unwrap();
    let tx = db.transaction()?;
    for (kind, uri, album) in add {
        let covered = match album {
            Some((a, at)) => tx
                .query_row(
                    "SELECT 1 FROM item WHERE spotify_uri = ?1 AND added_via = 'inbox' AND created_at >= ?2",
                    [a, at],
                    |_| Ok(()),
                )
                .optional()?
                .is_some(),
            None => false,
        };
        if covered || items::id_by_uri(&tx, uri)?.is_some() {
            out.skipped += 1;
            continue;
        }
        items::insert_name_needed(&tx, *kind, uri, "inbox")?;
        out.imported += 1;
        if *kind == ItemType::Album {
            out.albums += 1;
        }
    }
    tx.commit()?;
    Ok(())
}

/// `check_inbox` (also at start and every 15 min, lib.rs). Disconnected → `spotify_auth` before
/// any request. A gone Inbox (unlisted or 404) → state `missing` and `not_found`, nothing imported,
/// never re-created here; no Inbox recorded → nothing to do.
pub async fn check(st: &AppState, sp: &Spotify) -> Result<Checked> {
    sp.require_connected()?;
    let _one = sp.inbox_lock.lock().await;
    let i = inbox(st);
    let gone = || AppError::not_found("The Runout Inbox is gone. Create it again in Settings.");
    let Some(id) = i.playlist_id.clone() else {
        return Ok(Checked::default());
    };
    if i.missing {
        return Err(gone());
    }
    let started = now();
    let mut out = Checked::default();

    // Deleting a playlist in Spotify only unfollows it (its items stay readable): check it's listed.
    let mut listed = false;
    each(sp, "/me/playlists?limit=50", None, |p| {
        listed = p["id"].as_str() == Some(id.as_str());
        !listed
    })
    .await?;
    let mut entries = vec![];
    let read = if listed {
        each(sp, &format!("/playlists/{id}/items?limit=50"), None, |e| {
            entries.push(e.clone());
            true
        })
        .await
    } else {
        Err(gone())
    };
    match read {
        Err(e) if e.code == crate::error::Code::NotFound => {
            update(st, |i| i.missing = true)?;
            return Err(gone()); // not "Nothing new": the UI offers Create Inbox
        }
        r => r?,
    }

    let p = plan(&entries);
    import(st, &p.add, &mut out)?;
    // Imported (or skipped) entries leave the Inbox; a failed removal waits for the next check
    // (import is idempotent by URI). Every copy of a URI goes (kb/spotify.md).
    for c in chunks(&p.remove) {
        let body = json!({ "items": c.iter().map(|u| json!({ "uri": u })).collect::<Vec<_>>() });
        if sp
            .send_json(
                Method::DELETE,
                &format!("/playlists/{id}/items"),
                Some(body),
            )
            .await
            .is_err()
        {
            break;
        }
    }

    // Optional extras are best effort: their failure never hides the Inbox import above.
    let albums_ok = i.saved_albums
        && saved_albums(
            st,
            sp,
            i.albums_after.as_deref().unwrap_or(&started),
            &mut out,
        )
        .await
        .is_ok();
    if i.followed_artists && sp.tokens().is_some_and(|t| t.has_scope(FOLLOW_SCOPE)) {
        let _ = followed(st, sp, &mut out).await;
    }

    update(st, |i| {
        i.last_check = Some(started.clone());
        i.imported_last = out.imported;
        if albums_ok && i.saved_albums {
            i.albums_after = Some(started.clone());
        }
    })?;
    Ok(out)
}

/// Albums saved after `after`. Newest first (Spotify's order for saved albums): stops at the first
/// one not newer.
async fn saved_albums(st: &AppState, sp: &Spotify, after: &str, out: &mut Checked) -> Result<()> {
    let mut add = vec![];
    each(sp, "/me/albums?limit=50", None, |e| {
        let newer = e["added_at"].as_str().is_some_and(|a| a > after);
        if let (true, Some(u)) = (newer, e["album"]["uri"].as_str())
            && let Ok(u) = check_uri(u, &["album"])
        {
            add.push((ItemType::Album, u, None));
        }
        newer
    })
    .await?;
    import(st, &add, out)
}

/// Artists followed since the previous read, diffed against the cached baseline. No baseline (just
/// turned on, disconnected since, or expired): this read becomes it, nothing imported. The new
/// baseline is stored only after the import succeeded, so a failed import is retried next time.
async fn followed(st: &AppState, sp: &Spotify, out: &mut Checked) -> Result<()> {
    let mut ids = vec![];
    each(
        sp,
        "/me/following?type=artist&limit=50",
        Some("artists"),
        |a| {
            if let Some(id) = a["id"].as_str() {
                ids.push(id.to_string());
            }
            true
        },
    )
    .await?;
    let t = chrono::Utc::now();
    if let Some(Value::Array(base)) = sp.with_cache(|c| cache::get(c, FOLLOWED, t))? {
        let old: HashSet<&str> = base.iter().filter_map(Value::as_str).collect();
        let add: Vec<_> = ids
            .iter()
            .filter(|a| !old.contains(a.as_str()))
            .filter_map(|a| check_uri(&format!("spotify:artist:{a}"), &["artist"]).ok())
            .map(|u| (ItemType::Artist, u, None))
            .collect();
        import(st, &add, out)?;
    }
    sp.with_cache(|c| cache::put(c, FOLLOWED, "inbox_followed", &json!(ids), t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spotify::testing::*;
    use std::sync::Mutex;

    const PL: &str = "InboxPlaylist000000001";

    fn state(d: &std::path::Path) -> AppState {
        AppState {
            db: Mutex::new(crate::db::open_in_memory().unwrap()),
            paths: crate::state::Paths {
                config_dir: d.join("cfg"),
                data_dir: d.into(),
                local_data_dir: d.into(),
            },
            settings: Mutex::new(crate::settings::Settings::defaults(d, d)),
        }
    }

    fn ready(st: &AppState) {
        update(st, |i| i.playlist_id = Some(PL.into())).unwrap();
    }

    fn id(n: &str) -> String {
        format!("{n:0>22}")
    }

    fn entry(track: &str, album: &str, total: i64, at: &str) -> Value {
        json!({"added_at": at, "is_local": false, "item": {
            "uri": format!("spotify:track:{}", id(track)),
            "album": {"uri": format!("spotify:album:{}", id(album)), "total_tracks": total}}})
    }

    fn listed() -> String {
        json!({"items": [{"id": "Other00000000000000001"}, {"id": PL}], "next": null}).to_string()
    }

    fn page(items: Vec<Value>) -> String {
        json!({ "items": items, "next": null }).to_string()
    }

    fn rows(st: &AppState) -> Vec<(String, String, Option<String>, String)> {
        let db = st.db.lock().unwrap();
        let mut q = db
            .prepare(
                "SELECT i.type, i.spotify_uri, i.display_name, i.added_via || '/' || p.source FROM item i
                 JOIN field_provenance p ON p.item_id = i.id AND p.field = 'spotify_uri' ORDER BY i.id",
            )
            .unwrap();
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn plan_collapses_album_batches_only() {
        let at = "2026-10-21T09:15:30Z";
        let mut e: Vec<Value> = (1..=4)
            .map(|n| entry(&format!("A{n}"), "AL1", 5, at))
            .collect();
        e.push(entry("B1", "AL2", 10, at)); // 1 of 10: a track
        e.push(entry("C1", "AL3", 3, at)); // 2 of 3 = 67 %: tracks
        e.push(entry("C2", "AL3", 3, "2026-10-21T09:15:59Z"));
        e.push(entry("D1", "AL4", 2, at)); // 1 of 2 in this minute, 1 the next: tracks
        e.push(entry("D2", "AL4", 2, "2026-10-21T09:16:00Z"));
        e.push(entry("A1", "AL1", 5, at)); // a second copy of a batch track
        e.push(json!({"is_local": true, "item": {"uri": "spotify:local:x:y:z:1"}}));
        e.push(json!({"item": {"uri": format!("spotify:episode:{}", id("E1"))}}));
        let p = plan(&e);
        let t = |n: &str, al: &str, at: &str| {
            (
                ItemType::Track,
                format!("spotify:track:{}", id(n)),
                Some((format!("spotify:album:{}", id(al)), at.to_string())),
            )
        };
        assert_eq!(
            p.add,
            vec![
                (
                    ItemType::Album,
                    format!("spotify:album:{}", id("AL1")),
                    None
                ),
                t("B1", "AL2", at),
                t("C1", "AL3", at),
                t("C2", "AL3", "2026-10-21T09:15:59Z"),
                t("D1", "AL4", at),
                t("D2", "AL4", "2026-10-21T09:16:00Z"),
            ]
        );
        let all = p.remove.concat();
        assert_eq!(
            all.len(),
            9,
            "every handled track once, never local/episode"
        );
        assert!(all.iter().all(|u| u.starts_with("spotify:track:")));
        assert_eq!(p.remove[0].len(), 4, "the album batch is one removal group");
    }

    #[test]
    fn removal_chunks_never_split_a_batch() {
        let g = |n: usize, tag: &str| (0..n).map(|i| format!("{tag}{i}")).collect::<Vec<_>>();
        let sizes =
            |groups: Vec<Vec<String>>| chunks(&groups).iter().map(Vec::len).collect::<Vec<_>>();
        let singles: Vec<Vec<String>> = (0..98).map(|i| vec![format!("s{i}")]).collect();
        assert_eq!(sizes([singles.clone(), vec![g(5, "a")]].concat()), [98, 5]);
        assert_eq!(sizes([singles, vec![g(2, "a")]].concat()), [100]);
        assert_eq!(
            sizes(vec![g(3, "x"), g(250, "big"), g(4, "y")]),
            [3, 100, 100, 50, 4]
        );
    }

    #[test]
    fn create_on_connect_once_and_never_twice() {
        let d = temp_dir("inbox-create");
        let stub = Stub::new(&[(
            201,
            &json!({"id": PL, "uri": format!("spotify:playlist:{PL}")}).to_string(),
        )]);
        let (sp, st) = (connected(&d, &stub), state(&d));
        assert_eq!(status(&st, &sp).state, "none");
        block(ensure_on_connect(&st, &sp));
        let s = status(&st, &sp);
        assert_eq!(
            (s.state, s.playlist_uri.as_deref()),
            ("ready", Some(format!("spotify:playlist:{PL}").as_str()))
        );
        let b = match &stub.requests.lock().unwrap()[0].body {
            Some(crate::spotify::Body::Json(b)) => b.clone(),
            _ => panic!("JSON body"),
        };
        assert_eq!(
            (b["name"].as_str(), &b["public"]),
            (Some(NAME), &json!(false))
        );
        block(ensure_on_connect(&st, &sp));
        block(create(&st, &sp)).unwrap();
        assert_eq!(
            stub.calls().len(),
            1,
            "a ready Inbox is never created again"
        );
        // persisted in settings.json
        let saved = std::fs::read_to_string(st.paths.settings_file()).unwrap();
        assert!(saved.contains(PL));
    }

    #[test]
    fn check_imports_skips_and_removes() {
        let d = temp_dir("inbox-check");
        let at = "2026-10-21T09:15:30Z";
        let mut e: Vec<Value> = (1..=4)
            .map(|n| entry(&format!("A{n}"), "AL1", 5, at))
            .collect();
        e.push(entry("B1", "AL2", 10, at));
        e.push(entry("K1", "AL9", 10, at)); // already in the library
        e.push(json!({"is_local": true, "item": {"uri": "spotify:local:x:y:z:1"}}));
        let stub = Stub::new(&[
            (200, &listed()),
            (200, &page(e)),
            (200, r#"{"snapshot_id":"s2"}"#),
        ]);
        let (sp, st) = (connected(&d, &stub), state(&d));
        ready(&st);
        let known = format!("spotify:track:{}", id("K1"));
        items::insert_name_needed(&st.db.lock().unwrap(), ItemType::Track, &known, "pc").unwrap();

        let got = block(check(&st, &sp)).unwrap();
        assert_eq!(
            got,
            Checked {
                imported: 2,
                albums: 1,
                skipped: 1
            }
        );
        let r = rows(&st);
        assert_eq!(r.len(), 3);
        assert_eq!(
            r[1],
            (
                "album".into(),
                format!("spotify:album:{}", id("AL1")),
                None,
                "inbox/promoted".into()
            )
        );
        assert_eq!(r[2].0, "track");
        assert_eq!(r[2].3, "inbox/promoted");
        let calls = stub.calls();
        assert!(
            calls[1]
                .1
                .ends_with(&format!("/playlists/{PL}/items?limit=50"))
        );
        assert_eq!(calls[2].0, "DELETE");
        let reqs = stub.requests.lock().unwrap();
        let Some(crate::spotify::Body::Json(b)) = &reqs[2].body else {
            panic!()
        };
        assert_eq!(
            b["items"].as_array().unwrap().len(),
            6,
            "4 batch + 1 track + 1 skipped"
        );
        assert_eq!(
            b["items"][0],
            json!({"uri": format!("spotify:track:{}", id("A1"))})
        );
        drop(reqs);
        let s = status(&st, &sp);
        assert_eq!((s.imported_last, s.last_check.is_some()), (2, true));
    }

    /// A failed removal leaves the entries; the next check skips them (already imported) and removes them.
    #[test]
    fn failed_removal_is_retried_without_a_second_import() {
        let d = temp_dir("inbox-retry");
        let e = || page(vec![entry("B1", "AL2", 10, "2026-10-21T09:15:30Z")]);
        let stub = Stub::new(&[(200, &listed()), (200, &e()), (500, "{}")]);
        let (sp, st) = (connected(&d, &stub), state(&d));
        ready(&st);
        assert_eq!(block(check(&st, &sp)).unwrap().imported, 1);
        stub.push(200, &listed());
        stub.push(200, &e());
        stub.push(200, "{}");
        let again = block(check(&st, &sp)).unwrap();
        assert_eq!((again.imported, again.skipped), (0, 1));
        assert_eq!(rows(&st).len(), 1);
        assert_eq!(stub.calls().last().unwrap().0, "DELETE");
    }

    /// External review of f01432a: an album batch crossing a 100-URI DELETE boundary whose second
    /// DELETE fails must not come back as stray track items on the next check.
    #[test]
    fn batch_never_straddles_a_failed_delete() {
        let d = temp_dir("inbox-straddle");
        let at = "2020-01-01T09:15:30Z"; // before the album item's created_at (this run's clock)
        let singles: Vec<Value> = (0..98)
            .map(|n| entry(&format!("S{n}"), &format!("SA{n}"), 10, at))
            .collect();
        let batch: Vec<Value> = (1..=5)
            .map(|n| entry(&format!("A{n}"), "AL1", 5, at))
            .collect();
        let stub = Stub::new(&[
            (200, &listed()),
            (200, &page([singles, batch.clone()].concat())),
            (200, "{}"),
            (500, "{}"),
        ]);
        let (sp, st) = (connected(&d, &stub), state(&d));
        ready(&st);
        let first = block(check(&st, &sp)).unwrap();
        assert_eq!((first.imported, first.albums), (99, 1));
        let n = |i: usize| match &stub.requests.lock().unwrap()[i].body {
            Some(crate::spotify::Body::Json(b)) => b["items"].as_array().unwrap().len(),
            _ => 0,
        };
        assert_eq!((n(2), n(3)), (98, 5), "the batch goes in one DELETE");
        // what a split chunk would have left (3 of 5 = 60 %): still no stray tracks
        stub.push(200, &listed());
        stub.push(200, &page(batch[2..].to_vec()));
        stub.push(200, "{}");
        let again = block(check(&st, &sp)).unwrap();
        assert_eq!((again.imported, again.skipped), (0, 3));
        let tracks: i64 = st
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM item WHERE type = 'track' AND spotify_uri LIKE 'spotify:track:%A%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tracks, 0);
    }

    /// Re-review of 74bcce6 (FR-046): a track of an album imported from the Inbox, added to the
    /// Inbox *after* that import, is a deliberate add and is imported; one with no `added_at` too.
    #[test]
    fn later_track_of_an_imported_album_is_imported() {
        let d = temp_dir("inbox-later");
        let old = "2020-01-01T09:15:30Z";
        let batch: Vec<Value> = (1..=5)
            .map(|n| entry(&format!("A{n}"), "AL1", 5, old))
            .collect();
        let stub = Stub::new(&[(200, &listed()), (200, &page(batch)), (200, "{}")]);
        let (sp, st) = (connected(&d, &stub), state(&d));
        ready(&st);
        assert_eq!(block(check(&st, &sp)).unwrap().albums, 1);
        let mut undated = entry("A3", "AL1", 5, old);
        undated["added_at"] = Value::Null;
        stub.push(200, &listed());
        stub.push(
            200,
            &page(vec![entry("A2", "AL1", 5, "2999-01-01T00:00:00Z"), undated]),
        );
        stub.push(200, "{}");
        let got = block(check(&st, &sp)).unwrap();
        assert_eq!((got.imported, got.skipped), (2, 0));
        assert_eq!(
            added_at(&json!({"added_at": "2026-10-21T09:15:30.123+02:00"})).as_deref(),
            Some("2026-10-21T07:15:30Z"),
            "normalised to created_at's format"
        );
    }

    /// A failing optional read (followed artists, scope revoked) never hides the Inbox import.
    #[test]
    fn optional_read_failure_keeps_the_import() {
        let d = temp_dir("inbox-optional");
        let e = page(vec![entry("B1", "AL2", 10, "2026-10-21T09:15:30Z")]);
        let stub = Stub::new(&[(200, &listed()), (200, &e), (200, "{}"), (403, "{}")]);
        let (sp, st) = (connected(&d, &stub), state(&d));
        ready(&st);
        sp.set_tokens(Some(crate::spotify::Tokens {
            scope: FOLLOW_SCOPE.into(),
            ..sp.tokens().unwrap()
        }))
        .unwrap();
        update(&st, |i| i.followed_artists = true).unwrap();
        assert_eq!(block(check(&st, &sp)).unwrap().imported, 1);
        assert_eq!(status(&st, &sp).imported_last, 1);
    }

    #[test]
    fn gone_inbox_is_missing_and_never_recreated() {
        let d = temp_dir("inbox-missing");
        let stub = Stub::new(&[(200, &page(vec![json!({"id": "Other00000000000000001"})]))]);
        let (sp, st) = (connected(&d, &stub), state(&d));
        ready(&st);
        let gone =
            |r: Result<Checked>| assert_eq!(r.unwrap_err().code, crate::error::Code::NotFound);
        gone(block(check(&st, &sp))); // not Ok({0,0,0}): the UI must not say "Nothing new"
        assert_eq!(status(&st, &sp).state, "missing");
        assert_eq!(stub.calls().len(), 1, "no items read, no create");
        gone(block(check(&st, &sp)));
        block(ensure_on_connect(&st, &sp)); // a recorded Inbox is never re-created on connect
        assert_eq!(stub.calls().len(), 1);
        // 404 on the items read counts as gone too
        update(&st, |i| i.missing = false).unwrap();
        stub.push(200, &listed());
        stub.push(404, "{}");
        gone(block(check(&st, &sp)));
        assert_eq!(status(&st, &sp).state, "missing");
        // Create Inbox makes a new one
        stub.push(
            201,
            &json!({"uri": format!("spotify:playlist:{}", id("New1"))}).to_string(),
        );
        let s = block(create(&st, &sp)).unwrap();
        assert_eq!(s.state, "ready");
        assert!(s.playlist_uri.unwrap().ends_with(&id("New1")));
    }

    #[test]
    fn disconnected_check_sends_nothing() {
        let d = temp_dir("inbox-off");
        let stub = Stub::new(&[]);
        let (sp, st) = (spotify(&d, &stub), state(&d));
        ready(&st);
        assert_eq!(
            block(check(&st, &sp)).unwrap_err().code,
            crate::error::Code::SpotifyAuth
        );
        assert!(stub.calls().is_empty());
    }

    #[test]
    fn saved_albums_since_the_last_check() {
        let d = temp_dir("inbox-albums");
        let albums = json!({"items": [
            {"added_at": "2026-10-21T10:00:00Z", "album": {"uri": format!("spotify:album:{}", id("N1"))}},
            {"added_at": "2026-10-20T10:00:00Z", "album": {"uri": format!("spotify:album:{}", id("O1"))}}],
            "next": "https://api.spotify.com/v1/me/albums?offset=2"});
        let stub = Stub::new(&[
            (200, &listed()),
            (200, &page(vec![])),
            (200, &albums.to_string()),
        ]);
        let (sp, st) = (connected(&d, &stub), state(&d));
        ready(&st);
        block(set_options(&st, &sp, true, false)).unwrap();
        update(&st, |i| {
            i.albums_after = Some("2026-10-21T00:00:00Z".into())
        })
        .unwrap();
        let got = block(check(&st, &sp)).unwrap();
        assert_eq!(
            got,
            Checked {
                imported: 1,
                albums: 1,
                skipped: 0
            }
        );
        assert_eq!(rows(&st)[0].1, format!("spotify:album:{}", id("N1")));
        assert_eq!(
            stub.calls().len(),
            3,
            "no DELETE for an empty Inbox, no second albums page"
        );
        assert_ne!(
            inbox(&st).albums_after.as_deref(),
            Some("2026-10-21T00:00:00Z"),
            "moved to this check"
        );
    }

    #[test]
    fn followed_artists_need_the_scope_then_diff_a_baseline() {
        let d = temp_dir("inbox-follow");
        let stub = Stub::new(&[]);
        let (sp, st) = (connected(&d, &stub), state(&d));
        ready(&st);
        sp.set_tokens(Some(crate::spotify::Tokens {
            scope: "playlist-read-private".into(),
            ..sp.tokens().unwrap()
        }))
        .unwrap();
        let s = block(set_options(&st, &sp, false, true)).unwrap();
        assert!(s.needs_reconnect, "old sign-in lacks user-follow-read");
        stub.push(200, &listed());
        stub.push(200, &page(vec![]));
        block(check(&st, &sp)).unwrap();
        assert_eq!(stub.calls().len(), 2, "no following read without the scope");

        sp.set_tokens(Some(crate::spotify::Tokens {
            scope: format!("{} {FOLLOW_SCOPE}", crate::spotify::auth::SCOPES),
            ..sp.tokens().unwrap()
        }))
        .unwrap();
        assert!(!status(&st, &sp).needs_reconnect);
        let following = |ids: &[&str]| {
            json!({"artists": {"items": ids.iter().map(|i| json!({"id": id(i)})).collect::<Vec<_>>(), "next": null}})
                .to_string()
        };
        for f in [following(&["R1", "R2"]), following(&["R1", "R2", "R3"])] {
            stub.push(200, &listed());
            stub.push(200, &page(vec![]));
            stub.push(200, &f);
        }
        assert_eq!(
            block(check(&st, &sp)).unwrap(),
            Checked::default(),
            "baseline only"
        );
        let got = block(check(&st, &sp)).unwrap();
        assert_eq!(got.imported, 1);
        assert_eq!(
            rows(&st)[0],
            (
                "artist".into(),
                format!("spotify:artist:{}", id("R3")),
                None,
                "inbox/promoted".into()
            )
        );
        assert!(
            stub.calls()
                .last()
                .unwrap()
                .1
                .contains("/me/following?type=artist")
        );
        // the baseline is Spotify data: in the cache, never in settings.json; disconnect drops it
        let saved = std::fs::read_to_string(st.paths.settings_file()).unwrap();
        assert!(!saved.contains(&id("R1")));
        block(sp.disconnect()).unwrap();
        sp.set_tokens(Some(crate::spotify::Tokens {
            scope: FOLLOW_SCOPE.into(),
            ..crate::spotify::Tokens::synthetic(3600)
        }))
        .unwrap();
        stub.push(200, &listed());
        stub.push(200, &page(vec![]));
        stub.push(200, &following(&["R1", "R2", "R3", "R9"]));
        assert_eq!(
            block(check(&st, &sp)).unwrap(),
            Checked::default(),
            "re-baselined after disconnect"
        );
        // off then on: re-baseline, nothing imported
        block(set_options(&st, &sp, false, false)).unwrap();
        block(set_options(&st, &sp, false, true)).unwrap();
        stub.push(200, &listed());
        stub.push(200, &page(vec![]));
        stub.push(200, &following(&["R1", "R2", "R3", "R4"]));
        assert_eq!(block(check(&st, &sp)).unwrap(), Checked::default());
    }
}
