//! Event → library item (TSD §4.6): `spotify_track_uri` first; else `norm(track)` against track
//! items, tie-broken by overlap between the event's artist string and the item's linked artists.
//! Still ambiguous → `None` (the user links it from "unmatched plays").
use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use rusqlite::Connection;

use super::normalize::norm;
use crate::error::Result;

/// Separators inside one artist string ("A, B", "A & B", "A feat. B", "A x B").
static ARTIST_SEP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s*(?:[,;&/]|\s(?:feat\.?|ft\.?|featuring|x)\s)\s*").expect("valid regex")
});

/// Track items indexed once, so a batch import doesn't query per event.
pub struct Matcher {
    by_uri: HashMap<String, i64>,
    /// norm(track name) → (item id, norm(linked artist names)).
    by_track: HashMap<String, Vec<(i64, Vec<String>)>>,
}

impl Matcher {
    pub fn load(conn: &Connection) -> Result<Self> {
        let mut by_uri = HashMap::new();
        let mut by_track: HashMap<String, Vec<(i64, Vec<String>)>> = HashMap::new();
        let mut artists: HashMap<i64, Vec<String>> = HashMap::new();
        let mut st = conn.prepare(
            "SELECT l.from_id, a.display_name FROM item_link l JOIN item a ON a.id = l.to_id
             WHERE l.kind = 'by_artist' AND a.display_name IS NOT NULL",
        )?;
        for r in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            let (id, name) = r?;
            artists.entry(id).or_default().push(norm(&name));
        }
        let mut st =
            conn.prepare("SELECT id, spotify_uri, display_name FROM item WHERE type = 'track'")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        for r in rows {
            let (id, uri, name) = r?;
            if let Some(uri) = uri {
                by_uri.insert(uri, id);
            }
            if let Some(name) = name {
                let linked = artists.remove(&id).unwrap_or_default();
                by_track.entry(norm(&name)).or_default().push((id, linked));
            }
        }
        Ok(Self { by_uri, by_track })
    }

    pub fn find(&self, uri: Option<&str>, track: &str, artist: &str) -> Option<i64> {
        if let Some(id) = uri.and_then(|u| self.by_uri.get(u)) {
            return Some(*id);
        }
        let cands = self.by_track.get(&norm(track))?;
        if let [(id, _)] = cands.as_slice() {
            return Some(*id);
        }
        let whole = norm(artist);
        let parts: Vec<&str> = ARTIST_SEP.split(&whole).collect();
        let mut hits = cands.iter().filter(|(_, linked)| {
            linked
                .iter()
                .any(|a| *a == whole || parts.contains(&a.as_str()))
        });
        match (hits.next(), hits.next()) {
            (Some((id, _)), None) => Some(*id),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const URI: &str = "spotify:track:AAAAAAAAAAAAAAAAAAAAAA";

    #[test]
    fn matching_table() {
        let c = db::open_in_memory().unwrap();
        c.execute_batch(&format!(
            "INSERT INTO item(id, type, display_name) VALUES
               (1, 'artist', 'Zorblax'), (2, 'artist', 'Quuxley'), (3, 'artist', 'Café Trio');
             INSERT INTO item(id, type, display_name, spotify_uri) VALUES (10, 'track', 'Unique Song', '{URI}');
             INSERT INTO item(id, type, display_name) VALUES
               (11, 'track', 'Shared Name'), (12, 'track', 'Shared Name'),
               (13, 'track', 'Twice'), (14, 'track', 'Twice'),
               (15, 'album', 'Album Only');
             INSERT INTO item(id, type) VALUES (16, 'track');
             INSERT INTO item_link VALUES (11, 1, 'by_artist'), (12, 2, 'by_artist'), (12, 3, 'by_artist'),
               (13, 1, 'by_artist'), (14, 1, 'by_artist');"
        ))
        .unwrap();
        let m = Matcher::load(&c).unwrap();
        for (uri, track, artist, want) in [
            // uri wins even when the name differs
            (Some(URI), "Other Title", "Nobody", Some(10)),
            // unknown uri falls back to the name
            (
                Some("spotify:track:BBBBBBBBBBBBBBBBBBBBBB"),
                "Unique Song",
                "Nobody",
                Some(10),
            ),
            // single candidate by normalized name, artist not needed
            (None, "UNIQUE SONG - Remastered 2011", "Anyone", Some(10)),
            // tie broken by artist overlap, SMTC multi-artist string
            (None, "Shared Name", "Someone, Quuxley", Some(12)),
            (
                None,
                "Shared Name (feat. Someone)",
                "Zorblax feat. Someone",
                Some(11),
            ),
            // diacritics / case in the artist
            (None, "Shared Name", "CAFE TRIO", Some(12)),
            // no overlap → unmatched
            (None, "Shared Name", "Stranger", None),
            // both candidates overlap → still ambiguous
            (None, "Twice", "Zorblax", None),
            // albums and nameless tracks never match by name
            (None, "Album Only", "Zorblax", None),
            (None, "", "", None),
            (None, "Missing", "Zorblax", None),
        ] {
            assert_eq!(m.find(uri, track, artist), want, "{track} / {artist}");
        }
    }
}
