//! Backlog order: reorder, remove without delete (FR-012).
use rusqlite::{Connection, params};

use crate::error::{AppError, Result};

/// In: keep the current place, or go to the end. Out: leave the backlog (item stays in the library).
pub(crate) fn set_in_backlog(conn: &Connection, id: i64, on: bool) -> Result<()> {
    let sql = if on {
        "UPDATE item SET in_backlog = 1, backlog_order = COALESCE(backlog_order,
           (SELECT COALESCE(MAX(backlog_order), 0) + 1 FROM item WHERE in_backlog = 1)) WHERE id = ?1"
    } else {
        "UPDATE item SET in_backlog = 0, backlog_order = NULL WHERE id = ?1"
    };
    conn.execute(sql, [id])?;
    Ok(())
}

/// `reorder_backlog`: `ids` is the visible order. Those items take the order slots they already
/// hold, re-dealt in the new sequence, so items outside the visible list keep their places.
/// Does not bump `modified_at` (reordering isn't touching the record; stale backlog relies on it).
pub fn reorder_backlog(conn: &mut Connection, ids: &[i64]) -> Result<()> {
    let tx = conn.transaction()?;
    let mut slots = vec![];
    {
        let mut st = tx.prepare(
            "SELECT backlog_order FROM item WHERE id = ?1 AND in_backlog = 1 AND backlog_order IS NOT NULL",
        )?;
        for id in ids {
            if let Some(o) = st.query_map([id], |r| r.get::<_, i64>(0))?.next() {
                slots.push(o?);
            }
        }
    }
    slots.sort_unstable();
    slots.dedup();
    if slots.len() != ids.len() {
        return Err(AppError::validation(
            "The backlog changed. Reload and try again.",
        ));
    }
    for (id, slot) in ids.iter().zip(slots) {
        tx.execute(
            "UPDATE item SET backlog_order = ?2 WHERE id = ?1",
            params![id, slot],
        )?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::library::ItemType;
    use crate::library::items::{ItemPatch, get, update};
    use crate::library::testutil::add;

    fn order(c: &Connection, id: i64) -> Option<i64> {
        get(c, id).unwrap().summary.backlog_order
    }

    #[test]
    fn reorder_reuses_slots_and_remove_keeps_the_item() {
        let mut c = open_in_memory().unwrap();
        let ids: Vec<i64> = (0..4)
            .map(|n| add(&mut c, ItemType::Album, &format!("A{n}")))
            .collect();
        // visible subset [0, 2] reversed → they swap slots 1 and 3; 1 and 3 keep 2 and 4.
        reorder_backlog(&mut c, &[ids[2], ids[0]]).unwrap();
        assert_eq!(
            ids.iter().map(|&i| order(&c, i)).collect::<Vec<_>>(),
            vec![Some(3), Some(2), Some(1), Some(4)]
        );
        assert!(reorder_backlog(&mut c, &[ids[0], ids[0]]).is_err());
        assert!(reorder_backlog(&mut c, &[999]).is_err());

        let p = ItemPatch {
            in_backlog: Some(false),
            ..Default::default()
        };
        let d = update(&mut c, ids[1], p).unwrap();
        assert!(!d.summary.in_backlog && d.summary.backlog_order.is_none());
        let p = ItemPatch {
            in_backlog: Some(true),
            ..Default::default()
        };
        assert_eq!(
            update(&mut c, ids[1], p).unwrap().summary.backlog_order,
            Some(5)
        );
    }
}
