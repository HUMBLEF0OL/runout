-- 0009_import_already_had (+ the How you listen indexes below): Your data's import history (M10 WI 5, API.md §4.1 DataCoverage.imports): how many rows of each
-- export batch were already stored (insert-or-ignore duplicates). Batches imported before 0009 show 0. Never edit once shipped.
ALTER TABLE import_batch ADD COLUMN already_had INTEGER NOT NULL DEFAULT 0;

-- How far into a track (How you listen, design §4.3.1): the longest known length per track, read for a
-- period's tracks. Only desktop rows carry duration_ms, so the index stays small.
CREATE INDEX listening_event_duration ON listening_event(norm_key, duration_ms) WHERE duration_ms IS NOT NULL;

-- Shuffle by year (How you listen) reads this covering index instead of every row.
CREATE INDEX listening_event_shuffle ON listening_event(shuffle, private_session, reconciled_with, ended_at_local, ended_at_utc)
  WHERE shuffle IS NOT NULL;
