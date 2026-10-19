-- 0005_listening_rollup: all-time reads over 500k events (NFR-04, G-17, TSD §3). Never edit once shipped.
-- Per-record sums of the base view (not reconciled; private kept apart so the setting still applies),
-- kept in step by triggers so every writer (insert, reconcile, link_play, matching, FK SET NULL,
-- delete) updates it. Keys are NOT NULL (0 / '' for NULL) so the upsert can match them.
-- ponytail: rows that drop to 0 events stay; readers filter `events > 0`.
CREATE TABLE listening_rollup (
  private_session INTEGER NOT NULL,
  item_id         INTEGER NOT NULL,
  norm_key        TEXT NOT NULL,
  track_name      TEXT NOT NULL,
  artist_name     TEXT NOT NULL,
  album_name      TEXT NOT NULL,
  events          INTEGER NOT NULL,
  counts_as_play  INTEGER NOT NULL,
  ms_played       INTEGER NOT NULL,
  skips           INTEGER NOT NULL,
  exported        INTEGER NOT NULL,   -- export rows that count as a play
  phone           INTEGER NOT NULL,   -- of those, iOS / Android platforms
  PRIMARY KEY (private_session, item_id, norm_key, track_name, artist_name, album_name)
) WITHOUT ROWID;

-- Listening time by local weekday (Mon = 0) × hour, for the all-time heatmap.
CREATE TABLE listening_heat (
  private_session INTEGER NOT NULL,
  dow             INTEGER NOT NULL,
  hour            INTEGER NOT NULL,
  ms_played       INTEGER NOT NULL,
  PRIMARY KEY (private_session, dow, hour)
) WITHOUT ROWID;

INSERT INTO listening_rollup
SELECT private_session, IFNULL(item_id, 0), IFNULL(norm_key, ''), IFNULL(track_name, ''),
       IFNULL(artist_name, ''), IFNULL(album_name, ''), COUNT(*), SUM(counts_as_play), SUM(ms_played),
       SUM(IFNULL(skipped, 0)), SUM(source = 'export' AND counts_as_play),
       SUM(CASE WHEN source = 'export' AND counts_as_play
                 AND (lower(platform) LIKE '%ios%' OR lower(platform) LIKE '%android%') THEN 1 ELSE 0 END)
FROM listening_event WHERE reconciled_with IS NULL GROUP BY 1, 2, 3, 4, 5, 6;

INSERT INTO listening_heat
SELECT private_session, dow, hour, SUM(ms_played) FROM (
  SELECT private_session, ms_played,
         IFNULL((CAST(strftime('%w', l) AS INT) + 6) % 7, -1) AS dow, IFNULL(CAST(strftime('%H', l) AS INT), -1) AS hour
  FROM (SELECT *, COALESCE(ended_at_local, substr(ended_at_utc, 1, 19)) AS l FROM listening_event
        WHERE reconciled_with IS NULL))
GROUP BY 1, 2, 3;

CREATE TRIGGER listening_rollup_add AFTER INSERT ON listening_event WHEN NEW.reconciled_with IS NULL BEGIN
  INSERT INTO listening_rollup VALUES (NEW.private_session, IFNULL(NEW.item_id, 0), IFNULL(NEW.norm_key, ''),
    IFNULL(NEW.track_name, ''), IFNULL(NEW.artist_name, ''), IFNULL(NEW.album_name, ''), 1, NEW.counts_as_play,
    NEW.ms_played, IFNULL(NEW.skipped, 0), NEW.source = 'export' AND NEW.counts_as_play,
    CASE WHEN NEW.source = 'export' AND NEW.counts_as_play
          AND (lower(NEW.platform) LIKE '%ios%' OR lower(NEW.platform) LIKE '%android%') THEN 1 ELSE 0 END)
  ON CONFLICT DO UPDATE SET events = events + 1, counts_as_play = counts_as_play + excluded.counts_as_play,
    ms_played = ms_played + excluded.ms_played, skips = skips + excluded.skips,
    exported = exported + excluded.exported, phone = phone + excluded.phone;
  INSERT INTO listening_heat VALUES (NEW.private_session,
    IFNULL((CAST(strftime('%w', COALESCE(NEW.ended_at_local, substr(NEW.ended_at_utc, 1, 19))) AS INT) + 6) % 7, -1),
    IFNULL(CAST(strftime('%H', COALESCE(NEW.ended_at_local, substr(NEW.ended_at_utc, 1, 19))) AS INT), -1),
    NEW.ms_played)
  ON CONFLICT DO UPDATE SET ms_played = ms_played + excluded.ms_played;
END;

-- Removal is a plain UPDATE (never an upsert): emptying the tables in any order (Delete library) stays safe.
CREATE TRIGGER listening_rollup_remove AFTER DELETE ON listening_event WHEN OLD.reconciled_with IS NULL BEGIN
  UPDATE listening_rollup SET events = events - 1, counts_as_play = counts_as_play - OLD.counts_as_play,
    ms_played = ms_played - OLD.ms_played, skips = skips - IFNULL(OLD.skipped, 0),
    exported = exported - (OLD.source = 'export' AND OLD.counts_as_play),
    phone = phone - (CASE WHEN OLD.source = 'export' AND OLD.counts_as_play
          AND (lower(OLD.platform) LIKE '%ios%' OR lower(OLD.platform) LIKE '%android%') THEN 1 ELSE 0 END)
  WHERE private_session = OLD.private_session AND item_id = IFNULL(OLD.item_id, 0)
    AND norm_key = IFNULL(OLD.norm_key, '') AND track_name = IFNULL(OLD.track_name, '')
    AND artist_name = IFNULL(OLD.artist_name, '') AND album_name = IFNULL(OLD.album_name, '');
  UPDATE listening_heat SET ms_played = ms_played - OLD.ms_played
  WHERE private_session = OLD.private_session
    AND dow = IFNULL((CAST(strftime('%w', COALESCE(OLD.ended_at_local, substr(OLD.ended_at_utc, 1, 19))) AS INT) + 6) % 7, -1)
    AND hour = IFNULL(CAST(strftime('%H', COALESCE(OLD.ended_at_local, substr(OLD.ended_at_utc, 1, 19))) AS INT), -1);
END;

-- An update = remove the old row, add the new one (the two triggers above, by hand).
CREATE TRIGGER listening_rollup_change AFTER UPDATE ON listening_event BEGIN
  UPDATE listening_rollup SET events = events - 1, counts_as_play = counts_as_play - OLD.counts_as_play,
    ms_played = ms_played - OLD.ms_played, skips = skips - IFNULL(OLD.skipped, 0),
    exported = exported - (OLD.source = 'export' AND OLD.counts_as_play),
    phone = phone - (CASE WHEN OLD.source = 'export' AND OLD.counts_as_play
          AND (lower(OLD.platform) LIKE '%ios%' OR lower(OLD.platform) LIKE '%android%') THEN 1 ELSE 0 END)
  WHERE OLD.reconciled_with IS NULL AND private_session = OLD.private_session AND item_id = IFNULL(OLD.item_id, 0)
    AND norm_key = IFNULL(OLD.norm_key, '') AND track_name = IFNULL(OLD.track_name, '')
    AND artist_name = IFNULL(OLD.artist_name, '') AND album_name = IFNULL(OLD.album_name, '');
  UPDATE listening_heat SET ms_played = ms_played - OLD.ms_played
  WHERE OLD.reconciled_with IS NULL AND private_session = OLD.private_session
    AND dow = IFNULL((CAST(strftime('%w', COALESCE(OLD.ended_at_local, substr(OLD.ended_at_utc, 1, 19))) AS INT) + 6) % 7, -1)
    AND hour = IFNULL(CAST(strftime('%H', COALESCE(OLD.ended_at_local, substr(OLD.ended_at_utc, 1, 19))) AS INT), -1);
  INSERT INTO listening_rollup SELECT NEW.private_session, IFNULL(NEW.item_id, 0), IFNULL(NEW.norm_key, ''),
    IFNULL(NEW.track_name, ''), IFNULL(NEW.artist_name, ''), IFNULL(NEW.album_name, ''), 1, NEW.counts_as_play,
    NEW.ms_played, IFNULL(NEW.skipped, 0), NEW.source = 'export' AND NEW.counts_as_play,
    CASE WHEN NEW.source = 'export' AND NEW.counts_as_play
          AND (lower(NEW.platform) LIKE '%ios%' OR lower(NEW.platform) LIKE '%android%') THEN 1 ELSE 0 END
  WHERE NEW.reconciled_with IS NULL
  ON CONFLICT DO UPDATE SET events = events + 1, counts_as_play = counts_as_play + excluded.counts_as_play,
    ms_played = ms_played + excluded.ms_played, skips = skips + excluded.skips,
    exported = exported + excluded.exported, phone = phone + excluded.phone;
  INSERT INTO listening_heat SELECT NEW.private_session,
    IFNULL((CAST(strftime('%w', COALESCE(NEW.ended_at_local, substr(NEW.ended_at_utc, 1, 19))) AS INT) + 6) % 7, -1),
    IFNULL(CAST(strftime('%H', COALESCE(NEW.ended_at_local, substr(NEW.ended_at_utc, 1, 19))) AS INT), -1),
    NEW.ms_played
  WHERE NEW.reconciled_with IS NULL
  ON CONFLICT DO UPDATE SET ms_played = ms_played + excluded.ms_played;
END;
