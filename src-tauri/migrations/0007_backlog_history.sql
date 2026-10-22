-- 0007_backlog_history: track backlog entry and exit for moved_after_14d_pct (G-16).
CREATE TABLE backlog_history (
  id         INTEGER PRIMARY KEY,
  item_id    INTEGER NOT NULL REFERENCES item(id) ON DELETE CASCADE,
  in_backlog INTEGER NOT NULL CHECK (in_backlog IN (0,1)),
  at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
CREATE INDEX backlog_history_item ON backlog_history(item_id, at);

-- Backfill from existing items:
-- 1. Any item currently in the backlog entered at added_at
INSERT INTO backlog_history(item_id, in_backlog, at)
  SELECT id, 1, added_at FROM item WHERE in_backlog = 1;

-- 2. Any non-artist item currently not in the backlog that was planned or moved on entered at added_at
INSERT INTO backlog_history(item_id, in_backlog, at)
  SELECT id, 1, added_at FROM item WHERE in_backlog = 0 AND type <> 'artist' AND (status <> 'planned' OR modified_at > added_at);

-- 3. And if it left the backlog, record its exit at explored_at or modified_at
INSERT INTO backlog_history(item_id, in_backlog, at)
  SELECT id, 0, COALESCE(explored_at, modified_at) FROM item WHERE in_backlog = 0 AND type <> 'artist' AND (status <> 'planned' OR modified_at > added_at);

-- Triggers to maintain backlog_history on future inserts and updates
CREATE TRIGGER item_backlog_ai AFTER INSERT ON item BEGIN
  INSERT INTO backlog_history(item_id, in_backlog, at) VALUES (new.id, new.in_backlog, new.added_at);
END;

CREATE TRIGGER item_backlog_au AFTER UPDATE OF in_backlog ON item WHEN new.in_backlog <> old.in_backlog BEGIN
  INSERT INTO backlog_history(item_id, in_backlog, at) VALUES (new.id, new.in_backlog, strftime('%Y-%m-%dT%H:%M:%SZ','now'));
END;

