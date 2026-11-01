//! The one writer of `listening_event` rows, shared by the logger and the importer (TSD §4.4–4.8):
//! computes `norm_key` and `dedupe_hash`, inserts or ignores on a duplicate hash.
use rusqlite::{Connection, OptionalExtension, params};

use super::normalize::norm_key;
use crate::backup::sha256_hex;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Logger,
    Export,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Logger => "logger",
            Self::Export => "export",
        }
    }
}

/// One play as written to `listening_event`; `counts_as_play` is a generated column.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub source: Source,
    /// ISO-8601 UTC; for the export this is `ts`.
    pub ended_at_utc: String,
    pub ended_at_local: Option<String>,
    pub ms_played: i64,
    pub track_name: String,
    pub artist_name: String,
    pub album_name: String,
    pub spotify_track_uri: Option<String>,
    pub platform: Option<String>,
    pub reason_start: Option<String>,
    pub reason_end: Option<String>,
    pub shuffle: Option<bool>,
    /// Track length from SMTC (logger rows only; `None` for the export or when the app didn't say).
    pub duration_ms: Option<i64>,
    pub skipped: Option<bool>,
    pub offline: Option<bool>,
    pub private_session: bool,
    pub item_id: Option<i64>,
    pub import_batch_id: Option<i64>,
}

impl Event {
    /// Export: `sha256(ts|spotify_track_uri|ms_played)` (TSD §4.8). Logger rows get their own
    /// namespace (and fall back to `norm_key` when SMTC gave no uri) so they never collide with export rows.
    pub fn dedupe_hash(&self) -> String {
        let uri = self.spotify_track_uri.as_deref().unwrap_or_default();
        let input = match self.source {
            Source::Export => format!("{}|{uri}|{}", self.ended_at_utc, self.ms_played),
            Source::Logger => {
                let id = if uri.is_empty() {
                    norm_key(&self.artist_name, &self.track_name)
                } else {
                    uri.to_owned()
                };
                format!("logger|{}|{id}|{}", self.ended_at_utc, self.ms_played)
            }
        };
        sha256_hex(input.as_bytes())
    }
}

/// Insert `e`; `Some(id)` when new, `None` when its `dedupe_hash` already exists.
pub fn insert(conn: &Connection, e: &Event) -> Result<Option<i64>> {
    // Cached: an import inserts ~100k rows through this one statement (NFR-04).
    Ok(conn
        .prepare_cached(
            "INSERT INTO listening_event(source, ended_at_utc, ended_at_local, ms_played, track_name,
               artist_name, album_name, norm_key, spotify_track_uri, platform, reason_start, reason_end,
               shuffle, skipped, offline, private_session, item_id, import_batch_id, dedupe_hash, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
             ON CONFLICT(dedupe_hash) DO NOTHING RETURNING id",
        )?
        .query_row(
            params![
                e.source.as_str(),
                e.ended_at_utc,
                e.ended_at_local,
                e.ms_played,
                e.track_name,
                e.artist_name,
                e.album_name,
                norm_key(&e.artist_name, &e.track_name),
                e.spotify_track_uri,
                e.platform,
                e.reason_start,
                e.reason_end,
                e.shuffle,
                e.skipped,
                e.offline,
                e.private_session,
                e.item_id,
                e.import_batch_id,
                e.dedupe_hash(),
                e.duration_ms.filter(|&d| d > 0),
            ],
            |r| r.get(0),
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn ev(source: Source, ms: i64) -> Event {
        Event {
            source,
            ended_at_utc: "2026-03-01T12:00:00Z".into(),
            ended_at_local: Some("2026-03-01T13:00:00".into()),
            ms_played: ms,
            track_name: "Fixture Song - Remastered 2011".into(),
            artist_name: "Fixture Bänd".into(),
            album_name: "Fixture Album".into(),
            spotify_track_uri: Some("spotify:track:AAAAAAAAAAAAAAAAAAAAAA".into()),
            platform: None,
            reason_start: None,
            reason_end: None,
            shuffle: Some(false),
            duration_ms: None,
            skipped: None,
            offline: None,
            private_session: false,
            item_id: None,
            import_batch_id: None,
        }
    }

    #[test]
    fn insert_is_idempotent_and_derives_columns() {
        let c = db::open_in_memory().unwrap();
        let id = insert(&c, &ev(Source::Export, 30_000)).unwrap();
        assert!(id.is_some());
        assert_eq!(
            insert(&c, &ev(Source::Export, 30_000)).unwrap(),
            None,
            "same hash"
        );
        // same moment from the logger is a separate row (reconciliation pairs them later)
        assert!(insert(&c, &ev(Source::Logger, 30_000)).unwrap().is_some());
        assert!(insert(&c, &ev(Source::Export, 29_999)).unwrap().is_some());
        let mut nouri = ev(Source::Logger, 5_000);
        nouri.spotify_track_uri = None;
        assert!(insert(&c, &nouri).unwrap().is_some());
        assert_eq!(insert(&c, &nouri).unwrap(), None);

        let (key, play, hash): (String, bool, String) = c
            .query_row(
                "SELECT norm_key, counts_as_play, dedupe_hash FROM listening_event WHERE id = ?1",
                [id.unwrap()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(key, "fixture band|fixture song");
        assert!(play);
        let want = sha256_hex(b"2026-03-01T12:00:00Z|spotify:track:AAAAAAAAAAAAAAAAAAAAAA|30000");
        assert_eq!(hash, want);
        let n: i64 = c
            .query_row("SELECT count(*) FROM listening_event", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 4);
    }
}
