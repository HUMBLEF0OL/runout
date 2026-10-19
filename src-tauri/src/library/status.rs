//! Status changes: `status_history`, `explored_at` (FR-005/006).
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::backlog;
use super::items::{self, ItemDetail};
use super::{NOW, Status};
use crate::error::{AppError, Result};
use crate::text::{self, limit};

/// One entry of `ItemDetail.history`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HistoryEntry {
    pub from: Option<Status>,
    pub to: Status,
    pub at: String,
    pub note: Option<String>,
}

pub(crate) fn insert(
    conn: &Connection,
    id: i64,
    from: Option<Status>,
    to: Status,
    note: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO status_history(item_id, from_status, to_status, note) VALUES (?1, ?2, ?3, ?4)",
        params![id, from, to, note],
    )?;
    Ok(())
}

/// Newest first (ties: latest insert first).
pub fn history(conn: &Connection, id: i64) -> Result<Vec<HistoryEntry>> {
    let mut st = conn.prepare(
        "SELECT from_status, to_status, at, note FROM status_history WHERE item_id = ?1
         ORDER BY at DESC, id DESC",
    )?;
    let rows = st.query_map([id], |r| {
        Ok(HistoryEntry {
            from: r.get(0)?,
            to: r.get(1)?,
            at: r.get(2)?,
            note: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn is_done(s: Status) -> bool {
    matches!(s, Status::Explored | Status::NotForMe)
}

/// What a status change does besides the `status` column (shared by `set_status` and `create`).
/// Into explored / not for me: sets `explored_at` and leaves the backlog (`not_for_me_this_month`
/// counts by `explored_at`). Out of them: clears `explored_at`; back to planned also rejoins the
/// backlog at the end.
pub(crate) fn apply_side_effects(
    conn: &Connection,
    id: i64,
    from: Option<Status>,
    to: Status,
) -> Result<()> {
    if is_done(to) {
        conn.execute(
            &format!(
                "UPDATE item SET explored_at = {NOW}, in_backlog = 0, backlog_order = NULL WHERE id = ?1"
            ),
            [id],
        )?;
    } else if from.is_some_and(is_done) {
        conn.execute("UPDATE item SET explored_at = NULL WHERE id = ?1", [id])?;
        if to == Status::Planned {
            backlog::set_in_backlog(conn, id, true)?;
        }
    }
    Ok(())
}

/// `set_status`. Same status → no-op; otherwise a history row plus `apply_side_effects`.
pub fn set_status(
    conn: &mut Connection,
    id: i64,
    status: Status,
    note: Option<&str>,
) -> Result<ItemDetail> {
    let tx = conn.transaction()?;
    let cur: Status = tx
        .query_row("SELECT status FROM item WHERE id = ?1", [id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That record no longer exists."))?;
    if cur != status {
        let note = text::line_or_none(note, limit::NOTE);
        insert(&tx, id, Some(cur), status, note.as_deref())?;
        tx.execute(
            "UPDATE item SET status = ?2 WHERE id = ?1",
            params![id, status],
        )?;
        apply_side_effects(&tx, id, Some(cur), status)?;
        items::touch(&tx, id)?;
    }
    tx.commit()?;
    items::get(conn, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::library::ItemType;
    use crate::library::testutil::add;

    #[test]
    fn history_is_newest_first_and_explored_sets_explored_at() {
        let mut c = open_in_memory().unwrap();
        let id = add(&mut c, ItemType::Album, "A");
        set_status(&mut c, id, Status::InProgress, Some("started")).unwrap();
        let d = set_status(&mut c, id, Status::Explored, None).unwrap();
        let steps: Vec<_> = d.history.iter().map(|h| (h.from, h.to)).collect();
        assert_eq!(
            steps,
            vec![
                (Some(Status::InProgress), Status::Explored),
                (Some(Status::Planned), Status::InProgress),
                (None, Status::Planned),
            ]
        );
        assert_eq!(d.history[1].note.as_deref(), Some("started"));
        assert!(d.explored_at.is_some());
        assert!(!d.summary.in_backlog && d.summary.backlog_order.is_none());
        // same status again: no new row
        let d = set_status(&mut c, id, Status::Explored, None).unwrap();
        assert_eq!(d.history.len(), 3);
        assert_eq!(
            set_status(&mut c, 99, Status::Explored, None)
                .unwrap_err()
                .code,
            crate::error::Code::NotFound
        );
    }

    #[test]
    fn leaving_explored_clears_explored_at_and_planned_rejoins_backlog_at_end() {
        let mut c = open_in_memory().unwrap();
        let a = add(&mut c, ItemType::Album, "A");
        let b = add(&mut c, ItemType::Album, "B");
        let last = items::get(&c, b).unwrap().summary.backlog_order.unwrap();
        set_status(&mut c, a, Status::Explored, None).unwrap();
        let d = set_status(&mut c, a, Status::Planned, None).unwrap();
        assert!(d.explored_at.is_none());
        assert!(d.summary.in_backlog);
        assert!(d.summary.backlog_order.unwrap() > last);
        // not_for_me → in_progress: explored_at cleared, stays out of the backlog
        set_status(&mut c, b, Status::NotForMe, None).unwrap();
        let d = set_status(&mut c, b, Status::InProgress, None).unwrap();
        assert!(d.explored_at.is_none());
        assert!(!d.summary.in_backlog && d.summary.backlog_order.is_none());
    }

    #[test]
    fn create_with_explored_status_gets_the_same_side_effects() {
        let mut c = open_in_memory().unwrap();
        let mut n = crate::library::testutil::new(ItemType::Album, "Done");
        n.status = Some(Status::NotForMe);
        let d = items::create(&mut c, n).unwrap();
        assert!(d.explored_at.is_some());
        assert!(!d.summary.in_backlog && d.summary.backlog_order.is_none());
    }
}
