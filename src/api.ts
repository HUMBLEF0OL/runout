// The only bridge between the UI and the Rust core (docs/API.md). Every command exists here in camelCase
// with the same snake_case fields as the Rust serde structs. `VITE_MOCK` (default on in M2) routes calls to
// src/mocks/; M6 routes MVP calls to `invoke` while R2/R3 remain mocked until M7.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { cleanLine, cleanTags, cleanText, LIMITS, lineOrNull, textOrNull } from "./lib/sanitize";

/* ------------------------------------------------------------------ §1 conventions */
export type AppErrorCode =
  | "not_found" | "validation" | "duplicate"
  | "invalid_link" | "unsupported_link"
  | "offline" | "io" | "integrity" | "conflict"
  | "spotify_auth" | "spotify_forbidden" | "spotify_busy"
  | "import_unrecognized" | "internal";
export interface AppError { code: AppErrorCode; message: string; existing_id?: number; retry_after_s?: number; detail?: string }
export interface Page<T> { rows: T[]; total: number; offset: number }
export const isAppError = (e: unknown): e is AppError => typeof e === "object" && e !== null && "code" in e && "message" in e;
/** Anything a command rejects with, as an AppError (a non-AppError throw becomes `internal`). */
export const toAppError = (e: unknown): AppError => (isAppError(e) ? e : { code: "internal", message: "Something went wrong.", detail: String(e) });

/* ------------------------------------------------------------------ §2 shared types */
export type ItemType = "album" | "track" | "artist";
export type Status = "planned" | "in_progress" | "explored" | "not_for_me" | "abandoned";
export type Priority = "high" | "medium" | "low";
export type Source = "U" | "S" | "E";
export type Range = "7d" | "30d" | "12m" | "all";

export interface ItemSummary {
  id: number; type: ItemType; display_name: string | null;
  spotify_uri: string | null; status: Status; priority: Priority | null;
  rating: number | null; favourite: boolean; revisit: boolean; in_backlog: boolean;
  backlog_order: number | null; tags: string[]; artist_names: string[];
  album_name: string | null; // tracks: the album it is on (row subtitle "Artist, from Album")
  added_via: "pc" | "phone"; // "From: Phone" column (SC-03)
  recommended_by: string | null; // row subtitles "Album, from Meera" (SC-02, SC-03)
  notes: string | null; // in-progress artist cards show it (SC-02)
  added_at: string; modified_at: string;
}
export type LinkKind = "by_artist" | "on_album" | "version_of";
export interface ItemDetail extends ItemSummary {
  review: string | null; notes: string | null; recommended_by: string | null;
  explored_at: string | null; reviewed_at: string | null;
  track_list: { position: number; name: string; item_id: number | null; plays: number }[] | null; // albums with a known track list (TSD §4.9)
  provenance: Record<string, "user" | "promoted">;
  links: { kind: LinkKind; direction: "from" | "to"; item: ItemSummary }[];
  history: { from: Status | null; to: Status; at: string; note: string | null }[];
}
export interface NewItem {
  type: ItemType; display_name: string | null; spotify_uri: string | null;
  status?: Status; priority?: Priority; in_backlog?: boolean; tags?: string[];
  recommended_by?: string; provenance: "user" | "promoted";
  promoted_fields?: string[]; // these fields get provenance "promoted"; the rest get `provenance`
  capture_ms?: number;
  artist_name?: string; notes?: string; // artist_name: core finds or creates the artist item and links it by_artist
  album_id?: number; // tracks: core links the new track on_album to this album
}
export type ItemPatch = Partial<Pick<ItemDetail,
  "display_name" | "priority" | "rating" | "review" | "notes" | "favourite" | "revisit"
  | "in_backlog" | "recommended_by">> & { type?: ItemType; tags?: string[]; promoted_fields?: string[]; artist_name?: string | null };
export interface ItemFilter {
  type?: ItemType; status?: Status[]; tags?: string[]; min_rating?: number; revisit?: boolean;
  favourite?: boolean; in_backlog?: boolean; name_needed?: boolean; priority?: Priority[];
  sort?: "backlog_order" | "added_at" | "modified_at" | "rating" | "name"; desc?: boolean;
  offset?: number; limit?: number;
}
export interface Tag { id: number; name: string; description: string | null; item_count: number }
export interface SavedFilter { id: number; name: string; definition: ItemFilter }
export interface Freshness { logger_last_event: string | null; export_latest_event: string | null }
export interface Sourced<T> { data: T; sources: Source[]; freshness: Freshness }

/* ------------------------------------------------------------------ §3 MVP types */
export interface Counts { library: number; albums: number; tracks: number; artists: number; backlog: number; in_progress: number; revisit: number; name_needed: number }
export interface Unresolved { id: number; raw_link: string; received_at: string; source: "shortcut" | "paste"; state: string; error: string | null }
export interface Dashboard {
  counts: Counts; in_progress: ItemSummary[];
  name_needed: ItemSummary[]; unresolved: Unresolved[];
  // SC-02 stat tiles and "started 6 months ago"
  planned: number; explored_this_month: number; not_for_me_this_month: number; explored_no_review: number;
  started_at: Record<number, string>; // in-progress item id → when it entered in_progress
}
export interface ParsedLink { state: "ok" | "short" | "unsupported" | "invalid"; uri?: string; type?: ItemType; existing_id?: number }
export interface SearchHit { item: ItemSummary; snippet: string; field: string }
export interface Rediscover { revisit: ItemSummary[]; top_rated: ItemSummary[]; stale_backlog: ItemSummary[]; incomplete_reviews: ItemSummary[] }
export interface Settings { library_path: string; backup_dir: string; backup_weekly: boolean; size_warn_mb: number; artwork_default: boolean }
export interface Capture { on: boolean; address: string; port: number; code: string }
export interface Backup { path: string; created_at: string; size_bytes: number; item_count: number; size_warning: boolean }
export interface RestoreCheck { ok: boolean; schema_version: number; counts: Partial<Counts>; error?: string }
export interface SuccessMetrics {
  capture_median_s: number; days_opened_this_week: number; searches_this_week: number;
  rediscover_visits_this_week: number; moved_after_14d_pct: number; explored_rated_pct: number;
}

/* ------------------------------------------------------------------ §4 R2 types */
export interface NowPlaying { track_name: string; artist_name: string; album_name: string; spotify_uri: string | null; ms_played: number; duration_ms: number; item: ItemSummary | null }
export interface LoggerStatus { state: "listening" | "idle" | "paused" | "off" | "error"; now_playing: NowPlaying | null; last_event_at: string | null; paused_until: string | null; autostart: boolean; include_private: boolean }
export interface Play {
  id: number; ended_at_local: string; ms_played: number; counts_as_play: boolean; skipped: boolean;
  track_name: string; artist_name: string; album_name: string; source: "logger" | "export";
  spotify_uri: string | null; item: ItemSummary | null; album_rating: number | null; // item = the track record; album_rating = its album record's rating
}
export interface Unmatched { norm_key: string; track_name: string; artist_name: string; plays: number; detail: string; suggestion: string; event_id: number; candidates: ItemSummary[] } // candidates[0] = the suggestion
export interface ItemListening { plays: number; ms_total: number; last_played_local: string | null; by_month: { month: string; plays: number }[] }
export interface ImportPreview { music: number; podcasts_skipped: number; overlap: number; playlists: number; private: number; from: string; until: string; files: string[] }
export interface ImportResult { batch_id: number; added: number; duplicates: number; reconciled: number; playlists: number; took_ms: number }
export interface ImportRecord { id: number; kind: "account" | "extended"; imported_at: string; record_count: number; latest_event_utc: string | null }
export interface RankRow { name: string; by: string; plays: number; hours: number; item_id: number | null; rating: number | null; status: Status | null }
export interface PeriodStats { plays: number; plays_prev: number; ms: number; ms_prev: number; new_artists: number; rated_this_period: number; unrated_plays: number }
export interface ListeningInsights {
  top_artists: RankRow[]; top_albums: RankRow[]; top_tracks: RankRow[];
  monthly: { month: string; hours: number }[]; heatmap: number[][];
  skip_rate: number; phone_share: number; totals: PeriodStats & { private_excluded: number; artists_total: number };
}
export interface CollectionInsights {
  backlog_growth: { month: string; added: number; finished: number }[];
  completion: { status: Status; count: number }[];
  rating_spread: { rating: number; count: number }[];
  tag_trends: { tag: string; count: number }[];
  avg_days_to_explored: number | null; reviews: number; explored_without_review: number;
}
export interface Take { item: ItemSummary; line: string }
export interface TakeInsights {
  rated_vs_played: Take[]; never_played_backlog: Take[]; loved_but_forgotten: Take[];
  unfinished_albums: { item: ItemSummary; played: number; total: number }[];
  unfinished_unknown: number; // albums without a track list
}
export interface YearReview {
  year: number; plays: number; hours: number; explored: number; reviews: number; busiest_month: string;
  top_artist: RankRow; top_album: RankRow;
}
export interface ExportedPlaylist { id: number; name: string; tracks: number; dupes: number; low: number }
export interface PlaylistCheckup { tracks: { position: number; name: string; artist: string; dupe: boolean; rating: number | null; never_played: boolean; tags: string[] }[] }

/* ------------------------------------------------------------------ §5 R3 types */
export interface SpotifyStatus { state: "disconnected" | "connected" | "reconnect"; display_name: string | null; renew_by: string | null }
export interface SpotifyPlaylist { uri: string; name: string; tracks: number; owned: boolean; collaborative: boolean }
export interface DraftTrack { uri: string; name: string; artist: string; rating: number | null; tags: string[]; dupe_of: string | null }
export interface Draft { playlist_uri: string; snapshot_id: string; tracks: DraftTrack[] }
export interface StudioDiff { removed: number; reordered: number; added: number }
export interface ApplyRecord { undo_id: number; applied_at: string; expires_at: string }
export interface Autofill { fields: { artist?: string; album?: string; year?: number; display_name?: string }; track_list?: { uri: string; name: string; position: number }[] }
export interface SpotifyEntry { uri: string; name: string; artist: string; in_library_id: number | null }

/* ------------------------------------------------------------------ transport */
export const MOCK = import.meta.env.VITE_MOCK !== "0";
// Loaded on demand: main.tsx awaits it before the first render on mocks. A VITE_MOCK=0 build (.env.production)
// shows MVP screens only (useRelease), which all run on Rust, so it drops the mocks and their dataset entirely
// and an R2/R3 call there fails rather than show mock data. The env check sits beside the import, not via MOCK,
// so the bundler can prove the branch dead.
export const loadMocks = () => (import.meta.env.VITE_MOCK !== "0" ? import("./mocks") : Promise.reject(new Error("Not available until Release 2/3 runs on your data")));
/** API.md §3 commands switched to Rust in M6; R2/R3 stay mocked until M7. */
export const MVP_COMMANDS = new Set([
  "get_counts", "get_dashboard", "list_items", "get_item", "create_item", "update_item", "set_status",
  "delete_item", "link_items", "unlink_items", "reorder_backlog", "parse_link", "resolve_link", "prefill_title",
  "list_unresolved", "retry_unresolved", "discard_unresolved", "search", "list_tags", "rename_tag", "merge_tags",
  "delete_tag", "list_saved_filters", "save_filter", "delete_saved_filter", "get_rediscover", "get_online",
  "get_settings", "update_settings", "get_capture", "set_capture_enabled", "new_pairing_code", "export_library",
  "list_backups", "backup_now", "pick_backup", "check_backup", "restore_backup", "choose_backup_dir",
  "move_library", "delete_library", "record_page_visit", "get_success_metrics",
]);
const MVP_EVENTS = new Set(["online-changed", "capture-received", "capture-disabled", "backup-finished", "unresolved-resolved"]);
type Listener = () => void;
const listeners = new Set<Listener>();
type InvokeTransport = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
type ListenTransport = <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void>;
let invokeCommand: InvokeTransport = invoke;
let listenEvent: ListenTransport = listen;
/** Test seam for transport routing; production callers should use the typed wrappers below. */
export const __setTransportForTests = (transport: {
  invoke: InvokeTransport;
  listen: ListenTransport;
}) => {
  invokeCommand = transport.invoke;
  listenEvent = transport.listen;
};
/** Data may have changed (a write command ran, even one that failed, or the dev state switcher moved): readers refetch. */
export const onInvalidate = (fn: Listener) => { listeners.add(fn); return () => { listeners.delete(fn); }; };
export const invalidate = () => listeners.forEach((fn) => fn());
const READ = /^(get_|list_|search$|parse_link|preview_|check_backup|prefill_title|autofill|load_studio|library_contains|record_page_visit|pick_)/;

async function call<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  // A failed write may still have changed data (a Spotify write that timed out keeps an undo, TSD §5.2).
  try {
    return !MOCK && MVP_COMMANDS.has(cmd)
      ? await invokeCommand<T>(cmd, args)
      : await (await loadMocks()).mockCall<T>(cmd, args);
  } finally {
    if (!READ.test(cmd)) invalidate();
  }
}

export interface EventMap {
  "online-changed": boolean; "capture-received": ItemSummary; "capture-disabled": { reason: "wrong_code" | "error" };
  "backup-finished": Backup; "unresolved-resolved": { resolved: number }; "logger-status": LoggerStatus;
  "play-logged": Play; "spotify-status": SpotifyStatus;
}
/** Subscribe to a Rust → UI event (API.md §6). Returns an unsubscribe function. */
export function onEvent<K extends keyof EventMap>(name: K, cb: (payload: EventMap[K]) => void): () => void {
  const p = !MOCK && MVP_EVENTS.has(name)
    ? listenEvent<EventMap[K]>(name, (e) => cb(e.payload))
    : loadMocks().then((m) => m.mockListen(name, cb as (p: unknown) => void), () => () => {}); // no mocks: R2/R3 events never fire
  return () => { p.then((un) => un()); };
}

/** Events that mean library data changed without a UI write (a phone share, a resolved short link, a capture
 *  switched off, a scheduled backup): readers refetch, as after a write. The mocks invalidate themselves; Rust
 *  only emits, so without this an open Backlog missed a phone share until the next navigation (M6). */
export const invalidateOnDataEvents = () => {
  const offs = (["capture-received", "capture-disabled", "unresolved-resolved", "backup-finished"] as const).map((e) => onEvent(e, () => invalidate()));
  return () => offs.forEach((off) => off());
};

/* ------------------------------------------------------------------ §3 MVP commands */
export const getCounts = () => call<Counts>("get_counts");
export const getDashboard = () => call<Dashboard>("get_dashboard");
export const listItems = (filter: ItemFilter) => call<Page<ItemSummary>>("list_items", { filter });
export const getItem = (id: number) => call<ItemDetail>("get_item", { id });
/** Free text is sanitized here, once, for every caller (src/lib/sanitize.ts). */
const cleanNew = (i: NewItem): NewItem => ({
  ...i, display_name: lineOrNull(i.display_name, LIMITS.name), tags: i.tags && cleanTags(i.tags),
  recommended_by: i.recommended_by == null ? undefined : cleanLine(i.recommended_by, LIMITS.person) || undefined,
  artist_name: i.artist_name == null ? undefined : cleanLine(i.artist_name, LIMITS.name) || undefined,
  notes: i.notes == null ? undefined : cleanText(i.notes) || undefined,
});
const cleanPatch = (p: ItemPatch): ItemPatch => ({
  ...p,
  ...("display_name" in p && { display_name: lineOrNull(p.display_name, LIMITS.name) }),
  ...("review" in p && { review: textOrNull(p.review) }),
  ...("notes" in p && { notes: textOrNull(p.notes) }),
  ...("recommended_by" in p && { recommended_by: lineOrNull(p.recommended_by, LIMITS.person) }),
  ...("artist_name" in p && { artist_name: lineOrNull(p.artist_name, LIMITS.name) }),
  ...(p.tags && { tags: cleanTags(p.tags) }),
});
export const createItem = (input: NewItem) => call<ItemDetail>("create_item", { input: cleanNew(input) });
export const updateItem = (id: number, patch: ItemPatch) => call<ItemDetail>("update_item", { id, patch: cleanPatch(patch) });
export const setStatus = (id: number, status: Status, note?: string) => call<ItemDetail>("set_status", { id, status, note: note == null ? undefined : cleanLine(note, LIMITS.note) || undefined });
export const deleteItem = (id: number) => call<void>("delete_item", { id });
export const linkItems = (fromId: number, toId: number, kind: LinkKind) => call<void>("link_items", { fromId, toId, kind });
export const unlinkItems = (fromId: number, toId: number, kind: LinkKind) => call<void>("unlink_items", { fromId, toId, kind });
export const reorderBacklog = (ids: number[]) => call<void>("reorder_backlog", { ids });
export const parseLink = (raw: string) => call<ParsedLink>("parse_link", { raw: cleanLine(raw, LIMITS.link) });
export const resolveLink = (raw: string) => call<{ uri: string; type: ItemType; existing_id?: number }>("resolve_link", { raw: cleanLine(raw, LIMITS.link) });
export const prefillTitle = (uri: string) => call<{ title: string | null }>("prefill_title", { uri });
export const listUnresolved = () => call<Unresolved[]>("list_unresolved");
export const retryUnresolved = () => call<{ resolved: number }>("retry_unresolved");
export const discardUnresolved = (id: number) => call<void>("discard_unresolved", { id });
export const search = (q: string, limit?: number, type?: ItemType) => call<SearchHit[]>("search", { q: cleanLine(q, LIMITS.query), limit, type });
export const listTags = () => call<Tag[]>("list_tags");
export const renameTag = (id: number, name: string) => call<Tag>("rename_tag", { id, name: cleanTags([name])[0] ?? "" });
export const mergeTags = (fromId: number, intoId: number) => call<Tag>("merge_tags", { fromId, intoId });
export const deleteTag = (id: number) => call<void>("delete_tag", { id });
export const listSavedFilters = () => call<SavedFilter[]>("list_saved_filters");
export const saveFilter = (name: string, definition: ItemFilter) => call<SavedFilter>("save_filter", { name: cleanLine(name, LIMITS.filter), definition });
export const deleteSavedFilter = (id: number) => call<void>("delete_saved_filter", { id });
export const getRediscover = () => call<Rediscover>("get_rediscover");
export const getOnline = () => call<boolean>("get_online");
export const getSettings = () => call<Settings>("get_settings");
export const updateSettings = (patch: Partial<Settings>) => call<Settings>("update_settings", { patch });
export const getCapture = () => call<Capture>("get_capture");
export const setCaptureEnabled = (on: boolean) => call<Capture>("set_capture_enabled", { on });
export const newPairingCode = () => call<Capture>("new_pairing_code");
export const exportLibrary = () => call<{ path: string; count: number } | null>("export_library");
export const listBackups = () => call<Backup[]>("list_backups");
export const backupNow = () => call<Backup>("backup_now");
export const pickBackup = () => call<string | null>("pick_backup");
export const checkBackup = (path: string) => call<RestoreCheck>("check_backup", { path });
export const restoreBackup = (path: string) => call<RestoreCheck>("restore_backup", { path });
export const chooseBackupDir = () => call<string | null>("choose_backup_dir");
export const moveLibrary = () => call<string | null>("move_library");
export const deleteLibrary = (confirm: string) => call<void>("delete_library", { confirm });
export const recordPageVisit = (page: string) => call<void>("record_page_visit", { page });
export const getSuccessMetrics = () => call<SuccessMetrics>("get_success_metrics");

/* ------------------------------------------------------------------ §4 R2 commands */
export const getLoggerStatus = () => call<LoggerStatus>("get_logger_status");
export const setLogger = (patch: { on?: boolean; autostart?: boolean; include_private?: boolean }) => call<LoggerStatus>("set_logger", { patch });
export const pauseLogger = (minutes: number | null) => call<LoggerStatus>("pause_logger", { minutes });
export const resumeLogger = () => call<LoggerStatus>("resume_logger");
export const listPlays = (q: { tab: "all" | "unrated" | "missing"; range: Range; offset: number; limit: number }) => call<Page<Play>>("list_plays", { q });
export const listUnmatched = () => call<Unmatched[]>("list_unmatched");
export const linkPlay = (eventId: number, itemId: number) => call<void>("link_play", { eventId, itemId });
export const getItemListening = (id: number) => call<Sourced<ItemListening>>("get_item_listening", { id });
export const pickImportSource = () => call<string | null>("pick_import_source");
export const previewImport = (path: string) => call<ImportPreview>("preview_import", { path });
export const runImport = (path: string) => call<ImportResult>("run_import", { path });
export const listImports = () => call<ImportRecord[]>("list_imports");
export const getFreshness = () => call<Freshness>("get_freshness");
export const getListeningInsights = (range: Range) => call<Sourced<ListeningInsights>>("get_listening_insights", { range });
export const getCollectionInsights = (range: Range) => call<Sourced<CollectionInsights>>("get_collection_insights", { range });
export const getTakeInsights = () => call<Sourced<TakeInsights>>("get_take_insights");
export const getYearReview = (year: number) => call<Sourced<YearReview>>("get_year_review", { year });
export const listExportedPlaylists = () => call<ExportedPlaylist[]>("list_exported_playlists");
export const getPlaylistCheckup = (id: number) => call<PlaylistCheckup>("get_playlist_checkup", { id });

/* ------------------------------------------------------------------ §5 R3 commands */
export const getSpotifyStatus = () => call<SpotifyStatus>("get_spotify_status");
export const connectSpotify = () => call<SpotifyStatus>("connect_spotify");
export const disconnectSpotify = () => call<void>("disconnect_spotify");
export const listSpotifyPlaylists = () => call<SpotifyPlaylist[]>("list_spotify_playlists");
export const loadStudioDraft = (playlistUri: string) => call<Draft>("load_studio_draft", { playlistUri });
export const previewStudioApply = (draft: Draft, keep: string[]) => call<StudioDiff>("preview_studio_apply", { draft, keep });
export const applyStudioDraft = (draft: Draft, keep: string[]) => call<{ undo_id: number; expires_at: string }>("apply_studio_draft", { draft, keep });
export const getLastApply = (playlistUri: string) => call<ApplyRecord | null>("get_last_apply", { playlistUri });
export const undoLastApply = (playlistUri: string) => call<void>("undo_last_apply", { playlistUri });
export const createPlaylistFromFilter = (filterId: number, name: string, isPublic: boolean) =>
  call<{ playlist_uri: string; added: number; skipped_no_uri: number }>("create_playlist_from_filter", { filterId, name: cleanLine(name, LIMITS.playlist), public: isPublic });
/** Rediscover shelves are rule-built, not saved filters: the playlist gets exactly the listed records (SC-07b). */
export const createPlaylistFromItems = (itemIds: number[], name: string, isPublic: boolean) =>
  call<{ playlist_uri: string; added: number; skipped_no_uri: number }>("create_playlist_from_items", { itemIds, name: cleanLine(name, LIMITS.playlist), public: isPublic });
export const autofill = (uri: string) => call<Autofill>("autofill", { uri });
export const bulkAddAlbumTracks = (albumId: number, tracks: { uri: string; name: string }[]) => call<ItemSummary[]>("bulk_add_album_tracks", { albumId, tracks: tracks.map((t) => ({ uri: t.uri, name: cleanLine(t.name, LIMITS.name) })) });
export const getTopItems = (kind: "tracks" | "artists", range: "4w" | "6m" | "1y") => call<SpotifyEntry[]>("get_top_items", { kind, range });
export const getRecentPlaysApi = () => call<(SpotifyEntry & { played_at: string })[]>("get_recent_plays_api");
export const libraryContains = (uris: string[]) => call<boolean[]>("library_contains", { uris });
export const saveToSpotifyLibrary = (uris: string[]) => call<void>("save_to_spotify_library", { uris });
export const removeFromSpotifyLibrary = (uris: string[]) => call<void>("remove_from_spotify_library", { uris });
