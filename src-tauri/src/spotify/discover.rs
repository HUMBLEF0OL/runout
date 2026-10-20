//! Top items and recently played (M5 WI 6): display only, served through the cache, never written
//! to `library.db`; `in_library_id` by URI lookup.
use std::sync::Mutex;

use chrono::{Duration, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Spotify, cache};
use crate::error::Result;

/// API.md `SpotifyEntry`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpotifyEntry {
    pub uri: String,
    pub name: String,
    pub artist: String,
    pub in_library_id: Option<i64>,
}

/// API.md `get_recent_plays_api` row: `SpotifyEntry & {played_at}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentPlay {
    #[serde(flatten)]
    pub entry: SpotifyEntry,
    pub played_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopKind {
    Tracks,
    Artists,
}

/// `'4w' | '6m' | '1y'` → Spotify `short_term | medium_term | long_term`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum TopRange {
    #[serde(rename = "4w")]
    W4,
    #[serde(rename = "6m")]
    M6,
    #[serde(rename = "1y")]
    Y1,
}

impl TopRange {
    fn term(self) -> &'static str {
        match self {
            TopRange::W4 => "short_term",
            TopRange::M6 => "medium_term",
            TopRange::Y1 => "long_term",
        }
    }
}

/// Freshness for display lists; the cache row itself still expires after `CACHE_DAYS`.
const TOP_TTL_MIN: i64 = 60;
const RECENT_TTL_MIN: i64 = 5;

/// Rows under `key` if fetched within `ttl_min`, else fetch `path`, keep only what `map` picks
/// (the displayed fields, BRD S8 minimisation) and cache that, never the raw response.
async fn cached<T: Serialize + DeserializeOwned>(
    sp: &Spotify,
    key: &str,
    ttl_min: i64,
    path: &str,
    map: impl Fn(&Value) -> T,
) -> Result<T> {
    let now = Utc::now();
    // ponytail: rows expire at fetched_at + CACHE_DAYS, so "fetched within ttl" = still valid at
    // now + (CACHE_DAYS - ttl). Avoids adding a fetched_at reader to cache.rs.
    let horizon = now + Duration::days(cache::CACHE_DAYS) - Duration::minutes(ttl_min);
    if let Some(v) = sp.with_cache(|c| cache::get(c, key, horizon))?
        && let Ok(hit) = serde_json::from_value(v)
    {
        return Ok(hit);
    }
    let rows = map(&sp.request(reqwest::Method::GET, path, None).await?);
    let v = serde_json::to_value(&rows)?;
    sp.with_cache(|c| cache::put(c, key, "list", &v, now))?;
    Ok(rows)
}

/// `{uri, name, artist}` from a Spotify track or artist object (an artist's `artist` is its name).
fn entry(o: &Value) -> Option<SpotifyEntry> {
    let name = o["name"].as_str()?.to_string();
    let artist = match o["artists"].as_array() {
        Some(a) => a
            .iter()
            .filter_map(|x| x["name"].as_str())
            .collect::<Vec<_>>()
            .join(", "),
        None => name.clone(),
    };
    Some(SpotifyEntry {
        uri: o["uri"].as_str()?.to_string(),
        name,
        artist,
        in_library_id: None,
    })
}

/// Fill `in_library_id` by `item.spotify_uri` (read only; lock held briefly, after the fetch).
fn link<'a>(db: &Mutex<Connection>, es: impl Iterator<Item = &'a mut SpotifyEntry>) -> Result<()> {
    let c = db.lock().unwrap_or_else(|e| e.into_inner());
    let mut q = c.prepare_cached("SELECT id FROM item WHERE spotify_uri = ?1")?;
    for e in es {
        e.in_library_id = q.query_row([&e.uri], |r| r.get(0)).optional()?;
    }
    Ok(())
}

pub async fn get_top_items(
    sp: &Spotify,
    db: &Mutex<Connection>,
    kind: TopKind,
    range: TopRange,
) -> Result<Vec<SpotifyEntry>> {
    let t = match kind {
        TopKind::Tracks => "tracks",
        TopKind::Artists => "artists",
    };
    let key = format!("spotify:top:{t}:{}", range.term());
    let path = format!("/me/top/{t}?time_range={}&limit=50", range.term());
    let mut out: Vec<SpotifyEntry> = cached(sp, &key, TOP_TTL_MIN, &path, |v| {
        v["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(entry)
            .collect()
    })
    .await?;
    link(db, out.iter_mut())?;
    Ok(out)
}

pub async fn get_recent_plays_api(sp: &Spotify, db: &Mutex<Connection>) -> Result<Vec<RecentPlay>> {
    let mut out: Vec<RecentPlay> = cached(
        sp,
        "spotify:recent:me",
        RECENT_TTL_MIN,
        "/me/player/recently-played?limit=50",
        |v| {
            v["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|it| {
                    Some(RecentPlay {
                        entry: entry(&it["track"])?,
                        played_at: it["played_at"].as_str()?.to_string(),
                    })
                })
                .collect()
        },
    )
    .await?;
    link(db, out.iter_mut().map(|r| &mut r.entry))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spotify::testing::*;
    use serde_json::json;

    const A: &str = "spotify:track:AAAAAAAAAAAAAAAAAAAAAA";
    const B: &str = "spotify:track:BBBBBBBBBBBBBBBBBBBBBB";

    fn tracks() -> String {
        json!({"items": [
            {"uri": A, "name": "Synthetic One", "artists": [{"name": "Test Band"}, {"name": "Guest"}], "popularity": 42, "album": {"name": "Raw Album"}},
            {"uri": B, "name": "Synthetic Two", "artists": [{"name": "Test Band"}]}
        ], "next": null})
        .to_string()
    }

    fn library(d: &std::path::Path) -> Mutex<Connection> {
        let c = crate::db::open(&d.join("library.db")).unwrap();
        c.execute(
            "INSERT INTO item(id, type, display_name, spotify_uri) VALUES (7, 'track', 'Mine', ?1)",
            [A],
        )
        .unwrap();
        Mutex::new(c)
    }

    /// Row count of every library table: equal before/after = nothing written.
    fn counts(db: &Mutex<Connection>) -> String {
        let c = db.lock().unwrap();
        let tables: Vec<String> = c
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        tables
            .iter()
            .map(|t| {
                let n: i64 = c
                    .query_row(&format!("SELECT count(*) FROM \"{t}\""), [], |r| r.get(0))
                    .unwrap_or(-1);
                format!("{t}={n}")
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn ranges_map_to_spotify_terms() {
        let d = temp_dir("disc-range");
        let db = library(&d);
        for (range, term) in [
            (TopRange::W4, "short_term"),
            (TopRange::M6, "medium_term"),
            (TopRange::Y1, "long_term"),
        ] {
            let stub = Stub::new(&[(200, r#"{"items":[]}"#)]);
            let sp = connected(&d.join(term), &stub);
            block(get_top_items(&sp, &db, TopKind::Artists, range)).unwrap();
            assert_eq!(
                stub.calls()[0].1,
                format!("https://api.spotify.com/v1/me/top/artists?time_range={term}&limit=50")
            );
        }
    }

    #[test]
    fn top_tracks_link_library_and_are_cached() {
        let d = temp_dir("disc-top");
        let db = library(&d);
        let stub = Stub::new(&[(200, &tracks())]);
        let sp = connected(&d, &stub);
        let before = counts(&db);
        let out = block(get_top_items(&sp, &db, TopKind::Tracks, TopRange::M6)).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].artist, "Test Band, Guest");
        assert_eq!(out[0].in_library_id, Some(7));
        assert_eq!(out[1].in_library_id, None);
        // Second call comes from spotify_cache.db: the stub has no response left, none is made.
        let again = block(get_top_items(&sp, &db, TopKind::Tracks, TopRange::M6)).unwrap();
        assert_eq!(again, out);
        assert_eq!(stub.calls().len(), 1);
        assert_eq!(counts(&db), before, "nothing written to library.db");
        let stored = sp
            .with_cache(|c| cache::get(c, "spotify:top:tracks:medium_term", Utc::now()))
            .unwrap()
            .unwrap()
            .to_string();
        assert!(stored.contains("Synthetic One"));
        assert!(
            !stored.contains("popularity") && !stored.contains("Raw Album"),
            "raw response cached: {stored}"
        );
    }

    #[test]
    fn artists_use_their_own_name() {
        let d = temp_dir("disc-art");
        let db = library(&d);
        let body = json!({"items": [{"uri": "spotify:artist:CCCCCCCCCCCCCCCCCCCCCC", "name": "Test Band"}]});
        let stub = Stub::new(&[(200, &body.to_string())]);
        let sp = connected(&d, &stub);
        let out = block(get_top_items(&sp, &db, TopKind::Artists, TopRange::W4)).unwrap();
        assert_eq!(
            (out[0].name.as_str(), out[0].artist.as_str()),
            ("Test Band", "Test Band")
        );
    }

    #[test]
    fn recent_plays_carry_played_at_and_never_touch_library() {
        let d = temp_dir("disc-recent");
        let db = library(&d);
        let body = json!({"items": [
            {"track": {"uri": A, "name": "Synthetic One", "artists": [{"name": "Test Band"}]},
             "played_at": "2026-10-18T12:00:00.000Z"}
        ]});
        let stub = Stub::new(&[(200, &body.to_string())]);
        let sp = connected(&d, &stub);
        let before = counts(&db);
        let out = block(get_recent_plays_api(&sp, &db)).unwrap();
        assert_eq!(
            stub.calls()[0].1,
            "https://api.spotify.com/v1/me/player/recently-played?limit=50"
        );
        assert_eq!(out[0].played_at, "2026-10-18T12:00:00.000Z");
        assert_eq!(out[0].entry.in_library_id, Some(7));
        assert_eq!(counts(&db), before, "nothing written to library.db");
    }

    #[test]
    fn stale_cache_refetches() {
        let d = temp_dir("disc-stale");
        let db = library(&d);
        let stub = Stub::new(&[(200, &tracks())]);
        let sp = connected(&d, &stub);
        let old = Utc::now() - Duration::minutes(TOP_TTL_MIN + 1);
        sp.with_cache(|c| cache::put(c, "spotify:top:tracks:long_term", "list", &json!([]), old))
            .unwrap();
        let out = block(get_top_items(&sp, &db, TopKind::Tracks, TopRange::Y1)).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(stub.calls().len(), 1);
    }
}
