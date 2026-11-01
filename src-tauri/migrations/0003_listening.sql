-- 0003_listening: Release 2 listening log + export import (TSD §3, §4.4-4.8). Never edit once shipped.
-- Every foreign key has an ON DELETE action so Delete library can empty tables in any order.

CREATE TABLE import_batch (
  id               INTEGER PRIMARY KEY,
  kind             TEXT NOT NULL CHECK (kind IN ('streaming_history','account_data')),
  imported_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  record_count     INTEGER NOT NULL DEFAULT 0,
  latest_event_utc TEXT,
  checksum         TEXT NOT NULL UNIQUE                 -- same files again → refused (idempotent)
);

CREATE TABLE listening_event (
  id                INTEGER PRIMARY KEY,
  source            TEXT NOT NULL CHECK (source IN ('logger','export')),
  ended_at_utc      TEXT NOT NULL,
  ended_at_local    TEXT,
  ms_played         INTEGER NOT NULL CHECK (ms_played >= 0),
  counts_as_play    INTEGER GENERATED ALWAYS AS (ms_played >= 30000) VIRTUAL,  -- D25 play rule
  track_name        TEXT,
  artist_name       TEXT,
  album_name        TEXT,
  norm_key          TEXT,                                -- norm(artist) || '|' || norm(track) (TSD §4.6)
  spotify_track_uri TEXT,
  platform          TEXT,
  reason_start      TEXT,
  reason_end        TEXT,
  shuffle           INTEGER CHECK (shuffle IN (0,1)),
  skipped           INTEGER CHECK (skipped IN (0,1)),
  offline           INTEGER CHECK (offline IN (0,1)),
  private_session   INTEGER NOT NULL DEFAULT 0 CHECK (private_session IN (0,1)),
  item_id           INTEGER REFERENCES item(id) ON DELETE SET NULL,
  reconciled_with   INTEGER REFERENCES listening_event(id) ON DELETE SET NULL,  -- logger event → its export partner
  import_batch_id   INTEGER REFERENCES import_batch(id) ON DELETE SET NULL,
  dedupe_hash       TEXT NOT NULL UNIQUE                 -- insert-or-ignore → idempotent import
);
CREATE UNIQUE INDEX one_to_one_reconcile ON listening_event(reconciled_with) WHERE reconciled_with IS NOT NULL;
CREATE INDEX listening_event_ended ON listening_event(ended_at_utc);
CREATE INDEX listening_event_norm  ON listening_event(norm_key);
CREATE INDEX listening_event_item  ON listening_event(item_id);
CREATE INDEX listening_event_uri   ON listening_event(spotify_track_uri);

CREATE TABLE exported_playlist (
  id              INTEGER PRIMARY KEY,
  import_batch_id INTEGER REFERENCES import_batch(id) ON DELETE SET NULL,
  name            TEXT NOT NULL,                         -- matched across exports by name (no ID in export)
  last_modified   TEXT
);
CREATE INDEX exported_playlist_name ON exported_playlist(name);

CREATE TABLE exported_playlist_item (
  playlist_id       INTEGER NOT NULL REFERENCES exported_playlist(id) ON DELETE CASCADE,
  position          INTEGER NOT NULL,
  track_name        TEXT,
  artist_name       TEXT,
  album_name        TEXT,
  spotify_track_uri TEXT,
  PRIMARY KEY (playlist_id, position)
) WITHOUT ROWID;
