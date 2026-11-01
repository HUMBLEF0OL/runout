// The only bridge between the UI and the Rust core (docs/API.md). Every command exists here in camelCase
// with the same snake_case fields as the Rust serde structs. With `VITE_MOCK=0` (.env.production) every call goes
// to `invoke` and every event to `listen`; otherwise (dev, Vitest) to src/mocks/.
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
export type Source = "U" | "S" | "E" | "M"; // M = open music database labels (get_genres_decades only)
export type Range = "7d" | "30d" | "12m" | "all";

export interface ItemSummary {
  id: number; type: ItemType; display_name: string | null;
  spotify_uri: string | null; status: Status; priority: Priority | null;
  rating: number | null; favourite: boolean; revisit: boolean; in_backlog: boolean;
  backlog_order: number | null; tags: string[]; artist_names: string[];
  album_name: string | null; // tracks: the album it is on (row subtitle "Artist, from Album")
  added_via: "pc" | "phone" | "inbox"; // "From: Phone" column (SC-03); inbox = Runout Inbox (TSD §5.3)
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
export interface Settings { library_path: string; backup_dir: string; backup_weekly: boolean; size_warn_mb: number; artwork_default: boolean; export_reminder: boolean; music_db: boolean }
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
/** `*_cmp` vs `*_prev`: like for like over `coverage.compared_sources` (FR-048); a delta is cmp - prev, none when `coverage.no_compare_reason`. */
export interface PeriodStats { plays: number; plays_prev: number; ms: number; ms_prev: number; plays_cmp: number; ms_cmp: number; new_artists: number; rated_this_period: number; unrated_plays: number }
export interface ListeningInsights {
  top_artists: RankRow[]; top_albums: RankRow[]; top_tracks: RankRow[];
  monthly: { month: string; hours: number; coverage: CoverageClass }[]; heatmap: number[][];
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

/* ------------------------------------------------------------------ §4.1 Insights v3 types (M10; definitions: design/M10-insights-v3.md §4.2) */
export type Grain = "week" | "month" | "year" | "all" | "custom";
export type Compare = "prev" | "last_year" | "usual";
export type CoverageClass = "all_devices" | "this_pc" | "no_data";
export type DeviceClass = "phone" | "computer" | "web" | "this_pc" | "other";
/** `at`: 'YYYY' | 'YYYY-MM' | 'YYYY-MM-DD' (week: any day) inside the period, null = current; custom uses from/to. */
export interface PeriodQuery { grain: Grain; vs: Compare; at: string | null; from?: string; to?: string }
export interface CoverageGap { from: string; to: string }
export interface Coverage { compared_sources: Source[]; desktop_only_since: string | null; gaps: CoverageGap[]; no_compare_reason: string | null }
export interface Covered<T> extends Sourced<T> { coverage: Coverage }
/** value = everything in the period; now/before = like for like over coverage.compared_sources (null = hidden). */
export interface Compared { value: number; now: number | null; before: number | null }
export interface Fingerprint { regularity: number; discovery: number; focus: number; variety: number; repeat: number }
export interface Phrase { text: string; em: boolean }
export interface Sentence { rule: "time_change" | "new_in_top5" | "left_top10" | "busiest_hour" | "streak" | "record"; parts: Phrase[] }
export interface TopRow extends RankRow { move: number | null; is_new: boolean; spark: number[] }
export interface PeriodInfo {
  grain: Grain; vs: Compare; from: string; to: string; vs_from: string | null; vs_to: string | null;
  prev_at: string | null; next_at: string | null; days: number;
}
export interface PeriodReport {
  period: PeriodInfo; private_excluded: number;
  tiles: { ms: Compared; plays: Compared; artists: Compared; new_artists: Compared; days_listened: Compared; streak: number; finished_pct: Compared | null };
  timeline: { key: string; ms: number; before_ms: number | null; coverage: CoverageClass }[];
  lines: Sentence[];
  clock: { hours: number[]; baseline: number[]; weekdays: number[] };
  top_new: { name: string; item_id: number | null; status: Status | null; first_played_local: string; first_track: string; ms: number; plays: number; rank: number; share_pct: number } | null;
  also_new: string[]; also_new_more: number;
  ratio: { tracks: Compared; albums: Compared; artists: Compared; tracks_per_artist: Compared; albums_per_artist: Compared };
  fingerprint: { now: Fingerprint; average: Fingerprint | null };
  tags: { rows: { tag: string; share_pct: number }[]; covered_pct: number };
  most_played: { artists: TopRow[]; albums: TopRow[]; tracks: TopRow[] };
}
export interface Arc {
  artist: string; item_id: number | null; kind: "rising" | "fading" | "back_again" | "constant";
  shares: number[]; change_pct: number | null; since: string | null; quiet_from: string | null; related: string[];
}
export interface Trends {
  months: { month: string; value: number; coverage: CoverageClass }[];
  eras: { from: string; to: string; artists: string[] }[];
  arcs: Arc[];
  fingerprint_by_year: { year: number; fingerprint: Fingerprint }[];
  tag_months: { months: string[]; rows: { tag: string; shares: number[] }[] };
  calendar: {
    year: number; minutes: (number | null)[]; // [0] = 1 Jan; null = not an all-device date
    biggest_day: { date: string; ms: number } | null; longest_streak: { days: number; from: string; to: string } | null; days_without_music: number;
  }[];
}
export interface DiscoveryRow { artist: string; item_id: number | null; first_played_local: string; first_track: string; plays_since: number; now: "in_rotation" | "dropped" | "back_again" }
export interface Discovery {
  new_artists: Compared; days_per_new: number | null;
  stick_rate: { pct: number | null; best_pct: number | null; best_year: number | null };
  new_time_pct: number; artists_known: number; known_since: string | null;
  per_month: { month: string; stuck: number; dropped: number; open: number; coverage: CoverageClass }[];
  starts: { kind: "picked" | "followed" | "back" | "other"; pct: number }[];
  log: DiscoveryRow[];
}
export interface Habits {
  export_plays: number;
  ends: { kind: "finished" | "skipped" | "back" | "stopped"; pct: number }[];
  skip_rate: number; skips_by_hour: number[];
  most_skipped: { name: string; item_id: number | null; skip_pct: number } | null;
  quick_bails: { track: string; artist: string; item_id: number | null; times: number }[];
  shuffle: { pct: number | null; by_year: { year: number; pct: number }[] };
  devices: { kind: DeviceClass; pct: number }[]; offline: { pct: number; peak_month: string | null };
  finished: { pct: number; how_far_pct: number | null; how_far_coverage_pct: number };
  sessions: { count: number; typical_ms: number; longest: { ms: number; start_local: string } | null; spread: number[] };
  deep_dives: { kind: "artist" | "album" | "repeat"; name: string; by: string; start_local: string; end_local: string; tracks: number; ms: number; times: number | null }[];
  start_to_finish: { item_id: number; name: string; by: string; times: number }[];
}
export interface Records {
  since: string | null; plays_total: number;
  first_play: { track: string; artist: string; at_local: string } | null;
  top_track: RankRow | null; top_artist: RankRow | null; top_album: RankRow | null;
  longest_streak: { days: number; from: string; to: string } | null;
  biggest_day: { date: string; ms: number } | null;
  biggest_year: { year: number; ms: number } | null;
  most_artists_month: { month: string; artists: number } | null;
  most_loyal: { name: string; item_id: number | null; years_in_top20: number; years: number } | null;
  next_milestone: { plays: number; eta_days: number | null };
  on_this_day: { year: number; plays: number; ms: number; top: { kind: "artist" | "album" | "track"; name: string; by: string } | null; first_artists: string[] }[];
  forgotten: { name: string; item_id: number | null; top10_year: number; last_played_local: string; play_uri: string | null }[];
}
export interface ExploreQuery {
  series: { kind: "artist" | "album" | "track"; name: string; by?: string }[];
  from: string; to: string; grain: "week" | "month" | "year"; measure: "hours" | "share" | "plays";
  hours?: number[]; weekdays?: number[]; devices?: DeviceClass[]; shuffle?: boolean;
}
export interface ExploreResult {
  buckets: string[];
  series: { kind: "artist" | "album" | "track"; name: string; by: string; values: number[]; total: number; peak: { bucket: string; value: number } | null; first_played_local: string | null }[];
  export_only: boolean;
}
export interface ExploreView { id: number; name: string; query: ExploreQuery }
export interface DataCoverage {
  export: { from: string; to: string; plays: number } | null;
  desktop: { from: string; to: string; plays: number; matched: number } | null;
  logger_runs_since: string | null;
  imports: { id: number; imported_at: string; from: string | null; to: string | null; added: number; already_had: number }[];
  private_excluded: number;
}
export interface GenresDecades {
  lookups: { on: boolean; contact_set: boolean; pending: number; last_lookup: string | null }; // contact_set false = U-12 open: nothing looked up
  attribution: string; // show wherever genres show (CC BY-NC-SA 3.0)
  genre_count: number;
  top_genres: { name: string; share_pct: number }[];
  genre_months: { months: string[]; rows: { genre: string; shares: number[] }[] };
  decades: { decade: number; share_pct: number; top_album: { name: string; by: string } | null }[];
  unknown_year_pct: number;
  average_age: { years: number; year: number } | null;
  released_this_year_pct: number | null;
  countries: { name: string; share_pct: number }[]; // ISO 3166-1 alpha-2
  coverage: { artists_genre_pct: number; albums_year_pct: number; artists_country_pct: number; same_name_pct: number };
}

/* ------------------------------------------------------------------ §5 R3 types */
export interface SpotifyStatus { state: "disconnected" | "connected" | "reconnect"; display_name: string | null; renew_by: string | null }
export interface SpotifyPlaylist { uri: string; name: string; tracks: number; owned: boolean; collaborative: boolean }
export interface DraftTrack { uri: string; name: string; artist: string; rating: number | null; tags: string[]; dupe_of: string | null }
export interface Draft { playlist_uri: string; snapshot_id: string; tracks: DraftTrack[] }
export interface StudioDiff { removed: number; reordered: number; added: number }
export interface ApplyRecord { undo_id: number; applied_at: string; expires_at: string }
export interface Autofill { fields: { artist?: string; album?: string; year?: number; display_name?: string }; track_list?: { uri: string; name: string; position: number }[] }
export interface SpotifyEntry { uri: string; name: string; artist: string; in_library_id: number | null }
/** A track of the loaded playlist that the user's other own/collab playlists also hold (G-24). */
export interface CrossDupe { uri: string; playlists: { uri: string; name: string }[] }
export interface InboxStatus {
  state: "none" | "ready" | "missing"; playlist_uri: string | null; last_check: string | null; imported_last: number;
  saved_albums: boolean; followed_artists: boolean; needs_reconnect: boolean;
}
export interface InboxCheck { imported: number; albums: number; skipped: number }

/* ------------------------------------------------------------------ transport */
export const MOCK = import.meta.env.VITE_MOCK !== "0";
// Loaded on demand: main.tsx awaits it before the first render on mocks. The env check sits beside the import, not
// via MOCK, so the bundler can prove the branch dead and a VITE_MOCK=0 build carries no mock code (check.py).
export const loadMocks = () => (import.meta.env.VITE_MOCK !== "0" ? import("./mocks") : Promise.reject(new Error("This build has no mock data")));
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
const READ = /^(get_|list_|search$|explore$|parse_link|preview_|check_backup|prefill_title|autofill|load_studio|library_contains|record_page_visit|pick_|find_cross_playlist_dupes$)/;

async function call<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  // A failed write may still have changed data (a Spotify write that timed out keeps an undo, TSD §5.2).
  try {
    return !MOCK
      ? await invokeCommand<T>(cmd, args)
      : await (await loadMocks()).mockCall<T>(cmd, args);
  } finally {
    if (!READ.test(cmd)) invalidate();
  }
}

export interface EventMap {
  "online-changed": boolean; "capture-received": ItemSummary; "capture-disabled": { reason: "wrong_code" | "error" };
  "backup-finished": Backup; "unresolved-resolved": { resolved: number }; "logger-status": LoggerStatus;
  "play-logged": Play; "spotify-status": SpotifyStatus; "inbox-imported": InboxCheck; "open-quick-add": null;
}
/** Subscribe to a Rust → UI event (API.md §6). Returns an unsubscribe function. */
export function onEvent<K extends keyof EventMap>(name: K, cb: (payload: EventMap[K]) => void): () => void {
  const p = !MOCK
    ? listenEvent<EventMap[K]>(name, (e) => cb(e.payload))
    : loadMocks().then((m) => m.mockListen(name, cb as (p: unknown) => void), () => () => {});
  return () => { p.then((un) => un()); };
}

/** Events that mean data changed without a UI write (a phone share, a resolved short link, a capture switched
 *  off, a scheduled backup, a logged play, the logger or Spotify connection changing state, an Inbox import): readers refetch, as
 *  after a write. The mocks invalidate themselves; Rust only emits, so without this an open Backlog missed a phone
 *  share until the next navigation (M6), and the rail's freshness and now-playing went stale (M7). */
export const invalidateOnDataEvents = () => {
  // One refetch per burst: a retry that resolves 20 queued links emits 21 events.
  let t: ReturnType<typeof setTimeout> | undefined;
  const soon = () => { clearTimeout(t); t = setTimeout(invalidate, 50); };
  const offs = (["capture-received", "capture-disabled", "unresolved-resolved", "backup-finished", "play-logged", "logger-status", "spotify-status", "inbox-imported"] as const).map((e) => onEvent(e, soon));
  return () => { clearTimeout(t); offs.forEach((off) => off()); };
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
/** `date` (local YYYY-MM-DD, M10 Trends calendar) keeps that day only and replaces `range`. */
export const listPlays = (q: { tab: "all" | "unrated" | "missing"; range: Range; offset: number; limit: number; date?: string }) => call<Page<Play>>("list_plays", { q });
export const listUnmatched = () => call<Unmatched[]>("list_unmatched");
export const linkPlay = (eventId: number, itemId: number) => call<void>("link_play", { eventId, itemId });
export const getItemListening = (id: number) => call<Sourced<ItemListening>>("get_item_listening", { id });
export const pickImportSource = () => call<string | null>("pick_import_source");
export const previewImport = (path: string) => call<ImportPreview>("preview_import", { path });
export const runImport = (path: string) => call<ImportResult>("run_import", { path });
export const listImports = () => call<ImportRecord[]>("list_imports");
export const getFreshness = () => call<Freshness>("get_freshness");
export const getListeningInsights = (range: Range) => call<Covered<ListeningInsights>>("get_listening_insights", { range });
export const getCollectionInsights = (range: Range) => call<Sourced<CollectionInsights>>("get_collection_insights", { range });
export const getTakeInsights = () => call<Sourced<TakeInsights>>("get_take_insights");
export const getYearReview = (year: number) => call<Sourced<YearReview>>("get_year_review", { year });
/* §4.1 Insights v3 (M10). Dates and names are free text: cleaned here like every other input. */
const cleanPeriod = (p: PeriodQuery): PeriodQuery => ({
  grain: p.grain, vs: p.vs, at: p.at == null ? null : cleanLine(p.at, 10),
  ...(p.from != null && { from: cleanLine(p.from, 10) }), ...(p.to != null && { to: cleanLine(p.to, 10) }),
});
export const getPeriodReport = (period: PeriodQuery) => call<Covered<PeriodReport>>("get_period_report", { period: cleanPeriod(period) });
export const getTrends = (measure: "hours" | "plays" | "artists") => call<Covered<Trends>>("get_trends", { measure });
export const getDiscovery = (period: PeriodQuery) => call<Covered<Discovery>>("get_discovery", { period: cleanPeriod(period) });
export const getListeningHabits = (period: PeriodQuery) => call<Covered<Habits>>("get_listening_habits", { period: cleanPeriod(period) });
export const getRecords = () => call<Covered<Records>>("get_records");
const cleanExplore = (query: ExploreQuery): ExploreQuery => ({
  ...query, from: cleanLine(query.from, 10), to: cleanLine(query.to, 10),
  series: query.series.map((x) => ({ kind: x.kind, name: cleanLine(x.name, LIMITS.name), ...(x.by != null && { by: cleanLine(x.by, LIMITS.name) }) })),
});
export const explore = (query: ExploreQuery) => call<Covered<ExploreResult>>("explore", { query: cleanExplore(query) });
export const listExploreViews = () => call<ExploreView[]>("list_explore_views");
export const saveExploreView = (name: string, query: ExploreQuery) => call<ExploreView>("save_explore_view", { name: cleanLine(name, LIMITS.filter), query: cleanExplore(query) });
export const deleteExploreView = (id: number) => call<void>("delete_explore_view", { id });
export const getDataCoverage = () => call<Covered<DataCoverage>>("get_data_coverage");
export const getGenresDecades = (period: PeriodQuery) => call<Covered<GenresDecades>>("get_genres_decades", { period: cleanPeriod(period) });
export const setMusicDb = (on: boolean) => call<Settings>("set_music_db", { on });
export const listExportedPlaylists = () => call<ExportedPlaylist[]>("list_exported_playlists");
export const getPlaylistCheckup = (id: number) => call<PlaylistCheckup>("get_playlist_checkup", { id });

/* ------------------------------------------------------------------ §5 R3 commands */
export const getSpotifyStatus = () => call<SpotifyStatus>("get_spotify_status");
/** `clientId`: pasted in the Connect dialog when Spotify isn't set up (`spotify_auth`, detail `client_id`); Rust saves it. */
export const connectSpotify = (clientId?: string) =>
  call<SpotifyStatus>("connect_spotify", clientId === undefined ? {} : { clientId: cleanLine(clientId, 64) });
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
export const findCrossPlaylistDupes = (playlistUri: string) => call<CrossDupe[]>("find_cross_playlist_dupes", { playlistUri });
export const getInbox = () => call<InboxStatus>("get_inbox");
export const createInbox = () => call<InboxStatus>("create_inbox");
export const checkInbox = () => call<InboxCheck>("check_inbox");
export const setInboxOptions = (savedAlbums: boolean, followedArtists: boolean) => call<InboxStatus>("set_inbox_options", { savedAlbums, followedArtists });
