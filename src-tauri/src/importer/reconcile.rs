//! Logger ↔ export reconciliation (TSD §4.7): each unlinked logger event gets at most one export
//! partner with the same `norm(track)` within ±120 s, preferring artist overlap, then the smallest
//! gap. `reconciled_with` + the `one_to_one_reconcile` index keep it one-to-one; insights skip
//! reconciled logger events, so a play is never counted twice.
use chrono::NaiveDateTime;
use rusqlite::{Connection, params};

use super::matching::artists;
use super::normalize::norm;
use crate::error::Result;

pub const WINDOW_S: i64 = 120;
const FMT: &str = "%Y-%m-%dT%H:%M:%SZ";

/// Link every unreconciled logger event it can; returns how many were linked.
pub fn reconcile(conn: &Connection) -> Result<usize> {
    let mut st = conn.prepare(
        "SELECT id, ended_at_utc, track_name, artist_name FROM listening_event
         WHERE source = 'logger' AND reconciled_with IS NULL ORDER BY ended_at_utc, id",
    )?;
    let row = |r: &rusqlite::Row| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            r.get::<_, Option<String>>(3)?.unwrap_or_default(),
        ))
    };
    let logger = st
        .query_map([], row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut cands = conn.prepare(
        "SELECT e.id, e.ended_at_utc, e.track_name, e.artist_name FROM listening_event e
         WHERE e.source = 'export' AND e.ended_at_utc BETWEEN ?1 AND ?2
           AND NOT EXISTS (SELECT 1 FROM listening_event l WHERE l.reconciled_with = e.id)",
    )?;
    let mut link = conn.prepare("UPDATE listening_event SET reconciled_with = ?1 WHERE id = ?2")?;
    let mut linked = 0;
    for (id, ts, track, artist) in logger {
        let (Ok(t), key) = (NaiveDateTime::parse_from_str(&ts, FMT), norm(&track)) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        let w = chrono::Duration::seconds(WINDOW_S);
        let (lo, hi) = (
            (t - w).format(FMT).to_string(),
            (t + w).format(FMT).to_string(),
        );
        let mine = artists(&artist);
        let best = cands
            .query_map(params![lo, hi], row)?
            .filter_map(|r| r.ok())
            .filter(|(_, _, track, _)| norm(track) == key)
            .filter_map(|(cid, cts, _, cartist)| {
                let gap = (NaiveDateTime::parse_from_str(&cts, FMT).ok()? - t)
                    .num_seconds()
                    .abs();
                let overlap = artists(&cartist).iter().any(|a| mine.contains(a));
                Some(((!overlap, gap), cid))
            })
            .min();
        if let Some((_, cid)) = best {
            link.execute(params![cid, id])?;
            linked += 1;
        }
    }
    Ok(linked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::importer::events::{Event, Source, insert};

    pub fn ev(source: Source, ts: &str, track: &str, artist: &str) -> Event {
        Event {
            source,
            ended_at_utc: ts.into(),
            ended_at_local: None,
            ms_played: 60_000,
            track_name: track.into(),
            artist_name: artist.into(),
            album_name: String::new(),
            spotify_track_uri: None,
            platform: None,
            reason_start: None,
            reason_end: None,
            shuffle: None,
            skipped: None,
            offline: None,
            private_session: false,
            item_id: None,
            import_batch_id: None,
        }
    }

    fn partner(c: &Connection, id: i64) -> Option<i64> {
        c.query_row(
            "SELECT reconciled_with FROM listening_event WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn one_to_one_artist_then_gap() {
        let c = db::open_in_memory().unwrap();
        let add = |s, ts, t, a| insert(&c, &ev(s, ts, t, a)).unwrap().unwrap();
        use Source::{Export as E, Logger as L};
        // artist overlap beats a closer candidate without it
        let far = add(E, "2026-01-01T10:01:30Z", "Song", "Band A");
        let _near = add(E, "2026-01-01T10:00:05Z", "Song", "Other");
        let l1 = add(
            L,
            "2026-01-01T10:00:00Z",
            "Song - Remastered 2011",
            "Band A, Guest",
        );
        // back-to-back repeat: both logger plays closest to e1 → each gets its own partner
        let e1 = add(E, "2026-02-01T10:00:00Z", "Loop", "X");
        let e2 = add(E, "2026-02-01T10:01:00Z", "Loop", "X");
        let l2 = add(L, "2026-02-01T10:00:02Z", "Loop", "X");
        let l3 = add(L, "2026-02-01T10:00:20Z", "Loop", "X");
        // outside the window, or a different track → stays unlinked
        let l4 = add(L, "2026-02-01T10:05:00Z", "Loop", "X");
        let l5 = add(L, "2026-01-01T10:00:00Z", "Different", "Band A");
        assert_eq!(reconcile(&c).unwrap(), 3);
        assert_eq!(partner(&c, l1), Some(far));
        assert_eq!((partner(&c, l2), partner(&c, l3)), (Some(e1), Some(e2)));
        assert_eq!((partner(&c, l4), partner(&c, l5)), (None, None));
        // rerun links nothing new, and nothing is linked twice
        assert_eq!(reconcile(&c).unwrap(), 0);
        let dup: i64 = c
            .query_row(
                "SELECT count(*) - count(DISTINCT reconciled_with) FROM listening_event
                 WHERE reconciled_with IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dup, 0);
    }
}
