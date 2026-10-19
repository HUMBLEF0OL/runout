//! Account data (TSD §4.8, FR-033): `Playlist*.json` → `exported_playlist*`, replaced by name on a
//! newer export (exports carry no playlist id). Read-only check-up: duplicates, low-rated, never played.
use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::matching::Matcher;
use super::normalize::norm_key;
use super::source::{File, unrecognized};
use crate::error::{AppError, Result};
use crate::text::{clean_line, limit};

/// Low-rated track: rating ≤ 2★ (TSD §4.13).
pub const LOW_RATING: f64 = 2.0;

// ⚑ T1: field names unverified until U-2 (WI 4). Only these keys are read; everything else
// (descriptions, followers, collaborators, episodes, local files) is ignored.
#[derive(Deserialize)]
struct FileBody {
    playlists: Vec<RawPlaylist>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlaylist {
    name: String,
    last_modified_date: Option<String>,
    #[serde(default)]
    items: Vec<RawItem>,
}
#[derive(Deserialize)]
struct RawItem {
    track: Option<RawTrack>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTrack {
    track_name: Option<String>,
    artist_name: Option<String>,
    album_name: Option<String>,
    track_uri: Option<String>,
}

pub struct Track {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub uri: Option<String>,
}

pub struct Playlist {
    pub name: String,
    pub last_modified: Option<String>,
    pub tracks: Vec<Track>,
}

pub fn parse(files: &[File]) -> Result<Vec<Playlist>> {
    let line = |s: Option<String>| clean_line(&s.unwrap_or_default(), limit::NAME);
    let mut out = Vec::new();
    for f in files {
        let body: FileBody = serde_json::from_slice(&f.bytes).map_err(|e| {
            unrecognized("This file isn't a Spotify playlist file.")
                .with_detail(format!("{}: {e}", f.name))
        })?;
        for p in body.playlists {
            let tracks = p.items.into_iter().filter_map(|i| i.track).map(|t| Track {
                name: line(t.track_name),
                artist: line(t.artist_name),
                album: line(t.album_name),
                uri: t.track_uri.filter(|u| u.starts_with("spotify:track:")),
            });
            out.push(Playlist {
                name: clean_line(&p.name, limit::NAME),
                last_modified: p.last_modified_date.map(|d| clean_line(&d, 40)),
                tracks: tracks.collect(),
            });
        }
    }
    Ok(out)
}

/// Replace every stored playlist that shares a name with this export, then insert them all.
pub fn store(conn: &Connection, batch_id: i64, lists: &[Playlist]) -> Result<usize> {
    let names: HashSet<&str> = lists.iter().map(|p| p.name.as_str()).collect();
    for n in names {
        conn.execute("DELETE FROM exported_playlist WHERE name = ?1", [n])?;
    }
    let mut item = conn.prepare(
        "INSERT INTO exported_playlist_item(playlist_id, position, track_name, artist_name,
           album_name, spotify_track_uri) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for p in lists {
        conn.execute(
            "INSERT INTO exported_playlist(import_batch_id, name, last_modified) VALUES (?1, ?2, ?3)",
            params![batch_id, p.name, p.last_modified],
        )?;
        let id = conn.last_insert_rowid();
        for (i, t) in p.tracks.iter().enumerate() {
            item.execute(params![id, i as i64 + 1, t.name, t.artist, t.album, t.uri])?;
        }
    }
    Ok(lists.len())
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ExportedPlaylist {
    pub id: i64,
    pub name: String,
    pub tracks: usize,
    pub dupes: usize,
    pub low: usize,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CheckRow {
    pub position: i64,
    pub name: String,
    pub artist: String,
    pub dupe: bool,
    pub rating: Option<f64>,
    pub never_played: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PlaylistCheckup {
    pub tracks: Vec<CheckRow>,
}

fn rows(conn: &Connection, m: &Matcher, id: i64) -> Result<Vec<CheckRow>> {
    let mut st = conn.prepare(
        "SELECT position, track_name, artist_name, spotify_track_uri FROM exported_playlist_item
         WHERE playlist_id = ?1 ORDER BY position",
    )?;
    let mut item = conn.prepare("SELECT rating FROM item WHERE id = ?1")?;
    let mut tags = conn.prepare(
        "SELECT t.name FROM item_tag it JOIN tag t ON t.id = it.tag_id WHERE it.item_id = ?1 ORDER BY t.name",
    )?;
    let mut played = conn.prepare(
        "SELECT EXISTS (SELECT 1 FROM listening_event WHERE counts_as_play
           AND (spotify_track_uri = ?1 OR item_id = ?2))",
    )?;
    let raw = st.query_map([id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for r in raw {
        let (position, name, artist, uri) = r?;
        let key = uri.clone().unwrap_or_else(|| norm_key(&artist, &name));
        let item_id = m.find(uri.as_deref(), &name, &artist);
        let (rating, tag_names) = match item_id {
            Some(i) => (
                item.query_row([i], |r| r.get(0)).optional()?.flatten(),
                tags.query_map([i], |r| r.get(0))?
                    .collect::<rusqlite::Result<Vec<String>>>()?,
            ),
            None => (None, vec![]),
        };
        let played: bool = played.query_row(params![uri, item_id], |r| r.get(0))?;
        out.push(CheckRow {
            position,
            name,
            artist,
            dupe: !seen.insert(key),
            rating,
            never_played: !played,
            tags: tag_names,
        });
    }
    Ok(out)
}

pub fn list(conn: &Connection) -> Result<Vec<ExportedPlaylist>> {
    let m = Matcher::load(conn)?;
    let mut st =
        conn.prepare("SELECT id, name FROM exported_playlist ORDER BY name COLLATE NOCASE, id")?;
    let lists = st
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let low = |r: &CheckRow| r.rating.is_some_and(|x| x <= LOW_RATING);
    lists
        .into_iter()
        .map(|(id, name)| {
            let rs = rows(conn, &m, id)?;
            Ok(ExportedPlaylist {
                id,
                name,
                tracks: rs.len(),
                dupes: rs.iter().filter(|r| r.dupe).count(),
                low: rs.iter().filter(|r| low(r)).count(),
            })
        })
        .collect()
}

pub fn checkup(conn: &Connection, id: i64) -> Result<PlaylistCheckup> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM exported_playlist WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(AppError::not_found(
            "That playlist isn't in your imported data.",
        ));
    }
    let m = Matcher::load(conn)?;
    Ok(PlaylistCheckup {
        tracks: rows(conn, &m, id)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::importer::source::{self, fixture};

    #[test]
    fn stores_by_name_and_checks_up() {
        let c = db::open_in_memory().unwrap();
        let lists = parse(&source::read(&fixture()).unwrap().playlists).unwrap();
        // episode and local-file items are not tracks
        assert_eq!(
            lists.iter().map(|p| p.tracks.len()).collect::<Vec<_>>(),
            [4, 0]
        );
        c.execute_batch(
            "INSERT INTO import_batch(id, kind, checksum) VALUES (1, 'account_data', 'a'), (2, 'account_data', 'b')",
        )
        .unwrap();
        assert_eq!(store(&c, 1, &lists).unwrap(), 2);
        assert_eq!(
            store(&c, 2, &lists).unwrap(),
            2,
            "newer export replaces by name"
        );
        let n: i64 = c
            .query_row("SELECT count(*) FROM exported_playlist", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);

        // Glass Harbor: in library rated 1.5 + tagged, played; Quiet Engine: never played
        let gh = &lists[0].tracks[0];
        c.execute(
            "INSERT INTO item(id, type, display_name, spotify_uri, rating) VALUES (1, 'track', ?1, ?2, 1.5)",
            params![gh.name, gh.uri],
        )
        .unwrap();
        c.execute_batch(
            "INSERT INTO tag(id, name) VALUES (1, 'calm'); INSERT INTO item_tag VALUES (1, 1);",
        )
        .unwrap();
        c.execute(
            "INSERT INTO listening_event(source, ended_at_utc, ms_played, item_id, dedupe_hash)
             VALUES ('logger', '2026-01-01T00:00:00Z', 31000, 1, 'h')",
            [],
        )
        .unwrap();
        let all = list(&c).unwrap();
        let mix = all.iter().find(|p| p.name == "Fixture Mix").unwrap();
        assert_eq!((mix.tracks, mix.dupes, mix.low), (4, 1, 2));
        let t = checkup(&c, mix.id).unwrap().tracks;
        let flags: Vec<_> = t
            .iter()
            .map(|r| (r.dupe, r.rating, r.never_played))
            .collect();
        assert_eq!(
            flags,
            [
                (false, Some(1.5), false),
                (false, None, true),
                (true, Some(1.5), false),
                (false, None, true)
            ]
        );
        assert_eq!(t[0].tags, ["calm"]);
        assert_eq!(
            checkup(&c, 999).err().unwrap().code,
            crate::error::Code::NotFound
        );
    }
}
