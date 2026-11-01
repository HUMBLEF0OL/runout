-- 0004_listening_perf: NFR-04 at 500k events (M4 WI 9, TSD §3). Never edit once shipped.
-- "New artists" in a range looks up each artist's earlier plays (insights/listening.rs).
CREATE INDEX listening_event_artist ON listening_event(lower(artist_name), ended_at_utc);
