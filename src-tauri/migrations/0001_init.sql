-- 0001_init: MVP core tables (TSD §3). Never edit once shipped; add 0002_… instead.
-- Times are ISO-8601 UTC strings ("2026-10-18T13:55:39Z").

CREATE TABLE item (
  id             INTEGER PRIMARY KEY,
  type           TEXT NOT NULL CHECK (type IN ('album','track','artist')),
  display_name   TEXT,                                  -- NULL = "name needed" (TSD §4.2)
  spotify_uri    TEXT UNIQUE,                           -- NULL allowed; duplicates blocked (FR-004)
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
  added_via      TEXT NOT NULL DEFAULT 'pc' CHECK (added_via IN ('pc','phone')),  -- API.md ItemSummary.added_via
  added_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  explored_at    TEXT,
  reviewed_at    TEXT,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  modified_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
CREATE INDEX item_status   ON item(status);
CREATE INDEX item_type     ON item(type);
CREATE INDEX item_backlog  ON item(in_backlog, backlog_order);
CREATE INDEX item_added    ON item(added_at);
CREATE INDEX item_modified ON item(modified_at);

CREATE TABLE field_provenance (
  item_id INTEGER NOT NULL REFERENCES item(id) ON DELETE CASCADE,
  field   TEXT NOT NULL,
  source  TEXT NOT NULL CHECK (source IN ('user','promoted')),
  PRIMARY KEY (item_id, field)
) WITHOUT ROWID;

CREATE TABLE item_link (
  from_id INTEGER NOT NULL REFERENCES item(id) ON DELETE CASCADE,
  to_id   INTEGER NOT NULL REFERENCES item(id) ON DELETE CASCADE,
  kind    TEXT NOT NULL CHECK (kind IN ('by_artist','on_album','version_of')),
  PRIMARY KEY (from_id, to_id, kind),
  CHECK (from_id <> to_id)
) WITHOUT ROWID;
CREATE INDEX item_link_to ON item_link(to_id, kind);

CREATE TABLE tag (
  id          INTEGER PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE COLLATE NOCASE,
  description TEXT
);

CREATE TABLE item_tag (
  item_id INTEGER NOT NULL REFERENCES item(id) ON DELETE CASCADE,
  tag_id  INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
  PRIMARY KEY (item_id, tag_id)
) WITHOUT ROWID;
CREATE INDEX item_tag_tag ON item_tag(tag_id);

CREATE TABLE status_history (
  id          INTEGER PRIMARY KEY,
  item_id     INTEGER NOT NULL REFERENCES item(id) ON DELETE CASCADE,
  from_status TEXT CHECK (from_status IN ('planned','in_progress','explored','not_for_me','abandoned')),
  to_status   TEXT NOT NULL CHECK (to_status IN ('planned','in_progress','explored','not_for_me','abandoned')),
  at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  note        TEXT
);
CREATE INDEX status_history_item ON status_history(item_id, at);

CREATE TABLE saved_filter (
  id              INTEGER PRIMARY KEY,
  name            TEXT NOT NULL,
  definition_json TEXT NOT NULL
);

CREATE TABLE unresolved_capture (
  id          INTEGER PRIMARY KEY,
  raw_link    TEXT NOT NULL,
  received_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  source      TEXT NOT NULL CHECK (source IN ('paste','shortcut')),
  state       TEXT NOT NULL DEFAULT 'unresolved' CHECK (state IN ('unresolved','failed')),
  error       TEXT
);

-- Success metrics (FR-045), local only, never exported.
CREATE TABLE usage_event (
  id          INTEGER PRIMARY KEY,
  kind        TEXT NOT NULL CHECK (kind IN ('app_open','page_visit','capture')),
  at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  detail      TEXT,
  duration_ms INTEGER
);
CREATE INDEX usage_event_kind ON usage_event(kind, at);

-- Search (NFR-04). item_fts holds denormalised copies (rowid = item.id) so snippet() works and
-- tag names / linked artist names are searchable. The triggers below keep it in sync.
CREATE VIRTUAL TABLE item_fts USING fts5(
  display_name, review, notes, recommended_by, tags, artists,
  tokenize = 'unicode61 remove_diacritics 2'
);
CREATE VIEW item_fts_src AS
SELECT i.id, i.display_name, i.review, i.notes, i.recommended_by,
       (SELECT group_concat(t.name, ' ') FROM item_tag it JOIN tag t ON t.id = it.tag_id
         WHERE it.item_id = i.id) AS tags,
       (SELECT group_concat(a.display_name, ' ') FROM item_link l JOIN item a ON a.id = l.to_id
         WHERE l.from_id = i.id AND l.kind = 'by_artist') AS artists
FROM item i;

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
CREATE TRIGGER item_tag_fts_ai AFTER INSERT ON item_tag BEGIN
  DELETE FROM item_fts WHERE rowid = new.item_id;
  INSERT INTO item_fts(rowid, display_name, review, notes, recommended_by, tags, artists)
    SELECT * FROM item_fts_src WHERE id = new.item_id;
END;
CREATE TRIGGER item_tag_fts_ad AFTER DELETE ON item_tag BEGIN
  DELETE FROM item_fts WHERE rowid = old.item_id;
  INSERT INTO item_fts(rowid, display_name, review, notes, recommended_by, tags, artists)
    SELECT * FROM item_fts_src WHERE id = old.item_id;
END;
CREATE TRIGGER tag_fts_au AFTER UPDATE OF name ON tag BEGIN
  DELETE FROM item_fts WHERE rowid IN (SELECT item_id FROM item_tag WHERE tag_id = new.id);
  INSERT INTO item_fts(rowid, display_name, review, notes, recommended_by, tags, artists)
    SELECT * FROM item_fts_src WHERE id IN (SELECT item_id FROM item_tag WHERE tag_id = new.id);
END;
CREATE TRIGGER item_link_fts_ai AFTER INSERT ON item_link WHEN new.kind = 'by_artist' BEGIN
  DELETE FROM item_fts WHERE rowid = new.from_id;
  INSERT INTO item_fts(rowid, display_name, review, notes, recommended_by, tags, artists)
    SELECT * FROM item_fts_src WHERE id = new.from_id;
END;
CREATE TRIGGER item_link_fts_ad AFTER DELETE ON item_link WHEN old.kind = 'by_artist' BEGIN
  DELETE FROM item_fts WHERE rowid = old.from_id;
  INSERT INTO item_fts(rowid, display_name, review, notes, recommended_by, tags, artists)
    SELECT * FROM item_fts_src WHERE id = old.from_id;
END;
