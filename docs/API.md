# Command Contract — Runout

| | |
|---|---|
| Version | 1.0 |
| Date | 18 Oct 2026 |
| Implements | [TSD §1 rule](TSD.md#1-technology-decisions-resolves-brd-o3), [DESIGN §6](DESIGN.md#6-screen-inventory-m1-designs-all-of-these) |
| Used by | M2 (`src/mocks/` implements it) · M3–M5 (`src-tauri/src/commands.rs` implements it) · M6–M7 (`src/api.ts` switches from mocks to commands) |

The **only** bridge between the UI and the Rust core. Each command and type below exists twice, with the same names and fields: in TypeScript (`src/api.ts`) and in Rust (`commands.rs` + `serde` structs). A change here comes first, then both sides follow in the same change.

## 1. Conventions
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
type Source = 'U' | 'S' | 'E';                          // never 'A' in insights (DESIGN §7)
type Range = '7d' | '30d' | '12m' | 'all';

interface ItemSummary {
  id: number; type: ItemType; display_name: string | null;   // null = name needed
  spotify_uri: string | null; status: Status; priority: Priority | null;
  rating: number | null; favourite: boolean; revisit: boolean; in_backlog: boolean;
  backlog_order: number | null; tags: string[]; artist_names: string[];
  added_at: string; modified_at: string;
}
interface ItemDetail extends ItemSummary {
  review: string | null; notes: string | null; recommended_by: string | null;
  explored_at: string | null; reviewed_at: string | null;
  provenance: Record<string, 'user' | 'promoted'>;
  links: { kind: 'by_artist' | 'on_album' | 'version_of'; direction: 'from' | 'to'; item: ItemSummary }[];
  history: { from: Status | null; to: Status; at: string; note: string | null }[];
}
interface NewItem {
  type: ItemType; display_name: string | null; spotify_uri: string | null;
  status?: Status; priority?: Priority; in_backlog?: boolean; tags?: string[];
  recommended_by?: string; provenance: 'user' | 'promoted';   // promoted = confirmed from Spotify data (TSD §4.2)
  capture_ms?: number;                                        // dialog open → save (FR-045)
}
type ItemPatch = Partial<Pick<ItemDetail,
  'display_name' | 'priority' | 'rating' | 'review' | 'notes' | 'favourite' | 'revisit'
  | 'in_backlog' | 'recommended_by'>> & { tags?: string[]; promoted_fields?: string[] };
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
| `get_dashboard` | — | `{counts, in_progress: ItemSummary[], recent_activity: {item, from, to, at}[], name_needed: ItemSummary[], unresolved: Unresolved[]}` | SC-01/02, FR-015 |
| `list_items` | `filter: ItemFilter` | `Page<ItemSummary>` | SC-03, SC-04 |
| `get_item` | `id` | `ItemDetail` | SC-05 |
| `create_item` | `input: NewItem` | `ItemDetail` | `duplicate` error with `existing_id` (FR-004); `capture` usage event |
| `update_item` | `id, patch: ItemPatch` | `ItemDetail` | `promoted_fields` sets provenance `promoted` for those fields |
| `set_status` | `id, status, note?` | `ItemDetail` | Writes `status_history`; sets `explored_at` (FR-005/006) |
| `delete_item` | `id` | `void` | FR-022 |
| `link_items` / `unlink_items` | `from_id, to_id, kind` | `void` | FR-003/004 versions |
| `reorder_backlog` | `ids: number[]` | `void` | Whole visible order; FR-012 |
| `parse_link` | `raw: string` | `{state: 'ok' \| 'short' \| 'unsupported' \| 'invalid', uri?, type?, existing_id?}` | Pure; TSD §4.1. `short` = needs resolve |
| `resolve_link` | `raw: string` | `{uri, type, existing_id?}` | Short link → URI; `offline` error queues it in `unresolved_capture` |
| `prefill_title` | `uri` | `{title: string \| null}` | oEmbed; **response never stored** (TSD §4.3) |
| `list_unresolved` / `retry_unresolved` / `discard_unresolved` | — / — / `id` | `Unresolved[]` / `{resolved: number}` / `void` | `Unresolved = {id, raw_link, received_at, source, state, error}` |
| `search` | `q, limit?` | `{item: ItemSummary, snippet: string, field: string}[]` | FTS5, < 200 ms (NFR-04) |
| `list_tags` / `rename_tag` / `merge_tags` / `delete_tag` | — / `id, name` / `from_id, into_id` / `id` | `Tag[]` / `Tag` / `Tag` / `void` | SC-13b |
| `list_saved_filters` / `save_filter` / `delete_saved_filter` | — / `name, definition` / `id` | `SavedFilter[]` / `SavedFilter` / `void` | FR-013 |
| `get_rediscover` | — | `{revisit, top_rated, stale_backlog, incomplete_reviews: ItemSummary[]}` | SC-07, FR-013 |
| `get_online` | — | `boolean` | `net::online()`, cached 30 s (TSD §4.3) |
| `get_settings` / `update_settings` | — / `patch` | `Settings` | `{library_path, backup_dir, backup_weekly, size_warn_mb, artwork_default}` |
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
| `get_logger_status` | — | `{state: 'listening' \| 'idle' \| 'paused' \| 'off' \| 'error', now_playing: NowPlaying \| null, last_event_at, paused_until}` | Tray + SC-08 |
| `set_logger` | `patch: {on?, autostart?, include_private?}` | same as above | FR-026 |
| `pause_logger` / `resume_logger` | `minutes: number \| null` / — | same | null = until resumed |
| `list_plays` | `{tab: 'all' \| 'unrated' \| 'missing', range, offset, limit}` | `Page<Play>` | `Play = {id, ended_at_local, ms_played, counts_as_play, skipped, track_name, artist_name, album_name, source, item: ItemSummary \| null}` |
| `list_unmatched` / `link_play` | — / `event_id, item_id` | `Unmatched[]` / `void` | Grouped by `norm_key` with play count (FR-030) |
| `get_item_listening` | `id` | `Sourced<{plays, ms_total, last_played_local, by_month}>` | SC-05 listening panel |
| `pick_import_source` | — | `string \| null` | Folder or .zip |
| `preview_import` | `path` | `{music, podcasts_skipped, overlap, playlists, private, from, until, files: string[]}` | No writes (SC-10 step 2) |
| `run_import` | `path` | `{batch_id, added, duplicates, reconciled, playlists, took_ms}` | One transaction; `import_unrecognized` on bad file (TSD §6) |
| `list_imports` | — | `{id, kind, imported_at, record_count, latest_event_utc}[]` | Settings |
| `get_freshness` | — | `Freshness` | Rail freshness box |
| `get_listening_insights` | `range` | `Sourced<{top_artists, top_albums, top_tracks, monthly, heatmap, skip_rate, phone_share, totals}>` | FR-031; no source A |
| `get_collection_insights` | `range` | `Sourced<{backlog_growth, completion, rating_spread, tag_trends}>` | |
| `get_take_insights` | — | `Sourced<{rated_vs_played, never_played_backlog, loved_but_forgotten, unfinished_albums}>` | Unfinished: only albums with a track list (TSD §4.9) |
| `get_year_review` | `year` | `Sourced<{plays, hours, top_artist, top_album, explored, reviews, busiest_month}>` | FR-034 |
| `list_exported_playlists` / `get_playlist_checkup` | — / `id` | `{id, name, tracks, dupes, low}[]` / `{tracks: {position, name, artist, dupe, rating, never_played}[]}` | Read-only (FR-033) |

## 5. R3 commands (backend M5 · switched in M7)
| Command | Args | Returns | Notes / spec |
|---|---|---|---|
| `get_spotify_status` | — | `{state: 'disconnected' \| 'connected' \| 'reconnect', display_name, renew_by}` | SC-13c |
| `connect_spotify` | — | same | Opens browser, PKCE, loopback port (TSD §5.2) |
| `disconnect_spotify` | — | `void` | Deletes tokens + `spotify_cache.db` at once |
| `list_spotify_playlists` | — | `{uri, name, tracks, owned, collaborative}[]` | Own/collab only |
| `load_studio_draft` | `playlist_uri` | `Draft` | `{playlist_uri, snapshot_id, tracks: {uri, name, artist, rating, tags, dupe_of}[]}`; ratings/tags only (D27) |
| `preview_studio_apply` | `draft, keep: string[]` | `{removed, reordered, added}` | Diff shown before confirm |
| `apply_studio_draft` | `draft, keep: string[]` | `{undo_id, expires_at}` | `conflict` if `snapshot_id` changed |
| `get_last_apply` / `undo_last_apply` | `playlist_uri` | `{undo_id, applied_at, expires_at} \| null` / `void` | 30-day snapshot (O4) |
| `create_playlist_from_filter` | `filter_id, name, public` | `{playlist_uri, added, skipped_no_uri}` | FR-039 |
| `autofill` | `uri` | `{fields: {artist?, album?, year?, display_name?}, track_list?: {uri, name, position}[]}` | From cache; nothing written until confirm (TSD §4.2) |
| `bulk_add_album_tracks` | `album_id, tracks: {uri, name}[]` | `ItemSummary[]` | Promoted + linked `on_album` |
| `get_top_items` | `kind: 'tracks' \| 'artists', range: '4w' \| '6m' \| '1y'` | `SpotifyEntry[]` | Display only; `{uri, name, artist, in_library_id}` |
| `get_recent_plays_api` | — | `(SpotifyEntry & {played_at})[]` | Display only; never in insights |
| `library_contains` / `save_to_spotify_library` / `remove_from_spotify_library` | `uris: string[]` | `boolean[]` / `void` / `void` | ≤ 40 per call, batched in Rust |

“Add to backlog” from SC-12 is `create_item` with `provenance: 'promoted'` (FR-041) — no separate command.

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

## 7. Changing this contract
Add or change a command here first, then `api.ts` types, the mock, and the Rust command in one change. `check.py` (from M3) compares the command names registered in `commands.rs` with the names exported by `api.ts`.
