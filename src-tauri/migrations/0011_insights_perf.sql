-- 0011_insights_perf: indexes for Insights v3 at 500k events (M10 WI 5, NFR-04, design §6). Never edit once shipped.

-- "Heard before" (repeat plays in the fingerprint): a track's earliest rows by one seek, instead of visiting
-- every row of the track. Replaces 0003's listening_event_norm (same leading column). Not a partial index: a
-- partial one on the play rule is picked for whole-table scans and reads every row through it.
DROP INDEX listening_event_norm;
CREATE INDEX listening_event_norm_time ON listening_event(norm_key, private_session, ended_at_utc);
