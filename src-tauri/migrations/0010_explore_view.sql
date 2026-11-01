-- 0010_explore_view: Explore's saved views (M10 WI 5, API.md §4.1 ExploreView), local only, in
-- backups, like saved_filter (FR-013). Holds the builder only, never a result or a fingerprint score.
-- Never edit once shipped.
CREATE TABLE explore_view (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL,
  query_json TEXT NOT NULL
);
