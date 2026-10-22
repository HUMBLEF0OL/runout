-- 0007_backlog_history: record when an item enters or leaves the backlog, so the success metric
-- "moved after 14 days" (BRD §15) counts a planned item taken out of the backlog (G-16).
CREATE TABLE backlog_history (
  id         INTEGER PRIMARY KEY,
  item_id    INTEGER NOT NULL REFERENCES item(id) ON DELETE CASCADE,
  in_backlog INTEGER NOT NULL CHECK (in_backlog IN (0,1)),
  at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
CREATE INDEX backlog_history_item ON backlog_history(item_id, at);

-- Backfill entries only from what the old rule could see (in the backlog, or moved on from planned);
-- earlier exits weren't recorded, so none are guessed.
INSERT INTO backlog_history(item_id, in_backlog, at)
  SELECT id, 1, added_at FROM item WHERE type <> 'artist' AND (in_backlog = 1 OR status <> 'planned');

-- create_item inserts with in_backlog = 0 and then sets it, so only a real entry is recorded on insert.
CREATE TRIGGER item_backlog_ai AFTER INSERT ON item WHEN new.in_backlog = 1 BEGIN
  INSERT INTO backlog_history(item_id, in_backlog, at) VALUES (new.id, 1, new.added_at);
END;

CREATE TRIGGER item_backlog_au AFTER UPDATE OF in_backlog ON item WHEN new.in_backlog <> old.in_backlog BEGIN
  INSERT INTO backlog_history(item_id, in_backlog, at) VALUES (new.id, new.in_backlog, strftime('%Y-%m-%dT%H:%M:%SZ','now'));
END;
