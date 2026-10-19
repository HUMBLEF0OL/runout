//! `preview_import` (SC-10 step 2): counts only. The overlap with the desktop log comes from running
//! the real import in a transaction that is always rolled back, so nothing is ever written.
// ponytail: preview does the import work twice (preview + run); fine at NFR-04 sizes, cache `Loaded` if not.
use rusqlite::Connection;
use serde::Serialize;

use super::run::{Loaded, apply};
use crate::error::Result;

#[derive(Debug, Serialize, PartialEq)]
pub struct ImportPreview {
    pub music: usize,
    pub podcasts_skipped: usize,
    /// Logger plays the export would reconcile (TSD §4.7).
    pub overlap: usize,
    pub playlists: usize,
    pub private: usize,
    /// Local dates (`YYYY-MM-DD`) of the first and last music play; empty without music.
    pub from: String,
    pub until: String,
    pub files: Vec<String>,
}

pub fn preview(conn: &mut Connection, l: &Loaded) -> Result<ImportPreview> {
    let tx = conn.transaction()?;
    let r = apply(&tx, l)?;
    tx.rollback()?;
    let events = l
        .streaming
        .as_ref()
        .map(|(_, p)| p.events.as_slice())
        .unwrap_or_default();
    let day = |pick: fn(&str, &str) -> bool| {
        let mut best: Option<&str> = None;
        for e in events {
            let d = e.ended_at_local.as_deref().unwrap_or(&e.ended_at_utc);
            if best.is_none_or(|b| pick(d, b)) {
                best = Some(d);
            }
        }
        best.map(|d| d[..10].to_owned()).unwrap_or_default()
    };
    Ok(ImportPreview {
        music: events.len(),
        podcasts_skipped: l.streaming.as_ref().map_or(0, |(_, p)| p.skipped),
        overlap: r.reconciled,
        playlists: l.playlists.as_ref().map_or(0, |(_, p)| p.len()),
        private: events.iter().filter(|e| e.private_session).count(),
        from: day(|a, b| a < b),
        until: day(|a, b| a > b),
        files: l.files.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::importer::run::load;
    use crate::importer::source::fixture;

    #[test]
    fn counts_without_writing() {
        let mut c = db::open_in_memory().unwrap();
        let p = preview(&mut c, &load(&fixture()).unwrap()).unwrap();
        assert_eq!(
            (
                p.music,
                p.podcasts_skipped,
                p.overlap,
                p.playlists,
                p.private
            ),
            (16, 2, 0, 2, 1)
        );
        assert_eq!(p.files.len(), 4);
        assert!(
            p.from.starts_with("2026-03-0") && p.until.starts_with("2026-10-25"),
            "{p:?}"
        );
        for t in ["listening_event", "import_batch", "exported_playlist"] {
            let n: i64 = c
                .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{t}");
        }
    }
}
