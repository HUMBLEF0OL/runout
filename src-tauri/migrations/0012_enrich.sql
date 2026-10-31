-- 0012_enrich: source M, the open music database (BRD §5.1, TSD §5.4, M10 WI 9, enrich/). One row per
-- artist / album name looked up, misses included, so a name is never sent twice. Kept apart from the
-- user tables; the Settings switch empties all three in one transaction. Keys are lower(name), as the
-- 0008 rollups. `match`: matched = one confident hit; same_name = several artists share the name
-- (its own class: the top hit's labels, shown as "probably"); none = nothing confident. Never edit once shipped.
CREATE TABLE enrich_artist (
  artist       TEXT PRIMARY KEY,   -- lower(artist_name)
  match        TEXT NOT NULL CHECK (match IN ('matched', 'same_name', 'none')),
  via          TEXT CHECK (via IN ('musicbrainz', 'wikidata')),
  country      TEXT,               -- ISO 3166-1 alpha-2 (matched or same_name)
  looked_up_at TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE enrich_artist_genre (
  artist TEXT NOT NULL REFERENCES enrich_artist(artist) ON DELETE CASCADE,
  genre  TEXT NOT NULL,            -- MusicBrainz genre (CC BY-NC-SA 3.0) or Wikidata P136 label (CC0)
  votes  INTEGER NOT NULL,
  PRIMARY KEY (artist, genre)
) WITHOUT ROWID;

CREATE TABLE enrich_album (
  artist       TEXT NOT NULL,      -- lower(artist_name)
  album        TEXT NOT NULL,      -- lower(album_name)
  match        TEXT NOT NULL CHECK (match IN ('matched', 'same_name', 'none')),
  first_year   INTEGER,            -- release group first-release-date (matched or same_name)
  looked_up_at TEXT NOT NULL,
  PRIMARY KEY (artist, album)
) WITHOUT ROWID;
