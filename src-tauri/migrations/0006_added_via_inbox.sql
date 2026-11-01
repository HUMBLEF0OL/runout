-- 0006_added_via_inbox: item.added_via also takes 'inbox' (Runout Inbox, TSD §5.3, M7 WI 4). Never edit once shipped.
-- SQLite can't change a CHECK in place: rebuild `item` (sqlite.org/lang_altertable.html §7). db::migrate runs every
-- migration with foreign_keys OFF (else DROP TABLE would cascade-delete links, tags, provenance, history and null
-- listening_event.item_id) and legacy_alter_table ON (the rename leaves views/triggers that name `item` as they are),
-- then requires an empty foreign_key_check before commit. Indexes and item's own triggers go with the old table.
CREATE TABLE item_new (
  id             INTEGER PRIMARY KEY,
  type           TEXT NOT NULL CHECK (type IN ('album','track','artist')),
  display_name   TEXT,
  spotify_uri    TEXT UNIQUE,
  status         TEXT NOT NULL DEFAULT 'planned'
                 CHECK (status IN ('planned','in_progress','explored','not_for_me','abandoned')),
  priority       TEXT CHECK (priority IN ('high','medium','low')),
  rating         REAL CHECK (rating BETWEEN 0.5 AND 5 AND rating * 2 = CAST(rating * 2 AS INT)),
  review         TEXT,
  notes          TEXT,
  favourite      INTEGER NOT NULL DEFAULT 0 CHECK (favourite IN (0,1)),
  revisit        INTEGER NOT NULL DEFAULT 0 CHECK (revisit IN (0,1)),
  in_backlog     INTEGER NOT NULL DEFAULT 1 CHECK (in_backlog IN (0,1)),
  recommended_by TEXT,
  backlog_order  INTEGER,
  added_via      TEXT NOT NULL DEFAULT 'pc' CHECK (added_via IN ('pc','phone','inbox')),  -- API.md ItemSummary.added_via
  added_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  explored_at    TEXT,
  reviewed_at    TEXT,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  modified_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
INSERT INTO item_new SELECT * FROM item;
DROP TABLE item;
ALTER TABLE item_new RENAME TO item;

CREATE INDEX item_status   ON item(status);
CREATE INDEX item_type     ON item(type);
CREATE INDEX item_backlog  ON item(in_backlog, backlog_order);
CREATE INDEX item_added    ON item(added_at);
CREATE INDEX item_modified ON item(modified_at);

-- item's FTS triggers, as in 0001 (item_fts rows are untouched: rowid = item.id is kept).
CREATE TRIGGER item_fts_ai AFTER INSERT ON item BEGIN
  INSERT INTO item_fts(rowid, display_name, review, notes, recommended_by, tags, artists)
    SELECT * FROM item_fts_src WHERE id = new.id;
END;
CREATE TRIGGER item_fts_au AFTER UPDATE OF display_name, review, notes, recommended_by ON item BEGIN
  DELETE FROM item_fts WHERE rowid = new.id
     OR rowid IN (SELECT from_id FROM item_link WHERE to_id = new.id AND kind = 'by_artist');
  INSERT INTO item_fts(rowid, display_name, review, notes, recommended_by, tags, artists)
    SELECT * FROM item_fts_src WHERE id = new.id
       OR id IN (SELECT from_id FROM item_link WHERE to_id = new.id AND kind = 'by_artist');
END;
CREATE TRIGGER item_fts_ad AFTER DELETE ON item BEGIN
  DELETE FROM item_fts WHERE rowid = old.id;
END;
