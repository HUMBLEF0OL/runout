-- FR-022 / TSD §4.11: deleted notes and reviews must not stay readable in library.db. Plain FTS5
-- deletes only add tombstones, and VACUUM copies the old index segments, so the tokens survive.
-- 'secure-delete' (persistent FTS5 option, SQLite >= 3.42) makes every delete remove them;
-- 'optimize' merges existing segments so text deleted before this migration is dropped too.
INSERT INTO item_fts(item_fts, rank) VALUES('secure-delete', 1);
INSERT INTO item_fts(item_fts) VALUES('optimize');
