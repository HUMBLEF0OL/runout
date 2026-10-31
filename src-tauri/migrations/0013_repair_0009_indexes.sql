-- 0013_repair_0009_indexes (M10 WI 10): a library migrated by a pre-release 0009 (no indexes) has
-- user_version 9 without the two indexes habits.rs names with INDEXED BY, so How you listen failed
-- with "no such index" on the real history. IF NOT EXISTS: a no-op on every other library. Never edit once shipped.
CREATE INDEX IF NOT EXISTS listening_event_duration ON listening_event(norm_key, duration_ms) WHERE duration_ms IS NOT NULL;
CREATE INDEX IF NOT EXISTS listening_event_shuffle ON listening_event(shuffle, private_session, reconciled_with, ended_at_local, ended_at_utc)
  WHERE shuffle IS NOT NULL;
