//! Autofill (M5 WI 5, TSD §4.2): fields from `spotify_cache.db` (or `GET /albums|tracks|artists`),
//! nothing written to `library.db` until the user confirms via `create_item`/`update_item`.
//! `bulk_add_album_tracks` creates promoted tracks linked `on_album`.
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Spotify, cache, uri_id};
use crate::error::{AppError, Result};
use crate::library::items::{self, ItemSummary, NewItem};
use crate::library::{ItemType, LinkKind, Prov};

/// API.md `autofill` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Autofill {
    pub fields: AutofillFields,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_list: Option<Vec<TrackRef>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AutofillFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackRef {
    pub uri: String,
    pub name: String,
    pub position: i64,
}

/// `bulk_add_album_tracks` input row (`{uri, name}`); the command layer checks the URI and cleans the name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NewTrack {
    pub uri: String,
    pub name: String,
}

/// Album track lists longer than this are cut (Spotify pages hold 50).
const MAX_TRACKS: usize = 500;

/// `uri` is already checked (`spotify::check_uri`, album/track/artist). Only the trimmed result is
/// cached (7 days, source A); nothing touches `library.db` (TSD §4.2).
pub async fn autofill(sp: &Spotify, uri: &str) -> Result<Autofill> {
    if let Some(v) = sp.with_cache(|c| cache::get(c, uri, Utc::now()))?
        && let Ok(hit) = serde_json::from_value::<Autofill>(v)
    {
        return Ok(hit);
    }
    let kind = uri.split(':').nth(1).unwrap_or_default();
    let raw: Value = sp.get_json(&format!("/{kind}s/{}", uri_id(uri))).await?;
    let name = |v: &Value| v["name"].as_str().map(str::to_string);
    let year = |v: &Value| v["release_date"].as_str()?.get(..4)?.parse().ok();
    let mut out = Autofill {
        fields: AutofillFields {
            display_name: name(&raw),
            artist: name(&raw["artists"][0]),
            ..Default::default()
        },
        track_list: None,
    };
    match kind {
        "album" => {
            out.fields.year = year(&raw);
            let mut tracks: Vec<Value> = raw["tracks"]["items"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if let Some(next) = raw["tracks"]["next"].as_str()
                && tracks.len() < MAX_TRACKS
            {
                tracks.extend(
                    sp.paginate::<Value>(next, MAX_TRACKS - tracks.len())
                        .await?,
                );
            }
            out.track_list = Some(
                tracks
                    .iter()
                    .filter_map(|t| Some((t["uri"].as_str()?, name(t)?)))
                    .zip(1..)
                    .map(|((uri, name), position)| TrackRef {
                        uri: uri.into(),
                        name,
                        position,
                    })
                    .collect(),
            );
        }
        "track" => {
            out.fields.album = name(&raw["album"]);
            out.fields.year = year(&raw["album"]);
        }
        _ => out.fields.artist = None, // an artist has no artist field
    }
    let payload = serde_json::to_value(&out)?;
    sp.with_cache(|c| cache::put(c, uri, kind, &payload, Utc::now()))?;
    Ok(out)
}

/// Create each track (not already in the library by `spotify_uri`) through M3 `create_item`:
/// URI + confirmed name promoted, linked `on_album`, kept out of the backlog. A track already in
/// the library is linked `on_album` too (the user confirmed it is on this album, TSD §4.9) but
/// not returned: the result is what was added.
pub async fn bulk_add_album_tracks(
    _sp: &Spotify,
    db: &Mutex<Connection>,
    album_id: i64,
    tracks: Vec<NewTrack>,
) -> Result<Vec<ItemSummary>> {
    let mut c = db.lock().unwrap();
    if items::summary(&c, album_id)?.r#type != ItemType::Album {
        return Err(AppError::validation(
            "Tracks can only be added to an album.",
        ));
    }
    let mut out = Vec::new();
    for t in tracks {
        if let Some(existing) = items::id_by_uri(&c, &t.uri)? {
            // only a track can sit on an album; a URI held by another type is left alone
            if items::summary(&c, existing)?.r#type == ItemType::Track {
                crate::library::links::link(&c, existing, album_id, LinkKind::OnAlbum)?;
            }
            continue;
        }
        let d = items::create(
            &mut c,
            NewItem {
                r#type: ItemType::Track,
                display_name: Some(t.name),
                spotify_uri: Some(t.uri),
                status: None,
                priority: None,
                in_backlog: Some(false),
                tags: vec![],
                recommended_by: None,
                provenance: Prov::Promoted,
                promoted_fields: vec!["display_name".into(), "spotify_uri".into()],
                capture_ms: None,
                artist_name: None,
                notes: None,
                album_id: Some(album_id),
            },
        )?;
        out.push(d.summary);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::items::{ItemPatch, get, update};
    use crate::spotify::testing::{Stub, block, connected, temp_dir};

    const ALBUM: &str = "spotify:album:AAAAAAAAAAAAAAAAAAAAAA";
    const T1: &str = "spotify:track:TTTTTTTTTTTTTTTTTTTT01";
    const T2: &str = "spotify:track:TTTTTTTTTTTTTTTTTTTT02";

    fn album_json() -> String {
        format!(
            r#"{{"name":"Synthetic Album","release_date":"2019-04-02","artists":[{{"name":"Synthetic Quartet"}}],
               "tracks":{{"items":[{{"uri":"{T1}","name":"One"}},{{"uri":"{T2}","name":"Two"}}],"next":null}}}}"#
        )
    }

    fn library(d: &std::path::Path) -> Mutex<Connection> {
        Mutex::new(crate::db::open(&d.join("library.db")).unwrap())
    }

    fn changes(db: &Mutex<Connection>) -> String {
        db.lock()
            .unwrap()
            .query_row(
                "SELECT (SELECT count(*) FROM item) || '/' || (SELECT count(*) FROM field_provenance)",
                [],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn album_autofill_fetches_once_then_reads_cache_and_writes_no_library_row() {
        let d = temp_dir("autofill-album");
        let stub = Stub::new(&[(200, &album_json())]);
        let sp = connected(&d, &stub);
        let db = library(&d);
        let before = changes(&db);
        let a = block(autofill(&sp, ALBUM)).unwrap();
        assert_eq!(a.fields.display_name.as_deref(), Some("Synthetic Album"));
        assert_eq!(a.fields.artist.as_deref(), Some("Synthetic Quartet"));
        assert_eq!(a.fields.year, Some(2019));
        let tl = a.track_list.clone().unwrap();
        assert_eq!(
            (tl[1].uri.as_str(), tl[1].name.as_str(), tl[1].position),
            (T2, "Two", 2)
        );
        assert_eq!(
            stub.calls(),
            vec![(
                "GET".into(),
                format!("https://api.spotify.com/v1/albums/{}", uri_id(ALBUM))
            )]
        );
        // Second call: cache hit (no response left in the stub would error).
        assert_eq!(block(autofill(&sp, ALBUM)).unwrap(), a);
        assert_eq!(stub.calls().len(), 1);
        assert_eq!(changes(&db), before, "autofill must not write library.db");
    }

    #[test]
    fn track_and_artist_fields() {
        let d = temp_dir("autofill-kinds");
        let stub = Stub::new(&[
            (
                200,
                r#"{"name":"One","artists":[{"name":"Synthetic Quartet"}],"album":{"name":"Synthetic Album","release_date":"2019"}}"#,
            ),
            (200, r#"{"name":"Synthetic Quartet"}"#),
        ]);
        let sp = connected(&d, &stub);
        let t = block(autofill(&sp, T1)).unwrap();
        assert_eq!(t.fields.album.as_deref(), Some("Synthetic Album"));
        assert_eq!((t.fields.year, t.track_list), (Some(2019), None));
        let a = block(autofill(&sp, "spotify:artist:RRRRRRRRRRRRRRRRRRRRRR")).unwrap();
        assert_eq!(
            a.fields,
            AutofillFields {
                display_name: Some("Synthetic Quartet".into()),
                ..Default::default()
            }
        );
    }

    /// BRD §13.3 / TSD §4.2: unconfirmed cached fields vanish with the cache; confirmed ones
    /// (create/update with `promoted_fields`) live on in `item` as `promoted`.
    #[test]
    fn promotion_rule_confirmed_persists_unconfirmed_expires() {
        let d = temp_dir("autofill-promote");
        let stub = Stub::new(&[(200, &album_json())]);
        let sp = connected(&d, &stub);
        let db = library(&d);
        let a = block(autofill(&sp, ALBUM)).unwrap();
        // The user confirms only the name; artist and year stay unconfirmed in the cache.
        let id = {
            let mut c = db.lock().unwrap();
            let mut n = crate::library::testutil::new(
                ItemType::Album,
                a.fields.display_name.as_deref().unwrap(),
            );
            n.spotify_uri = Some(ALBUM.into());
            n.promoted_fields = vec!["display_name".into()];
            let id = items::create(&mut c, n).unwrap().summary.id;
            // …later confirms the artist on the edit screen.
            let p = ItemPatch {
                artist_name: Some(a.fields.artist.clone()),
                promoted_fields: vec!["artist".into()],
                ..Default::default()
            };
            update(&mut c, id, p).unwrap();
            id
        };
        let later = Utc::now() + chrono::Duration::days(cache::CACHE_DAYS + 1);
        sp.with_cache(|c| cache::sweep(c, later)).unwrap();
        assert_eq!(
            sp.with_cache(|c| cache::get(c, ALBUM, Utc::now())).unwrap(),
            None
        );
        let it = get(&db.lock().unwrap(), id).unwrap();
        assert_eq!(it.summary.display_name.as_deref(), Some("Synthetic Album"));
        assert_eq!(
            it.summary.artist_names,
            vec!["Synthetic Quartet".to_string()]
        );
        assert_eq!(it.provenance["display_name"], Prov::Promoted);
        assert_eq!(it.provenance["artist"], Prov::Promoted);
        // The unconfirmed year never reached library.db.
        let year_cols: i64 = db
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM pragma_table_info('item') WHERE name LIKE '%year%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(year_cols, 0);
    }

    #[test]
    fn bulk_add_creates_promoted_tracks_on_album_and_skips_known_uris() {
        let d = temp_dir("autofill-bulk");
        let stub = Stub::new(&[]);
        let sp = connected(&d, &stub);
        let db = library(&d);
        let album = crate::library::testutil::add(
            &mut db.lock().unwrap(),
            ItemType::Album,
            "Synthetic Album",
        );
        let row = |uri: &str, name: &str| NewTrack {
            uri: uri.into(),
            name: name.into(),
        };
        let got = block(bulk_add_album_tracks(&sp, &db, album, vec![row(T1, "One")])).unwrap();
        assert_eq!(got.len(), 1);
        let got = block(bulk_add_album_tracks(
            &sp,
            &db,
            album,
            vec![row(T1, "One"), row(T2, "Two")],
        ))
        .unwrap();
        assert_eq!(got.len(), 1, "T1 already in the library");
        // a track saved earlier, outside any album, joins the album's track list
        let loose = {
            let mut c = db.lock().unwrap();
            let mut n = crate::library::testutil::new(ItemType::Track, "Three");
            n.spotify_uri = Some("spotify:track:TTTTTTTTTTTTTTTTTTTT03".into());
            items::create(&mut c, n).unwrap().summary.id
        };
        let none = block(bulk_add_album_tracks(
            &sp,
            &db,
            album,
            vec![row("spotify:track:TTTTTTTTTTTTTTTTTTTT03", "Three")],
        ))
        .unwrap();
        assert!(none.is_empty());
        let list = get(&db.lock().unwrap(), album).unwrap().track_list.unwrap();
        assert!(
            list.iter().any(|t| t.item_id == Some(loose)),
            "existing track not linked on_album"
        );
        let t = &got[0];
        assert_eq!(
            (t.spotify_uri.as_deref(), t.display_name.as_deref()),
            (Some(T2), Some("Two"))
        );
        assert_eq!(
            (t.album_name.as_deref(), t.in_backlog),
            (Some("Synthetic Album"), false)
        );
        let det = get(&db.lock().unwrap(), t.id).unwrap();
        assert_eq!(det.provenance["display_name"], Prov::Promoted);
        assert_eq!(det.provenance["spotify_uri"], Prov::Promoted);
        assert!(stub.calls().is_empty(), "bulk add needs no network");
        // Not an album → validation, nothing created.
        let err = block(bulk_add_album_tracks(
            &sp,
            &db,
            t.id,
            vec![row("spotify:track:TTTTTTTTTTTTTTTTTTTT03", "Three")],
        ))
        .unwrap_err();
        assert_eq!(err.code, crate::error::Code::Validation);
    }
}
