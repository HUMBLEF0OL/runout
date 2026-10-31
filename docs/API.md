# Command Contract — Runout

| | |
|---|---|
| Version | 1.0 |
| Date | 18 Oct 2026 |
| Implements | [TSD §1 rule](TSD.md#1-technology-decisions-resolves-brd-o3), [DESIGN §6](DESIGN.md#6-screen-inventory-m1-designs-all-of-these) |
| Used by | M2 (`src/mocks/` implements it) · M3–M5 (`src-tauri/src/commands.rs` implements it) · M6–M7 (`src/api.ts` switches from mocks to commands) |

The **only** bridge between the UI and the Rust core. Each command and type below exists twice, with the same names and fields: in TypeScript (`src/api.ts`) and in Rust (`commands.rs` + `serde` structs). A change here comes first, then both sides follow in the same change.

## 1. Conventions
- **Arg keys.** Tauri maps Rust `snake_case` parameters to `camelCase` invoke keys, so multi-word args are sent as `fromId`, `playlistUri` etc. (`src/api.ts` does this).
- **Names.** Rust commands are `snake_case` (`get_item`); `api.ts` exports the same name in `camelCase` (`getItem`). Fields are `snake_case` on both sides (matches TSD §3 and the mock data); no `serde(rename_all)`.
- **Ids** are `number` (SQLite `INTEGER`). **Times** are ISO-8601 UTC strings; `*_local` fields are local time without offset.
- **Results.** Every command returns `Result<T, AppError>`; `invoke` rejects with the `AppError` object. Pages never catch raw strings.
- **Pickers.** File/folder dialogs are opened **by Rust** (`tauri-plugin-dialog` called from the command), never by the UI. A picker command returns `null` when cancelled. The UI has no fs or dialog permission (TSD §7).
- **Events** (Rust → UI) use `listen()` and are listed in §6. The UI never polls faster than 30 s.
- **Releases.** Commands are grouped by release. M2 mocks all of them; M6 switches §3, M7 switches §4 and §5.
- **Mocks.** `src/mocks/` serves the shared dataset `docs/design/prototype/mock-data.json` (copied at build time, never hand-edited) and keeps edits in memory. A dev-only state switcher forces any screen into `loading | empty | offline | error`, like the prototype controls. Mock delay: 150 ms.

```ts
type AppErrorCode =
  | 'not_found' | 'validation' | 'duplicate'            // duplicate carries existing_id (FR-004)
  | 'invalid_link' | 'unsupported_link'                 // unsupported = episode/show/audiobook/playlist-as-item (D23)
  | 'offline' | 'io' | 'integrity' | 'conflict'          // conflict = playlist changed on Spotify (TSD §5.2)
  | 'spotify_auth' | 'spotify_forbidden' | 'spotify_busy' // invalid_grant · 403 · 429
  | 'import_unrecognized' | 'internal';
interface AppError { code: AppErrorCode; message: string; existing_id?: number; retry_after_s?: number; detail?: string }
interface Page<T> { rows: T[]; total: number; offset: number }
```

## 2. Shared types
```ts
type ItemType = 'album' | 'track' | 'artist';
type Status = 'planned' | 'in_progress' | 'explored' | 'not_for_me' | 'abandoned';
type Priority = 'high' | 'medium' | 'low';
type Source = 'U' | 'S' | 'E' | 'M';                    // never 'A' in insights (DESIGN §7); M = open music database labels (M10 WI 9, only from get_genres_decades)
type Range = '7d' | '30d' | '12m' | 'all';

interface ItemSummary {
  id: number; type: ItemType; display_name: string | null;   // null = name needed
  spotify_uri: string | null; status: Status; priority: Priority | null;
  rating: number | null; favourite: boolean; revisit: boolean; in_backlog: boolean;
  backlog_order: number | null; tags: string[]; artist_names: string[];
  album_name: string | null;                                // tracks: "Artist, from Album" row subtitle (M2)
  added_via: 'pc' | 'phone' | 'inbox';                      // "From: Phone", "added from your iPhone" (M2); inbox = Runout Inbox (M7, TSD §5.3)
  recommended_by: string | null;                            // "Album, from Meera" row subtitle (M2)
  notes: string | null;                                     // in-progress artist cards show it (G-4, M3)
  added_at: string; modified_at: string;
}
interface ItemDetail extends ItemSummary {
  review: string | null; notes: string | null; recommended_by: string | null;
  explored_at: string | null; reviewed_at: string | null;
  track_list: { position: number; name: string; item_id: number | null; plays: number }[] | null;  // albums with a known track list (TSD §4.9); plays = counted plays of that track, 0 = "Not played yet" (G-1)
  provenance: Record<string, 'user' | 'promoted'>;
  links: { kind: 'by_artist' | 'on_album' | 'version_of'; direction: 'from' | 'to'; item: ItemSummary }[];
  history: { from: Status | null; to: Status; at: string; note: string | null }[];
}
interface NewItem {
  type: ItemType; display_name: string | null; spotify_uri: string | null;
  status?: Status; priority?: Priority; in_backlog?: boolean; tags?: string[];
  recommended_by?: string; provenance: 'user' | 'promoted';   // promoted = confirmed from Spotify data (TSD §4.2)
  promoted_fields?: string[];                                 // fields confirmed from Spotify data → provenance 'promoted'; others get `provenance`
  capture_ms?: number;                                        // dialog open → save (FR-045)
  artist_name?: string; notes?: string;                       // artist_name: find-or-create the artist item, link by_artist
  album_id?: number;                                          // tracks: link the new track on_album to this album (G-3, M3)
}
type ItemPatch = Partial<Pick<ItemDetail,
  'display_name' | 'priority' | 'rating' | 'review' | 'notes' | 'favourite' | 'revisit'
  | 'in_backlog' | 'recommended_by'>> & { type?: ItemType; tags?: string[]; promoted_fields?: string[]; artist_name?: string | null };  // artist_name re-links by_artist (null unlinks); type changes the record type (G-2)
interface ItemFilter {
  type?: ItemType; status?: Status[]; tags?: string[]; min_rating?: number; revisit?: boolean;
  favourite?: boolean; in_backlog?: boolean; name_needed?: boolean; priority?: Priority[];
  sort?: 'backlog_order' | 'added_at' | 'modified_at' | 'rating' | 'name'; desc?: boolean;
  offset?: number; limit?: number;                            // default 50
}
interface Tag { id: number; name: string; description: string | null; item_count: number }
interface SavedFilter { id: number; name: string; definition: ItemFilter }
interface Freshness { logger_last_event: string | null; export_latest_event: string | null }
interface Sourced<T> { data: T; sources: Source[]; freshness: Freshness }   // every insight (FR-032)
```

## 3. MVP commands (backend M3 · switched in M6)
| Command | Args | Returns | Notes / spec |
|---|---|---|---|
| `get_counts` | — | `{library, albums, tracks, artists, backlog, in_progress, revisit, name_needed}` | Rail counts (DESIGN §5 nav item) |
| `get_dashboard` | — | `{counts, in_progress: ItemSummary[], name_needed: ItemSummary[], unresolved: Unresolved[], planned, explored_this_month, not_for_me_this_month, explored_no_review, started_at: {[item_id]: string}}` | SC-01/02, FR-015. `started_at`: when each in-progress item entered in_progress. "Recently added" comes from `list_items` (G-5: `recent_activity` dropped in M3) |
| `list_items` | `filter: ItemFilter` | `Page<ItemSummary>` | SC-03, SC-04 |
| `get_item` | `id` | `ItemDetail` | SC-05 |
| `create_item` | `input: NewItem` | `ItemDetail` | `duplicate` error with `existing_id` (FR-004); `capture` usage event |
| `update_item` | `id, patch: ItemPatch` | `ItemDetail` | `promoted_fields` sets provenance `promoted` for those fields. A `type` change drops links that no longer fit (e.g. `on_album` when it is no longer a track) |
| `set_status` | `id, status, note?` | `ItemDetail` | Writes `status_history` (FR-005/006). Into `explored`/`not_for_me`: sets `explored_at`, leaves the backlog. Out of them: clears `explored_at`; back to `planned` rejoins the backlog at the end. `create_item` with a `status` gets the same effects |
| `delete_item` | `id` | `void` | FR-022 |
| `link_items` / `unlink_items` | `from_id, to_id, kind` | `void` | FR-003/004 versions. Direction: track → album (`on_album`), item → artist (`by_artist`), version → original (`version_of`) |
| `reorder_backlog` | `ids: number[]` | `void` | Whole visible order; FR-012 |
| `parse_link` | `raw: string` | `{state: 'ok' \| 'short' \| 'unsupported' \| 'invalid', uri?, type?, existing_id?}` | Pure; TSD §4.1. `short` = needs resolve |
| `resolve_link` | `raw: string` | `{uri, type, existing_id?}` | Short link → URI; `offline` error queues it in `unresolved_capture` |
| `prefill_title` | `uri` | `{title: string \| null}` | oEmbed; **response never stored** (TSD §4.3) |
| `list_unresolved` / `retry_unresolved` / `discard_unresolved` | — / — / `id` | `Unresolved[]` / `{resolved: number}` / `void` | `Unresolved = {id, raw_link, received_at, source, state, error}` |
| `search` | `q, limit?, type?` | `{item: ItemSummary, snippet: string, field: string}[]` | FTS5, < 200 ms (NFR-04). `type` limits hits to one record type (edit-mode link picker, M3) |
| `list_tags` / `rename_tag` / `merge_tags` / `delete_tag` | — / `id, name` / `from_id, into_id` / `id` | `Tag[]` / `Tag` / `Tag` / `void` | SC-13b |
| `list_saved_filters` / `save_filter` / `delete_saved_filter` | — / `name, definition` / `id` | `SavedFilter[]` / `SavedFilter` / `void` | FR-013. `save_filter` trims `definition.tags` and drops empties and case-insensitive duplicates; `rename_tag`/`merge_tags` rewrite saved filters' tags |
| `get_rediscover` | — | `{revisit, top_rated, stale_backlog, incomplete_reviews: ItemSummary[]}` | SC-07, FR-013 |
| `get_online` | — | `boolean` | `net::online()`, cached 30 s (TSD §4.3) |
| `get_settings` / `update_settings` | — / `patch` | `Settings` | `{library_path, backup_dir, backup_weekly, size_warn_mb, artwork_default, export_reminder, music_db}` (`export_reminder`, `music_db` M10, §4.1); `music_db` is set only by `set_music_db` |
| `get_capture` / `set_capture_enabled` / `new_pairing_code` | — / `on` / — | `{on, address, port, code}` | SC-13e; TSD §5.1 |
| `export_library` | — | `{path, count} \| null` | Save picker; JSON with provenance (FR-020) |
| `list_backups` / `backup_now` | — | `Backup[]` / `Backup` | `Backup = {path, created_at, size_bytes, item_count, size_warning}` |
| `pick_backup` / `check_backup` / `restore_backup` | — / `path` / `path` | `string \| null` / `RestoreCheck` / `RestoreCheck` | `{ok, schema_version, counts, error?}`; TSD §4.10 |
| `choose_backup_dir` / `move_library` | — | `string \| null` | Pickers (D15, D28) |
| `delete_library` | `confirm: string` | `void` | Requires typed phrase (SC-13f) |
| `record_page_visit` | `page: string` | `void` | `app_open` is recorded by Rust at start (TSD §4.12) |
| `get_success_metrics` | — | `{capture_median_s, days_opened_this_week, searches_this_week, rediscover_visits_this_week, moved_after_14d_pct, explored_rated_pct}` | BRD §15 |

## 4. R2 commands (backend M4 · switched in M7)
| Command | Args | Returns | Notes / spec |
|---|---|---|---|
| `get_logger_status` | — | `{state: 'listening' \| 'idle' \| 'paused' \| 'off' \| 'error', now_playing: NowPlaying \| null, last_event_at, paused_until, autostart, include_private}` | Tray + SC-08; `NowPlaying = {track_name, artist_name, album_name, spotify_uri, ms_played, duration_ms, item}` |
| `set_logger` | `patch: {on?, autostart?, include_private?}` | same as above | FR-026 |
| `pause_logger` / `resume_logger` | `minutes: number \| null` / — | same | null = until resumed |
| `list_plays` | `q: {tab: 'all' \| 'unrated' \| 'missing', range, offset, limit, date?}` | `Page<Play>` | `Play = {id, ended_at_local, ms_played, counts_as_play, skipped, track_name, artist_name, album_name, source: 'logger' \| 'export', spotify_uri, item: ItemSummary \| null, album_rating: number \| null}`; `date?: 'YYYY-MM-DD'` (local, M10: Trends calendar day → Recent plays) keeps that day's plays only and replaces `range`; Rust in M10 WI 5. `item` is the track record, `album_rating` the rating of the play's album record (null if none or unrated) |
| `list_unmatched` / `link_play` | — / `event_id, item_id` | `Unmatched[]` / `void` | Grouped by `norm_key` with play count (FR-030); `Unmatched = {norm_key, track_name, artist_name, plays, detail, suggestion, event_id, candidates: ItemSummary[]}`; `candidates[0]` is the suggested record, the rest are the other records it could be (for "Choose another") |
| `get_item_listening` | `id` | `Sourced<{plays, ms_total, last_played_local, by_month}>` | SC-05 listening panel |
| `pick_import_source` | — | `string \| null` | Folder or .zip |
| `preview_import` | `path` | `{music, podcasts_skipped, overlap, playlists, private, from, until, files: string[]}` | No writes (SC-10 step 2) |
| `run_import` | `path` | `{batch_id, added, duplicates, reconciled, playlists, took_ms}` | One transaction; `import_unrecognized` on bad file (TSD §6) |
| `list_imports` | — | `{id, kind, imported_at, record_count, latest_event_utc: string \| null}[]` | Settings; `latest_event_utc` is null for `account` batches (no plays) |
| `get_freshness` | — | `Freshness` | Rail freshness box |
| `get_listening_insights` | `range` | `Covered<{top_artists, top_albums, top_tracks, monthly, heatmap, skip_rate, phone_share, totals}>` | FR-031; no source A. `coverage` and `totals.plays_cmp`/`ms_cmp`: §4.1 (M10, G-46) |
| `get_collection_insights` | `range` | `Sourced<{backlog_growth, completion, rating_spread, tag_trends}>` | |
| `get_take_insights` | — | `Sourced<{rated_vs_played, never_played_backlog, loved_but_forgotten, unfinished_albums, unfinished_unknown}>` | Unfinished: only albums with a track list (TSD §4.9); `unfinished_unknown` = albums without one ("N other albums can't be checked") |
| `get_year_review` | `year` | `Sourced<{plays, hours, top_artist, top_album, explored, reviews, busiest_month}>` | FR-034 |
| `list_exported_playlists` / `get_playlist_checkup` | — / `id` | `{id, name, tracks, dupes, low}[]` / `{tracks: {position, name, artist, dupe, rating, never_played}[]}` | Read-only (FR-033) |

R2 payload shapes (insights, check-up):
```ts
interface RankRow { name: string; by: string; plays: number; hours: number; item_id: number | null; rating: number | null; status: Status | null }
interface PeriodStats { plays: number; plays_prev: number; ms: number; ms_prev: number; plays_cmp: number; ms_cmp: number; new_artists: number; rated_this_period: number; unrated_plays: number }   // *_cmp / *_prev: like for like over coverage.compared_sources (§4.1); a delta is cmp − prev
interface ListeningInsights {
  top_artists: RankRow[]; top_albums: RankRow[]; top_tracks: RankRow[];
  monthly: { month: string; hours: number }[]; heatmap: number[][];
  skip_rate: number; phone_share: number; totals: PeriodStats & { private_excluded: number; artists_total: number };
}
interface CollectionInsights {
  backlog_growth: { month: string; added: number; finished: number }[];
  completion: { status: Status; count: number }[];
  rating_spread: { rating: number; count: number }[];
  tag_trends: { tag: string; count: number }[];
  avg_days_to_explored: number | null; reviews: number; explored_without_review: number;
}
interface Take { item: ItemSummary; line: string }
interface TakeInsights {
  rated_vs_played: Take[]; never_played_backlog: Take[]; loved_but_forgotten: Take[];
  unfinished_albums: { item: ItemSummary; played: number; total: number }[];
  unfinished_unknown: number;   // albums in the library without a track list (can't be checked)
}
interface YearReview {
  year: number; plays: number; hours: number; explored: number; reviews: number; busiest_month: string;
  top_artist: RankRow; top_album: RankRow;
}
interface ExportedPlaylist { id: number; name: string; tracks: number; dupes: number; low: number }
interface PlaylistCheckup { tracks: { position: number; name: string; artist: string; dupe: boolean; rating: number | null; never_played: boolean; tags: string[] }[] }
```

### 4.1 Insights v3 (M10 · D35, FR-047, FR-048)
Every definition and threshold: [design/M10-insights-v3.md](design/M10-insights-v3.md) §4.2 (TSD §4.14). Sources U, S, E only, private sessions excluded per the logger setting, like §4.9. The listening tabs of Insights take a `PeriodQuery` (the page header); Home, Recent plays and the Collection tab keep `Range`.

| Command | Args | Returns | Notes / spec |
|---|---|---|---|
| `get_period_report` | `period: PeriodQuery` | `Covered<PeriodReport>` | Report tab (design §5) and Year in review's year cards (`grain: 'year'`). `validation` for a malformed `at`/`from`/`to` or `custom` without both dates |
| `get_trends` | `measure: 'hours' \| 'plays' \| 'artists'` | `Covered<Trends>` | Trends tab, whole history; `measure` changes `months` only |
| `get_discovery` | `period: PeriodQuery` | `Covered<Discovery>` | Discovery tab |
| `get_listening_habits` | `period: PeriodQuery` | `Covered<Habits>` | How you listen. Export plays only, except `devices` (desktop plays are "This PC") and sessions (all plays) |
| `get_records` | — | `Covered<Records>` | Records tab; all-time figures use all-device dates only (design §4.4) |
| `explore` | `query: ExploreQuery` | `Covered<ExploreResult>` | Explore, < 200 ms (NFR-04). `validation` for 0 or more than 3 series, `from` after `to`, or an hour/weekday out of range. A name with no plays gives a series of zeros, not an error |
| `get_data_coverage` | — | `Covered<DataCoverage>` | Your data tab; `coverage.gaps` = every no-data stretch in the whole history |
| `get_genres_decades` | `period: PeriodQuery` | `Covered<GenresDecades>` | Genres & decades tab (M10 WI 9, design §7, TSD §5.4): S/E listening time labelled from the `enrich` tables (source M; `sources` gains `'M'` when any label is used). No comparison: `coverage` is the period's gaps only. `validation` like `get_period_report` |
| `set_music_db` | `on: boolean` | `Settings` | The music database switch: off stops lookups and deletes every `enrich` row in one transaction; on resumes lookups (background, ≤ 1 request/s, names only). `update_settings` refuses `music_db` |
| `list_explore_views` / `save_explore_view` / `delete_explore_view` | — / `name, query: ExploreQuery` / `id` | `ExploreView[]` / `ExploreView` / `void` | Explore's "Save this view" (M10 WI 8), stored in the library DB like saved filters (FR-013): local only, in backups. `save_explore_view` validates `query` like `explore` and the name like `save_filter`; a view holds the builder only, never a result or a fingerprint score |

`get_listening_insights` (above) returns `Covered<ListeningInsights>` from M10: its `coverage` is computed for `range` vs the period before, and `PeriodStats` gains `plays_cmp`/`ms_cmp` so Home's strip and the Insights stat tiles compare like for like (G-46).

**Coverage rule (FR-048, design §4.1/§4.4).** One Rust function decides every comparison:
- `compared_sources` = the sources that cover the current **and** the comparison period (the export each date from its first to its last; the desktop log when it ran by the period's first date and on at least one of its dates, the dates it was off being `gaps`): `['S','E']` or `['E']` while the period ends on or before the last export's end; `['S']` once it reaches past it (`desktop_only_since` = the day after the export's last date, which drives the banner "From 3 Oct, only plays on this PC are counted"); `[]` when nothing is compared.
- `no_compare_reason` is set (and every `Compared.now`/`before`, `before_ms`, `move`, `plays_prev`/`ms_prev` is null or unused) when no source covers both periods: the desktop log wasn't running in the earlier period, there is no listening before it, or the grain is `all`.
- `gaps` = "no data" stretches (design §4.4) touching the periods shown: hatched in charts, skipped by streaks and Regularity, never silence.
- Totals that aren't comparisons (`Compared.value`, timeline `ms`) count every source there is, except for a period that reaches past the last export: while `coverage.desktop_only_since` is set, `tiles.days_listened` (its `value` too), `tiles.streak`, `fingerprint.now` and the `streak` sentence count source S for the whole period, so both halves count the same way, and the UI labels them "on this PC" (the sentence says it itself; design §4.4). Not for grain `all`: all-time figures use all-device dates only (design §4.4: the export's first to last date), so there `tiles.days_listened.value`, `tiles.streak` (the run at the export's last date), `fingerprint.now` and the `streak` sentence count every source on those dates and drop the this-PC dates past it; `desktop_only_since` is still set for the banner.

**Estimates** (labelled in the UI as estimates, design §4.2): `eras`, `arcs` (and their `related`), sessions and their start times, `deep_dives`, stick rate, `start_to_finish`, `next_milestone.eta_days`. Fingerprint scores are on screen only: never exported, copied or put in Explore's CSV.

```ts
type Grain = 'week' | 'month' | 'year' | 'all' | 'custom';
type Compare = 'prev' | 'last_year' | 'usual';          // usual = median of the last 12 periods of that length
type CoverageClass = 'all_devices' | 'this_pc' | 'no_data';
type DeviceClass = 'phone' | 'computer' | 'web' | 'this_pc' | 'other';
interface PeriodQuery {
  grain: Grain; vs: Compare;
  at: string | null;   // local date prefix inside the period: 'YYYY' (year), 'YYYY-MM' (month), 'YYYY-MM-DD' (week: any day; Monday-based); null = the current one
  from?: string; to?: string;   // custom only: local dates 'YYYY-MM-DD', inclusive
}
interface CoverageGap { from: string; to: string }      // local dates, inclusive
interface Coverage { compared_sources: Source[]; desktop_only_since: string | null; gaps: CoverageGap[]; no_compare_reason: string | null }
interface Covered<T> extends Sourced<T> { coverage: Coverage }
interface Compared { value: number; now: number | null; before: number | null }   // value = everything in the period; now/before = like for like over coverage.compared_sources (null = hidden)
interface Fingerprint { regularity: number; discovery: number; focus: number; variety: number; repeat: number }   // 0–100 each
interface Phrase { text: string; em: boolean }           // em = drawn bold (numbers, names)
interface Sentence { rule: 'time_change' | 'new_in_top5' | 'left_top10' | 'busiest_hour' | 'streak' | 'record'; parts: Phrase[] }   // design §5.1, ≤ 4, in rule order
interface TopRow extends RankRow { move: number | null; is_new: boolean; spark: number[] }   // move: places up (+) / down (−) vs the comparison period; is_new = not in its top 50; spark = hours per month, last 12

interface PeriodInfo {
  grain: Grain; vs: Compare; from: string; to: string;   // local dates, inclusive (to may be after today)
  vs_from: string | null; vs_to: string | null;          // null for 'usual' and 'all'
  prev_at: string | null; next_at: string | null;        // stepper `at` values; null before the first play / after the current period / custom
  days: number;                                          // covered days (no-data days left out): the "28 / 30" denominator
}
interface PeriodReport {
  period: PeriodInfo; private_excluded: number;
  tiles: { ms: Compared; plays: Compared; artists: Compared; new_artists: Compared; days_listened: Compared; streak: number; finished_pct: Compared | null };   // finished: export plays only, null when the period has none
  timeline: { key: string; ms: number; before_ms: number | null; coverage: CoverageClass }[];   // key = local date up to today (week, month, custom ≤ 92 days), else 'YYYY-MM'; before_ms = the aligned day/month of the comparison (mean of the 12 for 'usual'); while a comparison is shown, ms and before_ms are both like for like over compared_sources
  lines: Sentence[];
  clock: { hours: number[]; baseline: number[]; weekdays: number[] };   // % of time per hour 0–23 (baseline = last 6 months) and per weekday Mon–Sun
  top_new: { name: string; item_id: number | null; status: Status | null; first_played_local: string; first_track: string; ms: number; plays: number; rank: number; share_pct: number } | null;
  also_new: string[]; also_new_more: number;            // up to 3 more new artists, then how many others
  ratio: { tracks: Compared; albums: Compared; artists: Compared; tracks_per_artist: Compared; albums_per_artist: Compared };
  fingerprint: { now: Fingerprint; average: Fingerprint | null };       // average = your own 12-month average
  tags: { rows: { tag: string; share_pct: number }[]; covered_pct: number };   // top 6 tags by tagged listening time; covered = tagged ÷ total time
  most_played: { artists: TopRow[]; albums: TopRow[]; tracks: TopRow[] };      // 10 each
}

interface Arc {
  artist: string; item_id: number | null; kind: 'rising' | 'fading' | 'back_again' | 'constant';
  shares: number[];             // % of each month's time, 36 months ending this month
  change_pct: number | null;    // share change over the arc's window (fading −74, rising +210)
  since: string | null;         // 'YYYY-MM' the arc began (rising: first month up; back again: back since)
  quiet_from: string | null;    // back again: when the quiet stretch began
  related: string[];            // rising: may have taken time from; fading: may have given time to
}
interface Trends {
  months: { month: string; value: number; coverage: CoverageClass }[];   // every month of the history; class of most of its days
  eras: { from: string; to: string; artists: string[] }[];               // 'YYYY-MM', named after 1–2 artists
  arcs: Arc[];
  fingerprint_by_year: { year: number; fingerprint: Fingerprint }[];
  tag_months: { months: string[]; rows: { tag: string; shares: number[] }[] };   // last 12 months, % of tagged listening
  calendar: { year: number; minutes: (number | null)[]; biggest_day: { date: string; ms: number } | null; longest_streak: { days: number; from: string; to: string } | null; days_without_music: number }[];   // minutes[0] = 1 Jan; null = not an all-device date
}

interface DiscoveryRow { artist: string; item_id: number | null; first_played_local: string; first_track: string; plays_since: number; now: 'in_rotation' | 'dropped' | 'back_again' }
interface Discovery {
  new_artists: Compared; days_per_new: number | null;
  stick_rate: { pct: number | null; best_pct: number | null; best_year: number | null };   // pct null until the window has passed
  new_time_pct: number; artists_known: number; known_since: string | null;
  per_month: { month: string; stuck: number; dropped: number; open: number; coverage: CoverageClass }[];   // months of the period (≥ the last 24); open = window not over yet; coverage = class of most of its days, no_data months are 0 and hatched
  starts: { kind: 'picked' | 'followed' | 'back' | 'other'; pct: number }[];      // export only; [] when none
  log: DiscoveryRow[];                                                            // the period's new artists, newest first, ≤ 100; `now` per design §4.2
}

interface Habits {
  export_plays: number;                                              // the banner's "this tab covers your export plays"
  ends: { kind: 'finished' | 'skipped' | 'back' | 'stopped'; pct: number }[];   // reason_end trackdone / fwdbtn / backbtn / the rest
  skip_rate: number; skips_by_hour: number[];                        // % (G-45 denominator); 24 values
  most_skipped: { name: string; item_id: number | null; skip_pct: number } | null;   // the most skipped of the period's top 20 artists
  quick_bails: { track: string; artist: string; item_id: number | null; times: number }[];   // ≥ 3 times, ≤ 10
  shuffle: { pct: number | null; by_year: { year: number; pct: number }[] };
  devices: { kind: DeviceClass; pct: number }[]; offline: { pct: number; peak_month: string | null };
  finished: { pct: number; how_far_pct: number | null; how_far_coverage_pct: number };   // how far = mean ms_played ÷ duration where a duration is known
  sessions: { count: number; typical_ms: number; longest: { ms: number; start_local: string } | null; spread: number[] };   // spread: counts per length bucket, edges 5, 15, 30, 45, 60, 90, 120, 180, 240 min (10 buckets)
  deep_dives: { kind: 'artist' | 'album' | 'repeat'; name: string; by: string; start_local: string; end_local: string; tracks: number; ms: number; times: number | null }[];   // times: album passes, or plays of the track that day (on repeat); ≤ 10
  start_to_finish: { item_id: number; name: string; by: string; times: number }[];   // library albums with a track list only
}

interface Records {
  since: string | null; plays_total: number;
  first_play: { track: string; artist: string; at_local: string } | null;
  top_track: RankRow | null; top_artist: RankRow | null; top_album: RankRow | null;
  longest_streak: { days: number; from: string; to: string } | null;
  biggest_day: { date: string; ms: number } | null;
  biggest_year: { year: number; ms: number } | null;
  most_artists_month: { month: string; artists: number } | null;
  most_loyal: { name: string; item_id: number | null; years_in_top20: number; years: number } | null;
  next_milestone: { plays: number; eta_days: number | null };       // next multiple of 5,000; ETA at the last-90-day pace
  on_this_day: { year: number; plays: number; ms: number; top: { kind: 'artist' | 'album' | 'track'; name: string; by: string } | null; first_artists: string[] }[];   // today's date in earlier years, newest first
  forgotten: { name: string; item_id: number | null; top10_year: number; last_played_local: string; play_uri: string | null }[];   // play_uri: their most played track (export URI) for "Play again"
}

interface ExploreQuery {
  series: { kind: 'artist' | 'album' | 'track'; name: string; by?: string }[];   // 1–3; names matched case-insensitively
  from: string; to: string;                      // local dates 'YYYY-MM-DD', inclusive
  grain: 'week' | 'month' | 'year'; measure: 'hours' | 'share' | 'plays';
  hours?: number[]; weekdays?: number[];         // 0–23; 1 = Monday … 7 = Sunday
  devices?: DeviceClass[]; shuffle?: boolean;    // export only: desktop plays are left out when either is set
}
interface ExploreResult {
  buckets: string[];                             // week: its Monday 'YYYY-MM-DD'; month 'YYYY-MM'; year 'YYYY'
  series: { kind: 'artist' | 'album' | 'track'; name: string; by: string; values: number[]; total: number; peak: { bucket: string; value: number } | null; first_played_local: string | null }[];
  export_only: boolean;
}

interface ExploreView { id: number; name: string; query: ExploreQuery }
interface DataCoverage {
  export: { from: string; to: string; plays: number } | null;
  desktop: { from: string; to: string; plays: number; matched: number } | null;   // matched = desktop plays an export twin covers (counted once)
  logger_runs_since: string | null;              // logger_run recording began; earlier desktop coverage is inferred from plays
  imports: { id: number; imported_at: string; from: string | null; to: string | null; added: number; already_had: number }[];
  private_excluded: number;
}

interface GenresDecades {
  lookups: { on: boolean; contact_set: boolean; pending: number; last_lookup: string | null };   // contact_set false = no project contact yet (U-12): nothing is looked up
  attribution: string;                                   // "Genres from MusicBrainz, CC BY-NC-SA 3.0": shown wherever genres show
  genre_count: number;                                   // distinct genres of the period's artists
  top_genres: { name: string; share_pct: number }[];     // ≤ 10; an artist's genre = its most voted; % of the period's time
  genre_months: { months: string[]; rows: { genre: string; shares: number[] }[] };   // last 24 months to today, top 6 genres, % of each month's time
  decades: { decade: number; share_pct: number; top_album: { name: string; by: string } | null }[];   // by album first release year; % of the period's time
  unknown_year_pct: number;                              // time with no album year (unmatched, no album, not looked up yet)
  average_age: { years: number; year: number } | null;   // period's last year − time-weighted mean release year
  released_this_year_pct: number | null;                 // time on albums first released in the period's last year ÷ time with a year
  countries: { name: string; share_pct: number }[];      // ISO 3166-1 alpha-2 codes, ≤ 10; % of the time whose artist has a country
  coverage: { artists_genre_pct: number; albums_year_pct: number; artists_country_pct: number; same_name_pct: number };   // % of the period's time; same_name = several artists share the name: labels are the top hit's, shown as "probably" (included in the other %)
}
```

`Settings` gains `export_reminder: boolean` (off by default): a local reminder on app open, monthly from the last export's end, never network (design §5, Your data). It also gains `music_db: boolean` (on by default, U-11): open music database lookups, changed only by `set_music_db`.

## 5. R3 commands (backend M5 · switched in M7)
| Command | Args | Returns | Notes / spec |
|---|---|---|---|
| `get_spotify_status` | — | `{state: 'disconnected' \| 'connected' \| 'reconnect', display_name, renew_by}` | SC-13c |
| `connect_spotify` | `client_id?` | same | `client_id` (pasted in the Connect dialog) is validated and saved to `%LOCALAPPDATA%\…\spotify_client_id` first (`validation` if malformed). With no client ID from the env var or that file: `spotify_auth` with `detail: 'client_id'` (the dialog then asks for it). Opens browser, PKCE, loopback port (TSD §5.2); adds scope `user-follow-read` only while the Inbox's followed-artists option is on; then creates the Runout Inbox if none was ever recorded (TSD §5.3) |
| `disconnect_spotify` | — | `void` | Deletes tokens + `spotify_cache.db` at once |
| `list_spotify_playlists` | — | `{uri, name, tracks, owned, collaborative}[]` | Own/collab only |
| `load_studio_draft` | `playlist_uri` | `Draft` | `{playlist_uri, snapshot_id, tracks: {uri, name, artist, rating, tags, dupe_of}[]}`; ratings/tags only (D27). `validation` for a playlist over 10,000 entries or holding local files/episodes (TSD §5.2) |
| `preview_studio_apply` | `draft, keep: string[]` | `{removed, reordered, added}` | Diff shown before confirm. Multiset counts: `removed`/`added` = copies dropped/new; `reordered` = kept entries that must move (kept count − longest common subsequence), so cutting a copy of a duplicate is not a move (TSD §5.2) |
| `apply_studio_draft` | `draft, keep: string[]` | `{undo_id, expires_at}` | `conflict` if the playlist's tracks (URIs, order, copies) changed on Spotify since the draft was loaded (reload the draft); nothing is written. `draft.snapshot_id` is informational, never compared (Spotify's GET serves a stale one after a write, TSD §5.2). `validation` if the playlist now holds local files/episodes or is over 10,000 entries (TSD §5.2); nothing is written. `spotify_busy` (with `retry_after_s`) when this playlist's last apply/undo write has an unknown outcome less than 2 minutes ago: it may still land and cut this apply's result, so nothing is written and the old undo is kept (TSD §5.2). Any other failure may have partly applied the playlist (a write Spotify didn't refuse with a 4xx may have landed) and keeps the undo: `get_last_apply` returns it, so the UI reloads the draft and the undo record and offers Undo. A 429 Spotify can't wait out on a later 100-URI chunk returns `internal`, never `spotify_busy`: the first chunk already landed |
| `get_last_apply` | `playlist_uri` | `{undo_id, applied_at, expires_at} \| null` | 30-day undo snapshot (O4); `null` when there is none (none is kept when Spotify refused the apply's first write with a 4xx); one per playlist, so a newer apply replaces it |
| `undo_last_apply` | `playlist_uri` | `void` | `not_found` when there is no snapshot. `conflict` if the playlist's tracks changed on Spotify since Runout's last write to it (they no longer equal what that write left, or it now holds local files/episodes, or it was deleted or unfollowed: 404 on the read); the snapshot is then dropped (`get_last_apply` → `null`), since that undo can never go through. A failed write keeps it for a retry (the playlist may be partly restored; Undo again finishes it); a 429 on a later chunk returns `internal`, not `spotify_busy`. `spotify_busy` (with `retry_after_s`) when the playlist still holds the list from before Runout's last write and that write's outcome is unknown, less than 2 minutes after it: it may still land, so nothing is written and the snapshot is kept (TSD §5.2) |
| `create_playlist_from_filter` | `filter_id, name, public` | `{playlist_uri, added, skipped_no_uri}` | FR-039. An add that fails after the playlist was created keeps that error's code, says how many tracks landed ("at least N" unless Spotify refused the failed add with a 4xx: it may still have landed), and puts the new `playlist_uri` in `detail` (a retry would create a second playlist). A create call that fails without a 4xx (or answers without the new playlist) keeps its code and says the playlist may have been created: check Spotify before retrying |
| `create_playlist_from_items` | `item_ids, name, public` | same | FR-039 for Rediscover shelves (rule-built, not saved filters); albums add all their tracks |
| `autofill` | `uri` | `{fields: {artist?, album?, year?, display_name?}, track_list?: {uri, name, position}[]}` | From cache; nothing written until confirm (TSD §4.2) |
| `bulk_add_album_tracks` | `album_id, tracks: {uri, name}[]` | `ItemSummary[]` | Promoted + linked `on_album`; tracks already in the library are linked too but not returned (result = added) |
| `get_top_items` | `kind: 'tracks' \| 'artists', range: '4w' \| '6m' \| '1y'` | `SpotifyEntry[]` | Display only; `{uri, name, artist, in_library_id}` |
| `get_recent_plays_api` | — | `(SpotifyEntry & {played_at})[]` | Display only; never in insights |
| `library_contains` / `save_to_spotify_library` / `remove_from_spotify_library` | `uris: string[]` | `boolean[]` / `void` / `void` | ≤ 40 per call, batched in Rust. Track and album URIs only: an artist URI is `validation` before any request (follow needs scopes Runout doesn't request, TSD §5.2) |
| `find_cross_playlist_dupes` | `playlist_uri` | `CrossDupe[]` | G-24, BRD R3-1: `{uri, playlists: {uri, name}[]}`, one row per track of that playlist that the user's **other** own/collab playlists also hold (by URI), in that playlist's order; tracks found nowhere else are left out. Only on an explicit "Check other playlists" (one items read per playlist; 429 waits like every call, a long one is `spotify_busy`). A playlist deleted meanwhile is skipped. Reads playlists only, never listening data (D27) |
| `get_inbox` | — | `InboxStatus` | TSD §5.3. Settings only: no request, works disconnected. `{state: 'none' \| 'ready' \| 'missing', playlist_uri: string \| null, last_check: string \| null, imported_last: number, saved_albums: boolean, followed_artists: boolean, needs_reconnect: boolean}`; `missing` = the Inbox was deleted in Spotify (offer **Create Inbox**, never re-created silently); `needs_reconnect` = followed artists is on but the sign-in lacks `user-follow-read` (offer Reconnect: `connect_spotify` asks for it while the option is on) |
| `create_inbox` | — | `InboxStatus` | Creates the private “Runout Inbox” when state is `none` or `missing`; a `ready` one is returned as is. `connect_spotify` also creates it after a sign-in when none was ever recorded |
| `check_inbox` | — | `{imported, albums, skipped}` | **Check now**. `imported` = new “name needed” items (`added_via: 'inbox'`), `albums` = how many of those are albums (an album batch or a saved album), `skipped` = already in the library. Imported and skipped tracks leave the Inbox. `spotify_auth` when disconnected, `offline` offline, `not_found` when the Inbox is gone (deleted in Spotify: state is then `missing`, offer **Create Inbox**; never `{0,0,0}`, which means nothing new). An album batch's tracks are removed in one DELETE; a track already in the Inbox when its album was created by an earlier check (not this import) counts as `skipped` (a track added after that import, one with no `added_at`, or a single of an album batched in the same check is imported; a batch track re-added on its own is imported too). Also runs at start and every 15 min (skipped disconnected/offline) |
| `set_inbox_options` | `saved_albums: boolean, followed_artists: boolean` | `InboxStatus` | Both off by default. Saved albums: albums saved after the option was turned on / the last check. Followed artists: the first check after turning it on only records who you follow (imports nothing); later checks import artists followed since |

“Add to backlog” from SC-12 is `create_item` with `provenance: 'promoted'` (FR-041) — no separate command.

`connect_spotify` creates the Runout Inbox after a successful sign-in when none was ever recorded (TSD §5.3, BRD D33, FR-046).

## 6. Events (Rust → UI)
| Event | Payload | Release |
|---|---|---|
| `online-changed` | `boolean` | MVP |
| `capture-received` | `ItemSummary` | MVP |
| `capture-disabled` | `{reason: 'wrong_code' \| 'error'}` | MVP |
| `backup-finished` | `Backup` | MVP |
| `unresolved-resolved` | `{resolved: number}` | MVP |
| `logger-status` | `get_logger_status` shape | R2 |
| `play-logged` | `Play` | R2 |
| `spotify-status` | `get_spotify_status` shape | R3 |
| `inbox-imported` | `check_inbox` shape (`{imported, albums, skipped}`), sent only when `imported > 0` (background check or Check now) | R3 |
| `open-quick-add` | `null` | R2 · tray **Quick add** (G-35): the window is shown and focused, the UI opens Quick add |

## 7. Changing this contract
Add or change a command here first, then `api.ts` types, the mock, and the Rust command in one change. `check.py` (from M3) compares the command names registered in `commands.rs` with the names exported by `api.ts`.
