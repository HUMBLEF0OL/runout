# Source Map — Runout

Where every capability lives in code, which spec governs it, and how it's tested. Layout source: [TSD §9](TSD.md#9-repository-layout).

**Rules** (enforced by `python scripts/check.py`):
- Every file under `src-tauri/src/`, `src-tauri/migrations/` and `src/` must sit under a path listed here. New module → add a row first.
- A row with status `built` must point to a path that exists.
- Status: `planned` → `built` when its first work item passes proof. Update in the same change.

## Rust core (`src-tauri/`)

| Path | Responsibility | Capabilities / FRs | Spec | Tests | Milestone | Status |
|---|---|---|---|---|---|---|
| `src-tauri/src/main.rs` | Binary entry; calls `runout_lib::run()`; later `--uninstall-cleanup` flag | CAP-16 | TSD §2, §4.11 | manual | M0 | built |
| `src-tauri/src/lib.rs` | Tauri builder: plugins, commands, tray, close-to-tray | CAP-17 | TSD §2 | manual (tray) | M0 | built |
| `src-tauri/src/commands.rs` | Tauri command surface; the only UI→core entry point | all | TSD §1 rule | via module tests | M3–M5 | planned |
| `src-tauri/src/db/` | rusqlite connection, WAL, migration runner (`PRAGMA user_version`), FTS5 | CAP-10 | TSD §1, §3 | `db` unit tests | M3 | planned |
| `src-tauri/migrations/` | Numbered SQL: `0001_init` (MVP), `0002_listening` (R2), `0003_spotify` (R3) | — | TSD §3 | migration test (fresh + upgrade) | M3/M4/M5 | planned |
| `src-tauri/src/links.rs` | Spotify link parser + short-link resolve | CAP-01, FR-002 | TSD §4.1 | ≥15 table cases | M3 | planned |
| `src-tauri/src/net.rs` | Allow-listed HTTP client; `online()` check | FR-019 | TSD §4.3, §7 | host allow-list test | M3 | planned |
| `src-tauri/src/library/` | Items, links, tags, status history, journal, backlog, provenance, promotion rule | CAP-01–12 | TSD §3, §4.2 | promotion + CRUD tests | M3 | planned |
| `src-tauri/src/capture.rs` | `tiny_http` LAN listener, pairing code, lockout | CAP-13, FR-018 | TSD §5.1 | wrong-code ×5, non-LAN reject | M3 | planned |
| `src-tauri/src/backup.rs` | `VACUUM INTO`, zip + manifest, rotation 3, restore verify | CAP-15, FR-021 | TSD §4.10 | rotation ×4, restore integrity | M3 | planned |
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
| `src/mocks/` | Typed mock data behind `api.ts` (M2); removed per release in M6/M7 | all | TSD §1, DESIGN | tsc | M2 | built |
| `src/styles/` | Tailwind v4 entry (`index.css`), `tokens.css` (`@theme`), `app.css` recipes, self-hosted fonts | — | DESIGN §3 | contrast check | M2 | built |
| `src/pages/` | Dashboard, Library, Backlog, Rediscovery, Insights, Playlists, Settings, Data | CAP-01–35 | DESIGN §6, BRD §8 journeys | Vitest for logic; manual journeys | M2 | built |
| `src/components/` | Shared UI: quick-add, embed (lazy, offline placeholder), pickers | CAP-01, 04 | DESIGN §5, TSD §4.3 | Vitest | M2 | built |
| `src/hooks/` | Shared hooks: keyboard shortcuts, online state, events | CAP-17 | DESIGN §5, API.md §6 | Vitest (key → action) | M2 | built |
| `src/assets/` | Self-hosted fonts, official Spotify logo file | CAP-04, FR-044 | DESIGN §3, §7 | — | M2 | built |
| `src/lib/` | Input sanitization (`sanitize.ts`), display formatting (dates, plurals, labels) shared by pages; Studio cut suggestions (`studio.ts`: duplicate or rated ≤ 2, your data only); backlog reorder + quick-add step (`backlog.ts`) | all, CAP-29, CAP-01, 13 | DESIGN §5, D27 | Vitest via components; `studio.test.ts`, `backlog.test.ts` | M2 | built |
| `src/main.tsx` | React entry | — | — | — | M0 | built |
| `src/App.tsx` | App shell: layout, navigation, routing between pages | all | design spec | manual | M0 | built |
| `src/vite-env.d.ts` | Vite type declarations | — | — | tsc | M0 | built |

## Where to change X

| I want to change… | Go to |
|---|---|
| Which links are accepted | `links.rs` + TSD §4.1 table |
| What counts as a play | TSD §4.4 (D25) → `listening_event.counts_as_play` in migration + `logger/` |
| Which hosts the app may contact | `net.rs` allow-list + TSD §7 + `ALLOWED_HOSTS` in `scripts/check.py` |
| What an export import keeps | `importer/` field allow-list + TSD §4.8 |
| Anything Spotify-provided being stored | Stop: promotion rule (TSD §4.2) and `compliance-review` skill first |
