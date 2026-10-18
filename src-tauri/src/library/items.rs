//! Item CRUD (FR-001–004): create/get/update/delete, duplicate → `duplicate{existing_id}`. M3 step 4.
use rusqlite::{Connection, OptionalExtension, params};

use super::ItemType;
use crate::error::Result;

/// Id of the item holding `uri`, if any (FR-004 duplicate check, `parse_link.existing_id`).
pub fn id_by_uri(conn: &Connection, uri: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row("SELECT id FROM item WHERE spotify_uri = ?1", [uri], |r| {
            r.get(0)
        })
        .optional()?)
}

/// "Name needed" item from a captured/resolved link (TSD §4.2): URI only, provenance `promoted`,
/// end of the backlog. Returns the existing id if the URI is already in the library.
pub fn insert_name_needed(
    conn: &Connection,
    kind: ItemType,
    uri: &str,
    added_via: &str,
) -> Result<i64> {
    if let Some(id) = id_by_uri(conn, uri)? {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO item(type, spotify_uri, added_via, backlog_order)
         VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(backlog_order), 0) + 1 FROM item WHERE in_backlog = 1))",
        params![kind.as_str(), uri, added_via],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO field_provenance(item_id, field, source) VALUES (?1, 'spotify_uri', 'promoted')",
        [id],
    )?;
    conn.execute(
        "INSERT INTO status_history(item_id, from_status, to_status) VALUES (?1, NULL, 'planned')",
        [id],
    )?;
    Ok(id)
}
