-- 0008_listening_v3: Insights v3 data (M10 WI 2, design/M10-insights-v3.md §4.3, TSD §3). Never edit once shipped.
-- listening_event.shuffle already exists (0003); the logger now fills it from SMTC.

-- 4.3.1 Track length from SMTC (source S, desktop rows only; export rows stay NULL).
ALTER TABLE listening_event ADD COLUMN duration_ms INTEGER CHECK (duration_ms > 0);

-- 4.3.3 When the desktop logger was running (on and not paused). One row per interval; end_utc is NULL
-- while it runs. reason = how it ended: 'pause' | 'off' | 'exit' (also before a restore / Delete library)
-- | 'error' (SMTC unavailable) | 'crash' (closed at next start, at its last desktop play). Coverage (design §4.4) tells "no data" from "logger was off" with it.
CREATE TABLE logger_run (
  id        INTEGER PRIMARY KEY,
  start_utc TEXT NOT NULL,
  end_utc   TEXT,
  reason    TEXT CHECK (reason IN ('pause','off','exit','error','crash'))
);
CREATE INDEX logger_run_start ON logger_run(start_utc);

-- 4.3.4 Rollups over the base view (reconciled logger rows left out, private kept apart so the setting
-- still applies), kept by triggers like 0005 and rebuilt in one pass after an import (db::rebuild_rollups,
-- which runs the block between the rebuild markers below). Local time = ended_at_local, UTC when unknown.
-- Artist key = lower(artist_name), as insights' "new artists" (0004 index). ponytail: rows that drop to
-- 0 events stay; readers filter `events > 0`.
CREATE TABLE listening_day (
  day             TEXT NOT NULL,      -- local YYYY-MM-DD
  private_session INTEGER NOT NULL,
  events          INTEGER NOT NULL,
  plays           INTEGER NOT NULL,   -- counts_as_play
  ms_played       INTEGER NOT NULL,
  PRIMARY KEY (day, private_session)
) WITHOUT ROWID;

CREATE TABLE listening_month_artist (
  month           TEXT NOT NULL,      -- local YYYY-MM
  artist          TEXT NOT NULL,      -- lower(artist_name), '' when none
  private_session INTEGER NOT NULL,
  events          INTEGER NOT NULL,
  plays           INTEGER NOT NULL,
  ms_played       INTEGER NOT NULL,
  PRIMARY KEY (month, artist, private_session)
) WITHOUT ROWID;

-- First play (counts_as_play, D25) per artist (and private flag: with private sessions included, take the earlier
-- of the two). First = smallest (ended_at_utc, id). No row for events without an artist name; shorter events never
-- count as an artist's first play (design §4.2 New artist / First heard).
CREATE TABLE artist_first_play (
  private_session INTEGER NOT NULL,
  artist          TEXT NOT NULL,      -- lower(artist_name)
  event_id        INTEGER NOT NULL,
  ended_at_utc    TEXT NOT NULL,
  PRIMARY KEY (private_session, artist)
) WITHOUT ROWID;

-- rebuild:begin
DELETE FROM listening_day;
INSERT INTO listening_day
SELECT substr(l, 1, 10), private_session, COUNT(*), SUM(counts_as_play), SUM(ms_played)
FROM (SELECT *, COALESCE(ended_at_local, substr(ended_at_utc, 1, 19)) AS l FROM listening_event
      WHERE reconciled_with IS NULL)
GROUP BY 1, 2;
DELETE FROM listening_month_artist;
INSERT INTO listening_month_artist
SELECT substr(l, 1, 7), lower(IFNULL(artist_name, '')), private_session, COUNT(*), SUM(counts_as_play), SUM(ms_played)
FROM (SELECT *, COALESCE(ended_at_local, substr(ended_at_utc, 1, 19)) AS l FROM listening_event
      WHERE reconciled_with IS NULL)
GROUP BY 1, 2, 3;
DELETE FROM artist_first_play;
INSERT INTO artist_first_play
SELECT private_session, a, id, ended_at_utc FROM (
  SELECT private_session, lower(artist_name) AS a, id, ended_at_utc,
         ROW_NUMBER() OVER (PARTITION BY private_session, lower(artist_name) ORDER BY ended_at_utc, id) AS n
  FROM listening_event WHERE reconciled_with IS NULL AND IFNULL(artist_name, '') <> '' AND counts_as_play)
WHERE n = 1;
-- rebuild:end

-- Insert: add to the day and month, and keep the earlier first play.
CREATE TRIGGER listening_v3_add AFTER INSERT ON listening_event WHEN NEW.reconciled_with IS NULL BEGIN
  INSERT INTO listening_day VALUES (substr(COALESCE(NEW.ended_at_local, substr(NEW.ended_at_utc, 1, 19)), 1, 10),
    NEW.private_session, 1, NEW.counts_as_play, NEW.ms_played)
  ON CONFLICT DO UPDATE SET events = events + 1, plays = plays + excluded.plays, ms_played = ms_played + excluded.ms_played;
  INSERT INTO listening_month_artist VALUES (substr(COALESCE(NEW.ended_at_local, substr(NEW.ended_at_utc, 1, 19)), 1, 7),
    lower(IFNULL(NEW.artist_name, '')), NEW.private_session, 1, NEW.counts_as_play, NEW.ms_played)
  ON CONFLICT DO UPDATE SET events = events + 1, plays = plays + excluded.plays, ms_played = ms_played + excluded.ms_played;
  INSERT INTO artist_first_play SELECT NEW.private_session, lower(NEW.artist_name), NEW.id, NEW.ended_at_utc
  WHERE IFNULL(NEW.artist_name, '') <> '' AND NEW.counts_as_play
  ON CONFLICT DO UPDATE SET event_id = excluded.event_id, ended_at_utc = excluded.ended_at_utc
  WHERE (excluded.ended_at_utc, excluded.event_id) < (ended_at_utc, event_id);
END;

-- Delete: a plain UPDATE (never an upsert), so emptying the tables in any order (Delete library) stays safe.
CREATE TRIGGER listening_v3_remove AFTER DELETE ON listening_event WHEN OLD.reconciled_with IS NULL BEGIN
  UPDATE listening_day SET events = events - 1, plays = plays - OLD.counts_as_play, ms_played = ms_played - OLD.ms_played
  WHERE day = substr(COALESCE(OLD.ended_at_local, substr(OLD.ended_at_utc, 1, 19)), 1, 10)
    AND private_session = OLD.private_session;
  UPDATE listening_month_artist SET events = events - 1, plays = plays - OLD.counts_as_play, ms_played = ms_played - OLD.ms_played
  WHERE month = substr(COALESCE(OLD.ended_at_local, substr(OLD.ended_at_utc, 1, 19)), 1, 7)
    AND artist = lower(IFNULL(OLD.artist_name, '')) AND private_session = OLD.private_session;
END;

-- An update of a counted column = remove the old row, add the new one (the two triggers above, by hand).
-- Other updates (item_id from link_play / matching) leave the rollups alone.
CREATE TRIGGER listening_v3_change AFTER UPDATE OF reconciled_with, private_session, ended_at_utc, ended_at_local,
    ms_played, artist_name ON listening_event BEGIN
  UPDATE listening_day SET events = events - 1, plays = plays - OLD.counts_as_play, ms_played = ms_played - OLD.ms_played
  WHERE OLD.reconciled_with IS NULL
    AND day = substr(COALESCE(OLD.ended_at_local, substr(OLD.ended_at_utc, 1, 19)), 1, 10)
    AND private_session = OLD.private_session;
  UPDATE listening_month_artist SET events = events - 1, plays = plays - OLD.counts_as_play, ms_played = ms_played - OLD.ms_played
  WHERE OLD.reconciled_with IS NULL
    AND month = substr(COALESCE(OLD.ended_at_local, substr(OLD.ended_at_utc, 1, 19)), 1, 7)
    AND artist = lower(IFNULL(OLD.artist_name, '')) AND private_session = OLD.private_session;
  INSERT INTO listening_day SELECT substr(COALESCE(NEW.ended_at_local, substr(NEW.ended_at_utc, 1, 19)), 1, 10),
    NEW.private_session, 1, NEW.counts_as_play, NEW.ms_played
  WHERE NEW.reconciled_with IS NULL
  ON CONFLICT DO UPDATE SET events = events + 1, plays = plays + excluded.plays, ms_played = ms_played + excluded.ms_played;
  INSERT INTO listening_month_artist SELECT substr(COALESCE(NEW.ended_at_local, substr(NEW.ended_at_utc, 1, 19)), 1, 7),
    lower(IFNULL(NEW.artist_name, '')), NEW.private_session, 1, NEW.counts_as_play, NEW.ms_played
  WHERE NEW.reconciled_with IS NULL
  ON CONFLICT DO UPDATE SET events = events + 1, plays = plays + excluded.plays, ms_played = ms_played + excluded.ms_played;
  INSERT INTO artist_first_play SELECT NEW.private_session, lower(NEW.artist_name), NEW.id, NEW.ended_at_utc
  WHERE NEW.reconciled_with IS NULL AND IFNULL(NEW.artist_name, '') <> '' AND NEW.counts_as_play
  ON CONFLICT DO UPDATE SET event_id = excluded.event_id, ended_at_utc = excluded.ended_at_utc
  WHERE (excluded.ended_at_utc, excluded.event_id) < (ended_at_utc, event_id);
END;

-- The first play itself was deleted, or stopped counting for its artist (moved, renamed, reconciled, under 30 s): recount that artist (index 0004).
CREATE TRIGGER artist_first_play_gone AFTER DELETE ON listening_event
WHEN EXISTS (SELECT 1 FROM artist_first_play WHERE private_session = OLD.private_session
               AND artist = lower(IFNULL(OLD.artist_name, '')) AND event_id = OLD.id) BEGIN
  DELETE FROM artist_first_play WHERE private_session = OLD.private_session AND artist = lower(OLD.artist_name);
  INSERT INTO artist_first_play SELECT private_session, lower(artist_name), id, ended_at_utc FROM listening_event
  WHERE lower(artist_name) = lower(OLD.artist_name) AND private_session = OLD.private_session
    AND reconciled_with IS NULL AND counts_as_play
  ORDER BY ended_at_utc, id LIMIT 1;
END;

CREATE TRIGGER artist_first_play_moved AFTER UPDATE OF reconciled_with, private_session, ended_at_utc, artist_name, ms_played
ON listening_event
WHEN EXISTS (SELECT 1 FROM artist_first_play WHERE private_session = OLD.private_session
               AND artist = lower(IFNULL(OLD.artist_name, '')) AND event_id = OLD.id) BEGIN
  DELETE FROM artist_first_play WHERE private_session = OLD.private_session AND artist = lower(OLD.artist_name);
  INSERT INTO artist_first_play SELECT private_session, lower(artist_name), id, ended_at_utc FROM listening_event
  WHERE lower(artist_name) = lower(OLD.artist_name) AND private_session = OLD.private_session
    AND reconciled_with IS NULL AND counts_as_play
  ORDER BY ended_at_utc, id LIMIT 1;
END;

-- G-45 (M10 WI 3): skip rate = skipped ÷ events with a skip flag (desktop rows have none). All-time
-- reads count them on this index, not from the 0005 rollup, whose `events` includes desktop rows.
CREATE INDEX listening_event_skip_flag ON listening_event(private_session, reconciled_with, skipped)
  WHERE skipped IS NOT NULL;

-- 4.4 Coverage (M10 WI 4, insights/coverage.rs): the desktop log's first play and its dates in a window,
-- without walking the export rows that come before it.
CREATE INDEX listening_event_logger ON listening_event(ended_at_utc) WHERE source = 'logger';
