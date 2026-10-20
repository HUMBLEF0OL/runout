//! Playlist from a saved filter or from item ids (M5 WI 4, FR-039): resolve to track URIs (skip
//! items without one, count them; albums expand to their tracks), `POST /me/playlists`, then
//! `POST /playlists/{id}/items` in chunks of 100. Reads only `item` rows (type + URI).
use std::sync::Mutex;

use reqwest::Method;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Spotify, check_uri, uri_id};
use crate::error::{AppError, Result};
use crate::library::filters::{ItemFilter, MAX_LIMIT, list_items, list_saved_filters};

/// Spotify's max URIs per add-items request.
const CHUNK: usize = 100;
/// Spotify's playlist size cap; also bounds album expansion.
const MAX_TRACKS: usize = 10_000;

/// API.md `create_playlist_from_filter` / `create_playlist_from_items` result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreatedPlaylist {
    pub playlist_uri: String,
    pub added: i64,
    pub skipped_no_uri: i64,
}

/// `name` is already cleaned (`text::clean_line`, `limit::PLAYLIST`) and non-empty.
pub async fn create_playlist_from_filter(
    sp: &Spotify,
    db: &Mutex<Connection>,
    filter_id: i64,
    name: &str,
    public: bool,
) -> Result<CreatedPlaylist> {
    let rows = {
        let c = db.lock().map_err(|_| AppError::internal("library lock"))?;
        let f = list_saved_filters(&c)?
            .into_iter()
            .find(|f| f.id == filter_id)
            .ok_or_else(|| AppError::not_found("That saved filter no longer exists."))?;
        let def = ItemFilter {
            offset: None,
            limit: Some(MAX_LIMIT),
            ..f.definition
        };
        list_items(&c, &def)?
            .rows
            .into_iter()
            .map(|i| (i.r#type.as_str().to_string(), i.spotify_uri))
            .collect::<Vec<_>>()
    };
    create(sp, rows, name, public).await
}

/// Albums add all their tracks (API.md). Unknown ids count as skipped.
pub async fn create_playlist_from_items(
    sp: &Spotify,
    db: &Mutex<Connection>,
    item_ids: &[i64],
    name: &str,
    public: bool,
) -> Result<CreatedPlaylist> {
    let rows = {
        let c = db.lock().map_err(|_| AppError::internal("library lock"))?;
        let mut st = c.prepare("SELECT type, spotify_uri FROM item WHERE id = ?1")?;
        let mut rows = Vec::with_capacity(item_ids.len());
        for id in item_ids {
            let r = st
                .query_row([id], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))
                .optional()?;
            rows.push(r.unwrap_or_else(|| (String::new(), None)));
        }
        rows
    };
    create(sp, rows, name, public).await
}

#[derive(Deserialize)]
struct Uri {
    uri: String,
}

/// `(item type, spotify_uri)` rows → track URIs (deduped, in order) → new playlist.
async fn create(
    sp: &Spotify,
    rows: Vec<(String, Option<String>)>,
    name: &str,
    public: bool,
) -> Result<CreatedPlaylist> {
    let mut uris: Vec<String> = vec![];
    let mut skipped = 0i64;
    let push = |u: String, uris: &mut Vec<String>| {
        if !uris.contains(&u) {
            uris.push(u);
        }
    };
    for (kind, uri) in rows {
        // Artists can't be playlist entries, so they count as "no usable link" too.
        match (kind.as_str(), uri) {
            ("track", Some(u)) if check_uri(&u, &["track"]).is_ok() => push(u, &mut uris),
            ("album", Some(u)) if check_uri(&u, &["album"]).is_ok() => {
                let first = format!("/albums/{}/tracks?limit=50", uri_id(&u));
                for t in sp.paginate::<Uri>(&first, MAX_TRACKS).await? {
                    push(t.uri, &mut uris);
                }
            }
            _ => skipped += 1,
        }
    }
    uris.truncate(MAX_TRACKS);

    let pl = sp
        .send_json(
            Method::POST,
            "/me/playlists",
            Some(json!({ "name": name, "public": public })),
        )
        .await?;
    let (Some(id), Some(playlist_uri)) = (pl["id"].as_str(), pl["uri"].as_str()) else {
        return Err(AppError::internal(
            "Spotify didn't return the new playlist.",
        ));
    };
    // The playlist exists from here on: a failed chunk keeps its code (busy, offline…) but says
    // the playlist is there and how full, with its URI in `detail`, so the UI can link to it
    // and a retry isn't blind (it would create a second one).
    for (n, chunk) in uris.chunks(CHUNK).enumerate() {
        let sent = sp
            .send_json(
                Method::POST,
                &format!("/playlists/{}/items", super::client::enc(id)),
                Some(json!({ "uris": chunk })),
            )
            .await;
        if let Err(e) = sent {
            let added = n * CHUNK;
            return Err(AppError {
                message: format!(
                    "The playlist was created on Spotify, but only {added} of {} tracks were added. {}",
                    uris.len(),
                    e.message
                ),
                detail: Some(playlist_uri.to_string()),
                ..e
            });
        }
    }
    Ok(CreatedPlaylist {
        playlist_uri: playlist_uri.to_string(),
        added: uris.len() as i64,
        skipped_no_uri: skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spotify::Body;
    use crate::spotify::client::API;
    use crate::spotify::testing::*;

    const PL: &str =
        r#"{"id":"PlaylistId000000000000","uri":"spotify:playlist:PlaylistId000000000000"}"#;

    fn id22(n: usize) -> String {
        format!("{n:0>22}")
    }

    fn lib(d: &std::path::Path) -> Mutex<Connection> {
        Mutex::new(crate::db::open(&d.join("library.db")).unwrap())
    }

    fn add(db: &Mutex<Connection>, kind: &str, uri: Option<String>) -> i64 {
        let c = db.lock().unwrap();
        c.query_row(
            "INSERT INTO item(type, display_name, spotify_uri) VALUES (?1, 'Synthetic', ?2) RETURNING id",
            rusqlite::params![kind, uri],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn added_uris(stub: &Stub) -> Vec<Vec<String>> {
        stub.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.url.ends_with("/items"))
            .map(|r| match &r.body {
                Some(Body::Json(v)) => serde_json::from_value(v["uris"].clone()).unwrap(),
                _ => panic!("json body"),
            })
            .collect()
    }

    #[test]
    fn items_skip_missing_uris_and_artists() {
        let d = temp_dir("create-skip");
        let db = lib(&d);
        let t1 = add(&db, "track", Some(format!("spotify:track:{}", id22(1))));
        let t2 = add(&db, "track", None);
        let a = add(&db, "artist", Some(format!("spotify:artist:{}", id22(2))));
        let stub = Stub::new(&[(201, PL), (201, r#"{"snapshot_id":"s"}"#)]);
        let sp = connected(&d, &stub);
        let r = block(create_playlist_from_items(
            &sp,
            &db,
            &[t1, t2, a, 9999, t1],
            "Mix",
            false,
        ))
        .unwrap();
        assert_eq!(r.added, 1);
        assert_eq!(r.skipped_no_uri, 3);
        assert_eq!(r.playlist_uri, "spotify:playlist:PlaylistId000000000000");
        let calls = stub.calls();
        assert_eq!(calls[0], ("POST".into(), format!("{API}/me/playlists")));
        match &stub.requests.lock().unwrap()[0].body {
            Some(Body::Json(v)) => assert_eq!(v, &json!({"name":"Mix","public":false})),
            _ => panic!("json body"),
        }
        assert_eq!(
            added_uris(&stub),
            vec![vec![format!("spotify:track:{}", id22(1))]]
        );
    }

    #[test]
    fn adds_in_chunks_of_100() {
        let d = temp_dir("create-chunk");
        let db = lib(&d);
        let ids: Vec<i64> = (0..250)
            .map(|n| add(&db, "track", Some(format!("spotify:track:{}", id22(n)))))
            .collect();
        let stub = Stub::new(&[(201, PL), (201, "{}"), (201, "{}"), (201, "{}")]);
        let sp = connected(&d, &stub);
        let r = block(create_playlist_from_items(&sp, &db, &ids, "Big", true)).unwrap();
        assert_eq!((r.added, r.skipped_no_uri), (250, 0));
        let sizes: Vec<usize> = added_uris(&stub).iter().map(Vec::len).collect();
        assert_eq!(sizes, vec![100, 100, 50]);
        assert_eq!(
            stub.calls()[1].1,
            format!("{API}/playlists/PlaylistId000000000000/items")
        );
    }

    #[test]
    fn filter_expands_albums_to_tracks() {
        let d = temp_dir("create-album");
        let db = lib(&d);
        let album = format!("spotify:album:{}", id22(7));
        add(&db, "album", Some(album.clone()));
        add(&db, "track", None);
        let fid = {
            let c = db.lock().unwrap();
            crate::library::filters::save_filter(&c, "All", ItemFilter::default())
                .unwrap()
                .id
        };
        let next = format!("{API}/albums/{}/tracks?offset=2", id22(7));
        let p1 = format!(
            r#"{{"items":[{{"uri":"spotify:track:{}"}},{{"uri":"spotify:track:{}"}}],"next":"{next}"}}"#,
            id22(1),
            id22(2)
        );
        let p2 = format!(
            r#"{{"items":[{{"uri":"spotify:track:{}"}}],"next":null}}"#,
            id22(3)
        );
        let stub = Stub::new(&[(200, &p1), (200, &p2), (201, PL), (201, "{}")]);
        let sp = connected(&d, &stub);
        let r = block(create_playlist_from_filter(&sp, &db, fid, "Albums", false)).unwrap();
        assert_eq!((r.added, r.skipped_no_uri), (3, 1));
        assert_eq!(
            stub.calls()[0].1,
            format!("{API}/albums/{}/tracks?limit=50", id22(7))
        );
        assert_eq!(added_uris(&stub)[0].len(), 3);
    }

    /// A chunk fails after the playlist exists: the error names it (URI in detail) and how many
    /// tracks landed, and keeps the failure's code.
    #[test]
    fn failed_chunk_returns_the_created_playlist() {
        let d = temp_dir("create-partial");
        let db = lib(&d);
        let ids: Vec<i64> = (0..150)
            .map(|n| add(&db, "track", Some(format!("spotify:track:{}", id22(n)))))
            .collect();
        let stub = Stub::new(&[(201, PL), (201, "{}"), (503, "")]);
        let sp = connected(&d, &stub);
        let e = block(create_playlist_from_items(&sp, &db, &ids, "Big", true)).unwrap_err();
        assert_eq!(e.code, crate::error::Code::Internal);
        assert_eq!(
            e.detail.as_deref(),
            Some("spotify:playlist:PlaylistId000000000000")
        );
        assert!(e.message.contains("100 of 150"), "{}", e.message);
    }

    #[test]
    fn unknown_filter_is_not_found_and_sends_nothing() {
        let d = temp_dir("create-nofilter");
        let db = lib(&d);
        let stub = Stub::new(&[]);
        let sp = connected(&d, &stub);
        assert!(block(create_playlist_from_filter(&sp, &db, 42, "X", false)).is_err());
        assert!(stub.calls().is_empty());
    }
}
