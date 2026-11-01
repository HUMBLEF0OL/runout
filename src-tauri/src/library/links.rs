//! Item links (`item_link`; FR-003/004). Direction (API.md `link_items`): track → album `on_album`,
//! item → artist `by_artist`, version → original `version_of`.
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::items::{self, ItemSummary};
use super::{ItemType, LinkKind};
use crate::error::{AppError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    From,
    To,
}

/// One entry of `ItemDetail.links`: `from` = this item is `from_id`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LinkEntry {
    pub kind: LinkKind,
    pub direction: Direction,
    pub item: ItemSummary,
}

/// Which (from, to) types a link kind allows. The one rule `link` and `drop_misfits` share.
fn fits(kind: LinkKind, from: ItemType, to: ItemType) -> bool {
    match kind {
        LinkKind::ByArtist => to == ItemType::Artist && from != ItemType::Artist,
        LinkKind::OnAlbum => from == ItemType::Track && to == ItemType::Album,
        LinkKind::VersionOf => from == to,
    }
}

fn type_of(conn: &Connection, id: i64) -> Result<ItemType> {
    conn.query_row("SELECT type FROM item WHERE id = ?1", [id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That record no longer exists."))
}

/// `link_items`. A track sits on one album, so `on_album` replaces the track's previous album.
pub fn link(conn: &Connection, from: i64, to: i64, kind: LinkKind) -> Result<()> {
    if from == to {
        return Err(AppError::validation("A record can't link to itself."));
    }
    let (ft, tt) = (type_of(conn, from)?, type_of(conn, to)?);
    if !fits(kind, ft, tt) {
        return Err(AppError::validation(match kind {
            LinkKind::ByArtist => "By artist links a record to an artist.",
            LinkKind::OnAlbum => "On album links a track to an album.",
            LinkKind::VersionOf => "Versions link records of the same type.",
        }));
    }
    if kind == LinkKind::OnAlbum {
        conn.execute(
            "DELETE FROM item_link WHERE from_id = ?1 AND kind = 'on_album' AND to_id <> ?2",
            params![from, to],
        )?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO item_link(from_id, to_id, kind) VALUES (?1, ?2, ?3)",
        params![from, to, kind],
    )?;
    items::touch(conn, from)
}

/// `unlink_items`. Unlinking something that isn't linked is fine.
pub fn unlink(conn: &Connection, from: i64, to: i64, kind: LinkKind) -> Result<()> {
    type_of(conn, from)?;
    conn.execute(
        "DELETE FROM item_link WHERE from_id = ?1 AND to_id = ?2 AND kind = ?3",
        params![from, to, kind],
    )?;
    items::touch(conn, from)
}

/// Links touching `id`, both directions, ordered by kind then the other item's id.
pub fn of(conn: &Connection, id: i64) -> Result<Vec<LinkEntry>> {
    let mut st = conn.prepare(
        "SELECT kind, from_id = ?1, CASE WHEN from_id = ?1 THEN to_id ELSE from_id END AS other
         FROM item_link WHERE from_id = ?1 OR to_id = ?1 ORDER BY kind, other",
    )?;
    let rows = st
        .query_map([id], |r| {
            Ok((r.get::<_, LinkKind>(0)?, r.get::<_, bool>(1)?, r.get(2)?))
        })?
        .collect::<rusqlite::Result<Vec<(LinkKind, bool, i64)>>>()?;
    rows.into_iter()
        .map(|(kind, from, other)| {
            Ok(LinkEntry {
                kind,
                direction: if from { Direction::From } else { Direction::To },
                item: items::summary(conn, other)?,
            })
        })
        .collect()
}

/// After a type change: drop every link of `id` that [`fits`] no longer allows (API.md `update_item`).
pub(crate) fn drop_misfits(conn: &Connection, id: i64) -> Result<()> {
    let mut st = conn.prepare(
        "SELECT l.from_id, l.to_id, l.kind, f.type, t.type FROM item_link l
         JOIN item f ON f.id = l.from_id JOIN item t ON t.id = l.to_id
         WHERE l.from_id = ?1 OR l.to_id = ?1",
    )?;
    let bad = st
        .query_map([id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, LinkKind>(2)?,
                r.get::<_, ItemType>(3)?,
                r.get::<_, ItemType>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (from, to, kind, ft, tt) in bad {
        if !fits(kind, ft, tt) {
            conn.execute(
                "DELETE FROM item_link WHERE from_id = ?1 AND to_id = ?2 AND kind = ?3",
                params![from, to, kind],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::error::Code;
    use crate::library::testutil::add;

    #[test]
    fn link_rules_and_both_directions() {
        let mut c = open_in_memory().unwrap();
        let (al, tr, ar) = (
            add(&mut c, ItemType::Album, "Al"),
            add(&mut c, ItemType::Track, "Tr"),
            add(&mut c, ItemType::Artist, "Ar"),
        );
        assert_eq!(
            link(&c, al, tr, LinkKind::OnAlbum).unwrap_err().code,
            Code::Validation
        );
        assert_eq!(
            link(&c, ar, al, LinkKind::ByArtist).unwrap_err().code,
            Code::Validation
        );
        assert_eq!(
            link(&c, tr, al, LinkKind::VersionOf).unwrap_err().code,
            Code::Validation
        );
        assert_eq!(
            link(&c, tr, tr, LinkKind::VersionOf).unwrap_err().code,
            Code::Validation
        );
        assert_eq!(
            link(&c, tr, 999, LinkKind::OnAlbum).unwrap_err().code,
            Code::NotFound
        );
        link(&c, tr, al, LinkKind::OnAlbum).unwrap();
        link(&c, tr, al, LinkKind::OnAlbum).unwrap(); // idempotent
        link(&c, tr, ar, LinkKind::ByArtist).unwrap();
        let t = of(&c, tr).unwrap();
        assert_eq!(t.len(), 2);
        assert!(t.iter().all(|l| l.direction == Direction::From));
        let a = of(&c, al).unwrap();
        assert_eq!(
            (a[0].kind, a[0].direction, a[0].item.id),
            (LinkKind::OnAlbum, Direction::To, tr)
        );

        // a second album replaces the first
        let al2 = add(&mut c, ItemType::Album, "Al2");
        link(&c, tr, al2, LinkKind::OnAlbum).unwrap();
        assert!(of(&c, al).unwrap().is_empty());
        unlink(&c, tr, al2, LinkKind::OnAlbum).unwrap();
        assert_eq!(of(&c, tr).unwrap().len(), 1);
    }
}
