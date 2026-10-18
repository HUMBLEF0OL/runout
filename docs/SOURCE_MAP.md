# Source Map — Runout

Where every capability lives in code, which spec governs it, and how it's tested. Layout source: [TSD §9](TSD.md#9-repository-layout).

**Rules** (enforced by `python scripts/check.py`):
- Every file under `src-tauri/src/`, `src-tauri/migrations/` and `src/` must sit under a path listed here. New module → add a row first.
- A row with status `built` must point to a path that exists.
- Status: `planned` → `built` when its first work item passes proof. Update in the same change.

## Rust core (`src-tauri/`)

| Path | Responsibility | Capabilities / FRs | Spec | Tests | Milestone | Status |
|---|---|---|---|---|---|---|
| `src-tauri/src/main.rs` | Binary entry; `--uninstall-cleanup` → `runout_lib::uninstall_cleanup()` and exit (no window), else `run()` | CAP-16 | TSD §2, §4.11 | manual (exit 0, no window, EBWebView gone) | M0/M3 | built |
| `src-tauri/src/lib.rs` | Module tree; Tauri builder: dialog plugin, `AppState`, `app_open` usage event, pre-restore cleanup, capture `Listener` (managed, resumes if left on), weekly backup scheduler, `online-changed` (30 s re-probe) → `retry_unresolved`; `uninstall_cleanup()`; later commands, tray, close-to-tray | CAP-17 | TSD §2 | manual (tray) | M0 | built |
| `src-tauri/src/commands.rs` | Tauri command surface; the only UI→core entry point | all | TSD §1 rule | via module tests | M3–M5 | planned |
| `src-tauri/src/db/` | `open(path)` (WAL, foreign_keys, busy_timeout), `open_in_memory()` for tests, `migrate()` over embedded `migrations/*.sql` (`PRAGMA user_version`) | CAP-10 | TSD §1, §3 | `db` unit tests: migrate fresh + rerun, CHECKs, FTS sync (tag/artist rename, delete) | M3 | built |
| `src-tauri/src/error.rs` | `AppError {code, message, existing_id?, retry_after_s?, detail?}` + `From` sqlite/io/json/reqwest/tauri | all | API.md §1, TSD §6 | serialization + sqlite mapping | M3 | built |
| `src-tauri/src/state.rs` | `AppState {db: Mutex<Connection>, paths, settings}` managed by Tauri | all | TSD §2 | manual (app start) | M3 | built |
| `src-tauri/src/settings.rs` | `Settings` in `settings.json` (app config dir; not in library.db since `library_path` moves), merge/load/save | CAP-15 | API.md §3, D15 | merge/load/save test | M3 | built |
| `src-tauri/src/text.rs` | Free-text cleaning at the command boundary, mirrors `src/lib/sanitize.ts` | all | TSD §7 | `text` unit test | M3 | built |
| `src-tauri/migrations/` | Numbered SQL: `0001_init` (MVP core + `item_fts` + sync triggers + `usage_event`), `0002_listening` (R2), `0003_spotify` (R3) | — | TSD §3 | migration test (fresh + upgrade) | M3/M4/M5 | built |
| `src-tauri/src/error.rs` | `AppError` with API.md §1 codes; serializes to the UI error shape | all | API.md §1 | `error` unit tests | M3 | planned |
| `src-tauri/src/text.rs` | Free-text validation and length caps at the command boundary (mirrors `src/lib/sanitize.ts`) | FR-005, FR-006 | TSD §7 | `text` unit tests | M3 | planned |
| `src-tauri/src/links.rs` | `parse` (TSD §4.1), `resolve_short` (via net), `prefill_title` (oEmbed, never stored), `unresolved_capture` queue: `resolve_or_queue`, `list/discard/retry_unresolved` | CAP-01, FR-002, FR-017 | TSD §4.1, §4.3 | 35-case table (incl. `src/lib/link.test.ts` rows), queue lifecycle | M3 | built |
| `src-tauri/src/net.rs` | One shared `reqwest` client; `check`/`send`/`get` with host allow-list (also every redirect hop, max 5); `online()` cached 30 s + `on_online_change` | FR-019 | TSD §4.3, §7 | allow-list, no-network refusal, change hook | M3 | built |
| `src-tauri/src/library/` | `ItemType`; items (`id_by_uri`, `insert_name_needed` built), links, tags, status history, backlog, filters, search, provenance, rediscover, dashboard (stubs until step 4/5) | CAP-01–12 | TSD §3, §4.2 | promotion + CRUD tests | M3 | planned |
| `src-tauri/src/capture.rs` | Phone capture: `Guard::screen` (private IP, 10/min, ≤ 2 KB, code, 5 wrong → off, link parse), `handle` (effects), `save` (name-needed item, `capture` usage event, ItemSummary JSON), `Listener` (tiny_http on `0.0.0.0:47821`, stop flag + `unblock`, `capture.json` on/code) | CAP-13, FR-018, NFR-18 | TSD §5.1 | each NFR-18 rejection, lockout emits `capture-disabled`, save/duplicate, real-socket smoke | M3 | built |
| `src-tauri/src/export.rs` | `export_library(conn, path)`: JSON `{format_version, exported_at, items (+tags, provenance), tags, links, history, saved_filters}`, no `usage_event`; format in `docs/EXPORT_FORMAT.md` | CAP-15, FR-020 | TSD §4.12, EXPORT_FORMAT.md | doc keys = real export keys; provenance + no usage | M3 | built |
| `src-tauri/src/usage.rs` | `record`, `record_page_visit` (validated), `get_success_metrics` (BRD §15; rolling 7 days) | FR-045 | TSD §4.12, BRD §15 | metrics query test, page validation | M3 | built |
| `src-tauri/src/backup.rs` | `backup_now` (`VACUUM INTO` → zip + manifest → keep 3 → size warning), `list_backups`, `check_backup`, `restore_backup` (swap under the `AppState` lock, `library.db.pre-restore`), `cleanup_pre_restore`, `due` + `spawn_scheduler` (start + daily, `backup-finished`) | CAP-15, FR-021 | TSD §4.10 | rotation ×4, corrupted zip → integrity + library untouched, restore oldest verifies counts | M3 | built |
| `src-tauri/src/cleanup.rs` | `delete_library(confirm = "DELETE")` (empties every table + VACUUM); `uninstall_cleanup` (spotify_cache.db, EBWebView, `Runout/spotify` credential via `CredDeleteW`; missing is fine) | CAP-16, FR-022, FR-024 | TSD §4.11, SC-13f | phrase + emptied tables; cleanup tolerates missing | M3 | built |
| `src-tauri/src/logger/` | SMTC watcher, accumulation, pause, 5 s poll | CAP-18, FR-026 | TSD §4.5 | accumulation unit tests | M4 | planned |
| `src-tauri/src/importer/` | Export parse (allow-listed fields), minimization, dedupe, reconcile, matching, playlists | CAP-19–22 | TSD §4.6–4.8 | fixtures; idempotent re-import; overlap | M4 | planned |
| `src-tauri/src/insights/` | SQL views; sources + freshness; no cache-DB access | CAP-23–27 | TSD §4.9 | 3 hand-verified insights | M4 | planned |
| `src-tauri/src/spotify/` | PKCE auth, keyring, cache DB, Playlist Studio, autofill, top/recent | CAP-28–34 | TSD §5.2 | test playlist round-trip | M5 | planned |
| `src-tauri/examples/` | Spikes: `smtc_spike.rs` (M4), `spotify_gate.rs` (M5 O1 gate); not shipped | CAP-18, 28 | M4/M5 plans | manual | M4/M5 | planned |
| `src-tauri/windows/` | NSIS hooks (`hooks.nsh`): pre-uninstall cleanup, checkbox relabel | CAP-16, FR-024 | TSD §4.11 | uninstall cases on fresh profile | M6 | planned |
| `src-tauri/tests/fixtures/` | Synthetic exports, link tables — never real values | — | TSD §8 | — | M3+ | planned |

## UI (`src/`)

| Path | Responsibility | Capabilities | Spec | Tests | Milestone | Status |
|---|---|---|---|---|---|---|
| `src/api.ts` | Typed `invoke()` wrappers — the only bridge to Rust; mock-backed until M6/M7 | all | TSD §1 rule | tsc | M2 | built |
| `src/mocks/` | Typed mock data behind `api.ts` (M2), loaded by dynamic import so a `VITE_MOCK=0` build drops it; link parsing lives in `src/lib/link.ts`; removed per release in M6/M7 | all | TSD §1, DESIGN | tsc | M2 | built |
| `src/styles/` | Tailwind v4 entry (`index.css`), `tokens.css` (`@theme`), `app.css` recipes, self-hosted fonts | — | DESIGN §3 | contrast check | M2 | built |
| `src/pages/` | Dashboard, Library, Backlog, Rediscovery, Insights, Playlists, Settings, Data | CAP-01–35 | DESIGN §6, BRD §8 journeys | Vitest for logic; manual journeys | M2 | built |
| `src/components/` | Shared UI: quick-add, embed (lazy, offline placeholder), pickers; phone-capture Shortcut and Spotify-connect dialogs (`SetupDialogs.tsx`, SC-13c/SC-13e) | CAP-01, 04 | DESIGN §5, TSD §4.3 | Vitest | M2 | built |
| `src/hooks/` | Shared hooks: keyboard shortcuts, online state, events | CAP-17 | DESIGN §5, API.md §6 | Vitest (key → action) | M2 | built |
| `src/assets/` | Self-hosted fonts, official Spotify logo file | CAP-04, FR-044 | DESIGN §3, §7 | — | M2 | built |
| `src/lib/` | Input sanitization (`sanitize.ts`), display formatting (dates, plurals, labels) shared by pages; Studio cut suggestions (`studio.ts`: duplicate or rated ≤ 2, your data only); backlog reorder, priority sort + quick-add step (`backlog.ts`); Spotify link parse (`link.ts`: TSD §4.1, used by the mocks' `parse_link` and the combined field's label) | all, CAP-29, CAP-01, 13 | DESIGN §5, D27, TSD §4.1 | Vitest via components; `studio.test.ts`, `backlog.test.ts`, `link.test.ts` | M2 | built |
| `src/main.tsx` | React entry | — | — | — | M0 | built |
| `src/App.tsx` | Routing: hash routes, each page lazy-loaded as its own chunk | all | design spec | manual | M0 | built |
| `src/vite-env.d.ts` | Vite type declarations | — | — | tsc | M0 | built |

## Where to change X

| I want to change… | Go to |
|---|---|
| Which links are accepted | `links.rs` + TSD §4.1 table |
| What counts as a play | TSD §4.4 (D25) → `listening_event.counts_as_play` in migration + `logger/` |
| Which hosts the app may contact | `net.rs` allow-list + TSD §7 + `ALLOWED_HOSTS` in `scripts/check.py` |
| What an export import keeps | `importer/` field allow-list + TSD §4.8 |
| Anything Spotify-provided being stored | Stop: promotion rule (TSD §4.2) and `compliance-review` skill first |
