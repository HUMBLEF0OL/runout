# Technical Specification (TSD) — Runout

| | |
|---|---|
| Version | 1.0 |
| Date | 18 Oct 2026 |
| Implements | [BRD](../BRD.md) |
| Related | [Capabilities](CAPABILITIES.md) · [Milestones](MILESTONES.md) · [Command contract](API.md) · [User actions](USER_ACTIONS.md) |

Items marked **⚑ verify** are assumptions to confirm in the named milestone. Everything else is decided.

---

## 1. Technology decisions (resolves BRD O3)

| Concern | Decision | Why |
|---|---|---|
| App shell | **Tauri 2** (Rust core + system WebView2) | Already installed toolchain (Rust 1.98 + MSVC, Node 26, WebView2) → zero new installs; small binary; native tray; web UI hosts Spotify embeds naturally |
| UI | **React + TypeScript + Vite** | Most common, boring choice; good list/table/chart ecosystem |
| Styling | **Tailwind CSS v4** (`@tailwindcss/vite`) | User decision 18 Oct 2026. Design tokens live in `@theme` with Tailwind's default palette removed, so only DESIGN §3 tokens exist as utilities; multi-state recipes (buttons, cards, segmented, rating, skeleton) stay in `@layer components` |
| Charts (R2) | CSS bars, heatmap grid and rank rows (no chart library) | Every R2 chart is a bar, rank or heatmap; plain markup + tokens keeps them themable and adds no bundle weight. Recharts was planned in M2 and dropped unused |
| Database | **SQLite** via `rusqlite` (bundled), **FTS5** for search, WAL mode | Serverless, transactional (NFR-05), fast (NFR-04) |
| Migrations | Numbered SQL files run at startup, `PRAGMA user_version` | No migration framework needed |
| Windows media session | `windows` crate → `Windows.Media.Control` (SMTC) | Official OS API; proven on this PC (Spotify 1.305, 18 Oct 2026) |
| HTTP client | `reqwest` (Rust side only) | oEmbed, short-link resolution, Web API |
| Capture listener | `std::net::TcpListener` on a background thread (non-blocking accept, thread per request ≤ 4) | Needs per-socket read/write timeouts and a refuse-before-read on Content-Length (> 2 KB → 413, chunked → 411); tiny_http 0.12 had neither and drained huge declared bodies on drop (G-17, fixed 18 Oct 2026) |
| Secrets | Windows Credential Manager via the `windows` crate (`CredWriteW`/`CredReadW`/`CredDeleteW`, generic credential `Runout/spotify`; no `keyring` crate, M5) | NFR-08 |
| Autostart / tray | Tauri tray API + `tauri-plugin-autostart` | Native |
| Web view mode | Main window created with `incognito(true)` (WebView2 InPrivate); verified 18 Oct 2026, [KB](kb/windows-tauri.md#webview2) | Spotify embed content is never kept in a disk cache (BRD 5.3, Embeds Terms IV.2.c) |
| Bundle identifier | `app.runout.desktop` | Determines all app data folders and what the uninstaller deletes |
| Installer | Tauri NSIS bundle (per-user, unsigned) | Free; uninstaller hooks for FR-024 |
| Tests | Rust `cargo test` for core logic; Vitest for UI logic; fixture files | No e2e framework until needed |
| UI design | Direction D · Pulse; tokens, components and screens in [DESIGN.md](DESIGN.md) | Single source for look and behaviour (BRD D29) |
| UI data during M2 | `src/api.ts` typed wrappers over `src/mocks/`; swapped for Rust commands in M6/M7 | Design-first order (BRD D30) without page rewrites |

**Rule:** all data access and business logic live in Rust. The UI calls Rust through Tauri commands only; it never touches SQLite or the network directly (except embed iframes).

## 2. Architecture

```
┌──────────────────────── Tauri app (single process) ─────────────────────────┐
│  WebView2 UI (React)                                                         │
│   Dashboard · Library · Backlog · Rediscovery · Insights · Playlists ·       │
│   Settings · Data      ── iframes: open.spotify.com/embed/* (CSP-allowed)    │
│            │ invoke()                                                        │
│  ──────────▼──────────────────────── Rust core ────────────────────────────  │
│   commands ─┬─ library     ─┬─ db (rusqlite, FTS5, migrations)               │
│             ├─ links        │                                                │
│             ├─ capture ◄────┼── TcpListener (off by default)                 │
│             ├─ logger  ◄────┼── SMTC watcher thread                          │
│             ├─ importer     │                                                │
│             ├─ insights     │                                                │
│             ├─ backup       │                                                │
│             └─ spotify (R3) ┴─ cache store (separate SQLite file)            │
│                    │ reqwest                                                 │
└────────────────────┼─────────────────────────────────────────────────────────┘
                     ▼
     open.spotify.com/oembed · spotify.link · api.spotify.com · accounts.spotify.com
```

- Closing the window hides to tray; the logger keeps running. Quit from tray stops everything.
- Two database files, so cache deletion is a file delete:
  - `library.db` — user data (sources U, S, E). Backed up.
  - `spotify_cache.db` — all Spotify-provided data (source A, R3): cache entries **and playlist snapshots**. Never backed up.

**Locations.** Paths come from Tauri’s path API (`app_data_dir`, `app_local_data_dir`), never hard-coded, so they always match what the uninstaller deletes.
| Data | Path |
|---|---|
| Library DB | `%APPDATA%\app.runout.desktop\library.db` (movable in Settings, D15) |
| Spotify cache DB | `%LOCALAPPDATA%\app.runout.desktop\spotify_cache.db` (fixed) |
| WebView2 data | `%LOCALAPPDATA%\app.runout.desktop\EBWebView\` (InPrivate mode keeps no browsing cache here) |
| Backups | `%USERPROFILE%\Documents\Runout\Backups\` (changeable; outside app data by design, BRD D28) |
| Tokens | Windows Credential Manager, target `Runout/spotify` |

## 3. Data model (`library.db`)

```sql
-- Core
item(id INTEGER PK, type TEXT CHECK(type IN('album','track','artist')),
     display_name TEXT,                  -- NULL = “name needed” (phone capture, BRD 5.2); UI shows the link instead
     spotify_uri TEXT UNIQUE,            -- NULL allowed; duplicates blocked with “open existing” (FR-004)
     status TEXT NOT NULL DEFAULT 'planned'
       CHECK(status IN('planned','in_progress','explored','not_for_me','abandoned')),
     priority TEXT CHECK(priority IN('high','medium','low')),
     rating REAL CHECK(rating BETWEEN 0.5 AND 5 AND rating*2 = CAST(rating*2 AS INT)),
     review TEXT, notes TEXT, favourite INT DEFAULT 0, revisit INT DEFAULT 0,
     in_backlog INT DEFAULT 1, recommended_by TEXT, backlog_order INT,
     added_via TEXT NOT NULL DEFAULT 'pc' CHECK(added_via IN('pc','phone')),   -- phone = capture listener (M3)
     added_at TEXT, explored_at TEXT, reviewed_at TEXT, created_at TEXT, modified_at TEXT)
field_provenance(item_id, field, source CHECK(source IN('user','promoted')), PRIMARY KEY(item_id, field))
item_link(from_id, to_id, kind CHECK(kind IN('by_artist','on_album','version_of')), PK(from_id,to_id,kind))
tag(id PK, name UNIQUE COLLATE NOCASE, description)
item_tag(item_id, tag_id, PK(item_id, tag_id))
status_history(id PK, item_id, from_status, to_status, at TEXT, note)
saved_filter(id PK, name, definition_json)
unresolved_capture(id PK, raw_link, received_at, source CHECK(source IN('paste','shortcut')), state, error)
item_fts  -- FTS5 over display_name, review, notes, recommended_by, tag names, linked artist names

-- Release 2
listening_event(id PK, source CHECK(source IN('logger','export')),
     ended_at_utc TEXT, ended_at_local TEXT, ms_played INT, counts_as_play INT GENERATED ALWAYS AS (ms_played >= 30000),
     track_name, artist_name, album_name, norm_key TEXT,  -- normalized "artist|track"
     spotify_track_uri, platform, reason_start, reason_end,
     shuffle INT, skipped INT, offline INT, private_session INT,
     item_id, reconciled_with INT, import_batch_id,
     dedupe_hash TEXT UNIQUE)                              -- idempotent import
CREATE UNIQUE INDEX one_to_one_reconcile ON listening_event(reconciled_with) WHERE reconciled_with IS NOT NULL;
-- Indexes (0003): ended_at_utc, norm_key, item_id, spotify_track_uri.
-- 0004 (NFR-04, M4 WI 9): listening_event(lower(artist_name), ended_at_utc) for "new artists in range".
-- 0005 (NFR-04, G-17): listening_rollup (per private flag × item × norm_key × names: events, plays, ms,
--   skips, exported, phone) + listening_heat (private × weekday × hour: ms), kept by triggers on every
--   listening_event insert/update/delete; reconciled rows excluded. Range `all` (insights, list_plays
--   count, list_unmatched) reads these via insights::rollup; bounded ranges read `base`.
-- Query rules from the 500k perf run: `base` CTE is NOT MATERIALIZED; list_plays pages walk the
-- time index; attribution joins records via links (insights SRC), not per event.
import_batch(id PK, kind CHECK(kind IN('streaming_history','account_data')), imported_at,
     record_count, latest_event_utc, checksum UNIQUE)
exported_playlist(id PK, import_batch_id, name, last_modified)   -- matched across exports by name (no ID in export)
exported_playlist_item(playlist_id, position, track_name, artist_name, album_name, spotify_track_uri)

-- Success metrics (FR-045), local only
usage_event(id PK, kind CHECK(kind IN('app_open','page_visit','capture')), at TEXT, detail TEXT, duration_ms INT)
```

`spotify_cache.db` (source A only; deleted as a file on disconnect):
```sql
cache_entry(uri PK, kind, payload_json, fetched_at, expires_at)
playlist_snapshot(id PK, playlist_uri, spotify_snapshot_id, taken_at, expires_at, items_json, expected_json, unsure_at)  -- spotify_snapshot_id: the draft's id, informational only; items_json: pre-apply URI list (what undo restores); expected_json: JSON array of the URI lists the playlist may hold after Runout's last write (one list, or before/after of a write with unknown outcome) — undo's conflict check (cache migration 4, 2026-10-20, dropped applied_snapshot_id and renamed pending_json); unsure_at: when that last write failed with an unknown outcome, NULL once a write is answered (cache migration 5, audit r3)
```

Timestamps are ISO-8601 UTC strings except `ended_at_local`.

## 4. Key rules and algorithms

### 4.1 Spotify link parsing (FR-002) — shared by M3 and M4
Accept, after trimming:
| Input | Handling |
|---|---|
| `https://open.spotify.com/{intl-xx/}?{album|track|artist|playlist}/{id}?si=…` | Strip locale prefix and query → `spotify:{type}:{id}` |
| `spotify:{type}:{id}` | As is |
| `https://spotify.link/{code}` | Online: follow redirects (HEAD/GET, max 5, 5 s timeout) to an open.spotify.com URL, then parse. Offline: store in `unresolved_capture`, retry on reconnect |
| Episode/show/audiobook links | Reject with message (D23) |
| Anything else | Reject; never fetched |

ID must match `^[A-Za-z0-9]{22}$`. Playlist links are accepted only by Playlist features, not as library items.

### 4.2 Promotion rule and provenance (BRD 5.2)
- Creating an item from a pasted/shared link or “Add to backlog” on Spotify-provided data stores `spotify_uri` + the user-confirmed `display_name`, with provenance `promoted`.
- Autofill (R3) values render from `spotify_cache.db` with a “confirm” affordance; only on confirm/edit are they written to `item` with provenance `promoted`/`user`.
- MVP title prefill: the add dialog may pre-fill the name from oEmbed `title`; saving = confirmation. This confirmed title is the only oEmbed-derived value ever stored.
- Phone capture (no one at the PC): stores `spotify_uri` only, `display_name = NULL`. The backlog shows a “name needed” badge; opening the item runs the normal confirm step.

### 4.3 Embeds (FR-017)
- Embed URL is derived: `https://open.spotify.com/embed/{type}/{id}` — no oEmbed call needed to render.
- oEmbed (`/oembed?url=`) is called only to pre-fill the title in the add dialog. The response is never persisted; only a title the user confirms is stored (4.2).
- Lists: artwork mode off by default; when on, page size 20, iframes `loading="lazy"`, only mounted when visible; page ends with an “Open Spotify” link.
- Attribution (FR-044): wherever Spotify-sourced content appears outside the embed itself, show the Spotify logo and an “Open Spotify” link per the Design Guidelines.
- CSP: `frame-src https://open.spotify.com; connect-src ipc: http://ipc.localhost; img-src 'self' data:` (UI never fetches Spotify directly).
- Offline detection: cross-origin iframes don’t report load failures reliably, so Rust exposes `net::online()` (HEAD to `open.spotify.com`, result cached 30 s, re-checked on OS network-change events). The UI mounts embed iframes only when online; otherwise it shows the placeholder.

### 4.4 Play rule (D25)
`counts_as_play = ms_played >= 30000`, for logger and export alike. Listening time = Σ `ms_played` over all events.

### 4.5 Logger (FR-026)
- Subscribe to `GlobalSystemMediaTransportControlsSessionManager`; track the session whose `SourceAppUserModelId` contains `Spotify` (configurable allow-list).
- On media-properties change, a replay of the same track (timeline position back under 3 s after being further in) or session end: close the current event; `ms_played` = accumulated time in `Playing` state (sum of intervals; pauses excluded; seeks don’t add time).
- Poll timeline every 5 s as a fallback (some apps don’t raise every event).
- Event is written only if `ms_played >= 1000` (noise filter); `counts_as_play` derives as above.
- Logger rows use `dedupe_hash = sha256("logger|" ts|uri-or-norm_key|ms_played)`, so they never collide with export rows (one writer for both: `importer/events.rs`).
- SMTC has no private-session flag, so desktop private sessions can’t be detected. Tray menu offers **Pause logging** (until resumed or for 1 h); paused time writes nothing.

### 4.6 Normalization and matching (FR-028, FR-030)
`norm(s) = lower(strip_diacritics(s))` with suffixes such as ` - Remastered 2011`, ` (feat. …)`, ` - Live` removed by a small regex list. `norm_key = norm(artist) + "|" + norm(track)`.

Artist strings differ by source: the export’s `master_metadata_album_artist_name` may be the album artist, while SMTC’s Artist may list several artists. So matching keys on the **track** name and uses the artist only to break ties.
- Event → item: by `spotify_track_uri` when present; else by `norm(track)` against track items (an event with an unknown uri only against items without a uri: one with a different uri is another recording), tie-broken by any overlap between the event’s artist string and the item’s linked artists. Still ambiguous → unmatched (user links manually from the “unmatched plays” list).

### 4.7 Reconciliation (FR-028)
On each export import, for every logger event with no export partner: candidates = export events with the same `norm(track)` and `|Δ ended_at_utc| ≤ 120 s` that are not yet linked to another logger event. Several candidates → prefer artist overlap, then the smallest time gap. Link by setting `reconciled_with` on the logger event. The unique partial index makes the link one-to-one. Insights exclude reconciled logger events.

Fixtures must include a featured-artist track, a compilation track and a track repeated back-to-back.

### 4.8 Import (FR-027, FR-029, FR-033)
- Input: the user selects the unzipped export folder (or the .zip). The files sit in the subfolder `Spotify Extended Streaming History/`, so match files by name at any depth (zip entry path or folder walk).
- Streaming history: files `Streaming_History_Audio_<YYYY>[_<n>].json` and `Streaming_History_Video_<YYYY>.json`. Each holds a UTF-8 top-level JSON array of flat objects, every key present (`null` when empty). Fields (verified 19 Oct 2026, [KB](kb/spotify.md#extended-streaming-history-format-t1--verified-2026-10-19)): `ts` (`YYYY-MM-DDTHH:MM:SSZ`, UTC, stream end), `platform, ms_played, conn_country, ip_addr, master_metadata_track_name, master_metadata_album_artist_name, master_metadata_album_album_name, spotify_track_uri, episode_name, episode_show_name, spotify_episode_uri, audiobook_title, audiobook_uri, audiobook_chapter_uri, audiobook_chapter_title, reason_start, reason_end, shuffle, skipped, offline, offline_timestamp, incognito_mode`. Video files also contain music-track rows, so import both kinds. Don't infer the year from the file name.
- Parse into an explicit allow-list (`ts, platform, ms_played, master_metadata_*, spotify_track_uri, reason_start, reason_end, shuffle, skipped, offline, incognito_mode`). Everything else, including `conn_country` and `offline_timestamp`, is discarded at parse time and never written. In particular, any key matching `ip_addr*`, `user_agent*` or `username` is dropped; older exports used `ip_addr_decrypted` / `user_agent_decrypted`, and the bundled ReadMe still shows them. Booleans may be null and store as NULL. Skip rows where `spotify_track_uri` is null (episodes, audiobooks, rows with no URI).
- `dedupe_hash = sha256(ts|spotify_track_uri|ms_played)`; insert-or-ignore → idempotent.
- `ended_at_local` = convert `ts` with Windows time-zone rules for the PC’s zone at that date.
- `private_session = incognito_mode` (null → false); excluded from insights unless the setting is on.
- Account data: only `Playlist*.json` **⚑ verify in M4** → `exported_playlist*`. `YourLibrary.json` is not imported (data minimization; nothing uses it). Playlists are matched across exports by name.
- Whole import runs in one transaction; raw files are not copied.

### 4.9 Insights (FR-031, FR-032)
- Pure SQL views over `item`, `status_history`, `listening_event` (excluding reconciled logger events and, by default, private sessions). No source-A table is reachable from insight queries (enforced: insights module has no access to the cache DB).
- Every insight response carries `sources: ['U'|'S'|'E']` and `freshness: {logger_last_event, export_latest_event}`.
- “Unfinished albums” is computed only for album items whose track list exists in the library (tracks linked `on_album`, entered or confirmed by the user). For those, unfinished = some linked tracks have no counted play. Other albums are not shown, and the UI says why.

### 4.10 Backup (FR-021, D16)
1. `VACUUM INTO` a temp file (consistent snapshot, compacted).
2. Zip (deflate) → `runout-backup-YYYYMMDD-HHMMSS.zip` with a `manifest.json` (app version, schema version, sha256 of db, counts). `library.db` holds no source-A data, so backups never contain Spotify-provided data.
3. Keep newest 3; delete older.
4. If zip size > threshold (default 100 MB) → warning notification; backup still kept.
- Restore: verify sha256 and `PRAGMA integrity_check` on the extracted DB, show counts, then swap files (old DB kept as `library.db.pre-restore` until next successful start; a second restore in the same session is refused, and a pre-restore left by an earlier session — kept because the restored library failed `integrity_check` — is renamed `library.db.pre-restore.<UTC stamp>`, never deleted; Delete library removes these too).

### 4.11 Uninstall (FR-024)
Tauri’s NSIS uninstaller has a “delete application data” checkbox. It is off by default, removes `%APPDATA%\<identifier>` and `%LOCALAPPDATA%\<identifier>`, and is skipped in update mode.
1. **Always**, via `NSIS_HOOK_PREUNINSTALL`, run `runout.exe --uninstall-cleanup`. This deletes:
   - `spotify_cache.db` (cache and playlist snapshots)
   - the Credential Manager entry `Runout/spotify`
   - the `EBWebView` folder

   The hook must skip this step when the uninstaller runs in update mode, so updates keep the Spotify connection. Tauri's NSIS template parses `/UPDATE` into `$UpdateMode` before invoking `NSIS_HOOK_PREUNINSTALL`.
2. **Library**: removed only if the user ticks the checkbox, relabelled “Also delete my music library”. This removes `%APPDATA%\app.runout.desktop\` (including `library.db.pre-restore`).
3. **Backups**: the default folder is `Documents\Runout\Backups`, outside both app-data folders, so neither step touches it.
4. **Library moved elsewhere (D15)**: not removed automatically. Settings warns about this when the user moves it.

### 4.12 Usage counters (FR-045)
Insert a `usage_event` on app open, on each page visit, and on each capture. A capture’s duration is measured from the add dialog opening (or the Shortcut request arriving) to save. Counters are used only by the success-metrics view, never leave the PC, and are excluded from exports.

### 4.13 Thresholds (decided 18 Oct 2026, one constant each)
| Rule | Value | Used by |
|---|---|---|
| Stale backlog item | status `planned`, in backlog, unchanged ≥ 60 days | Rediscover (FR-013) |
| Low-rated track | rating ≤ 2★ | Playlist check-up (FR-033), Playlist Studio cut suggestions |
| Never played | no event with `counts_as_play` | Playlist check-up, Your take |

## 5. Integrations

### 5.1 Capture listener (FR-018, NFR-18; resolves BRD O6 → local listener)
- Off by default. When enabled: binds `0.0.0.0:<port>` (default 47821), shows the PC’s LAN IP and a 6-digit pairing code.
- Accepts `POST /capture` with JSON `{ "code": "...", "link": "..." }`, body ≤ 2 KB; link must parse per 4.1 or be a spotify.link short link.
- Rejects requests from non-private source IPs (only 10/8, 172.16/12, 192.168/16, 169.254/16).
- Rate limit 10 requests/min; 5 wrong codes → listener disables itself and notifies.
- On/off and the code persist in `capture.json` (app config dir), so the Shortcut keeps working after a restart. Turning it back on after a lockout issues a new code. A link already in the library answers 200 and changes nothing.
- Captured items are stored per 4.2 (“name needed”).
- Accepted risk (NFR-18): plain HTTP on the home network, so the pairing code is not encrypted in transit.
- Windows Firewall will prompt on first enable (user action U-4).
- iOS Shortcut: *Receive URLs from Share Sheet → Get contents of URL (POST, JSON) → Show result.*

### 5.2 Spotify Web API (R3)
- Auth: Authorization Code + PKCE, no client secret.
- Redirect URI: registered as `http://127.0.0.1:8888/callback` (the dashboard rejects a port-less loopback URI). At login the app binds a free loopback port and sends `http://127.0.0.1:<port>/callback`; Spotify accepts any port on the loopback IP literal (T3 verified 19 Oct 2026, KB spotify.md); `localhost` is not allowed.
- Client ID: read at run time from the `RUNOUT_SPOTIFY_CLIENT_ID` environment variable (`spotify::CLIENT_ID_ENV`); never in the repo, never logged (check.py `check_no_committed_client_id`). Missing or malformed → `spotify_auth` with a message naming the variable.
- Tokens in Credential Manager. Refresh on 401, and **store the new refresh token** whenever the refresh response contains one. Refresh is single-flight: concurrent callers wait for one refresh and re-read the tokens, so an already-rotated refresh token is never sent; a refresh answer that arrives after a disconnect is dropped. On `invalid_grant` (for the refresh token still stored), discard tokens, delete `spotify_cache.db` (a revoked grant counts as a disconnect for the 5-day rule) and show “Reconnect Spotify” (6-month expiry); the state survives a restart via an empty `spotify_reconnect` marker file. A 429/5xx from the token endpoint is “Spotify busy”, not a sign-in error.
- Endpoints (names per Feb 2026 Dev Mode guide): `GET /me`, `GET /me/playlists`, `GET|POST|PUT|DELETE /playlists/{id}/items`, `POST /me/playlists`, `GET /me/top/{artists|tracks}`, `GET /me/player/recently-played`, `PUT|DELETE /me/library`, `GET /me/library/contains`, `GET /playlists/{id}` (`snapshot_id`, informational), `GET /albums/{id}`, `GET /albums/{id}/tracks` (create-playlist album expansion; ⚑ Dev Mode availability unverified, G-23), `GET /tracks/{id}`, `GET /artists/{id}`.
- Scopes: `playlist-read-private playlist-read-collaborative playlist-modify-private playlist-modify-public user-top-read user-read-recently-played user-library-read user-library-modify`. `PUT /me/library` takes up to 40 URIs per call.
- Rate limiting: honour `Retry-After` on 429; max 1 in-flight write.
- Playlist conflict check:
  - Spotify applies item changes even if the playlist changed since the given `snapshot_id`, so that ID gives no protection.
  - Nor can `snapshot_id` detect changes: verified live 2026-10-20 (user's test playlist, `examples/studio_roundtrip.rs`), the write answered a new `snapshot_id` but `GET /playlists/{id}?fields=snapshot_id` kept serving the pre-write id immediately, +3 s and ~30 s later. So Studio checks **track content**, never `snapshot_id` (the draft keeps it, informational only).
  - Immediately before applying, read the current items (`GET /playlists/{id}/items`, the same read that becomes the undo snapshot; one function, `current_uris`, so the read path can be swapped in one place) and compare the URI list with the draft's tracks, order and multiplicity included. Different → stop, nothing written, show “playlist changed on Spotify, reload draft”. A content-neutral version bump doesn't block; a stale id can't hide an edit. The items read is fresh right after a write (verified 20 Oct 2026: a reorder showed at once while `snapshot_id` reads lagged ~33–36 s; kb/spotify.md).
  - The remaining race window is the gap between the end of that read and the write (sub-second). Accepted.
  - An edit made in the Spotify app reaches the Web API a few seconds later (~9 s observed, 20 Oct 2026); until then no read can see it, so apply can't block it. Accepted (same class as the race window).
  - The whole apply (check, snapshot, every 100-URI chunk) and the whole undo hold one Studio lock, so two of them never interleave.
  - Undo checks too: every apply/undo write that lands records the URI list the playlist should now hold (the cumulative prefix for a multi-chunk write) on the undo snapshot (`expected_json`), so after a write fails partway undo, or a retried undo, compares against Runout's own last write. Undo reads the current items and restores only if they equal that list; otherwise (including a playlist that now holds local files/episodes or is over the ceiling) it returns `conflict` instead of overwriting the edit, and drops the snapshot. A failed write may still have landed (timeout, reset, 5xx, unreadable 2xx), even after an immediate re-read shows the old list (the server may still be processing it; the `snapshot_id` GET is verified stale), so only a 4xx refusal (incl. 429) counts as not landed; any other failure is unknown, and both the list before that write and the list it would leave are expected so undo can tell Runout's own unanswered write from someone else's edit (audit 2026-10-20). If undo finds the list from *before* such a write less than 2 minutes after it (`UNSURE_WAIT`; requests time out after 20 s), the write may still land and would cut the restored playlist with no undo left, so undo writes nothing, keeps the snapshot and returns `spotify_busy` with `retry_after_s`; after that the read is trusted (audit r3). Apply has the same guard: while the playlist's latest snapshot has a write with unknown outcome less than `UNSURE_WAIT` old, apply writes nothing, keeps that snapshot (its only undo) and returns `spotify_busy` with `retry_after_s` (audit r5). The preview diff counts multisets: `removed`/`added` are copies dropped/new, `reordered` = common − LCS(original, kept) (Hunt–Szymanski, fast for 10,000 entries), so cutting an earlier copy of a duplicate is not a move (live run 20 Oct 2026 showed 7 for a removal-only edit before this fix). A failed apply that keeps its snapshot may have partly changed the playlist: the UI reloads the draft and the undo record after its own apply/undo unless the error means nothing was written (`conflict`, `validation`, `spotify_busy`), and offers Undo. So `spotify_busy` is returned only when nothing was written: a 429 the client can't wait out on a later chunk (the PUT already landed) comes back as `internal` (audit r6). An undo whose read gets 404 (playlist deleted or unfollowed) is a `conflict` and drops the snapshot, like any undo that can never go through. Only an apply whose first write Spotify refused keeps no snapshot. One undo per playlist: once an apply (maybe) landed, older snapshots of that playlist are dropped. A playlist longer than 10,000 entries is refused (`validation`), never truncated. A playlist holding local files or podcast episodes is refused too (`validation`, at `load_studio_draft` and apply): they can't be put back by URI, so a replace would silently delete them.
- Playlist Studio reads only `item` ratings and tags (source U) alongside API playlist data. It never queries `listening_event` or `exported_playlist*` (BRD 11.1, D27).
- Disconnect: delete tokens and delete `spotify_cache.db` (cache and snapshots) immediately, which satisfies the 5-day rule. Disconnect first waits for a running Studio apply/undo (seconds), so a playlist is never left cut short with its undo deleted. The cache file is only ever opened while signed in, so a request still in flight at disconnect can't re-create it; a cache found without a sign-in is deleted by the sweeper. A Credential Manager delete that fails leaves an empty `spotify_disconnect_pending` marker (written before the delete, removed once it or a new sign-in's store succeeds): while it exists the stored tokens count as deleted and every start retries the delete, so a confirmed disconnect never signs back in after a restart. A Credential Manager read that fails (anything but not-found) is not "signed out": it is not cached, the next call reads again, the sweeper deletes nothing on it, and `get_spotify_status` returns `io` (no `spotify-status` event) instead of `disconnected`. A new sign-in bumps the token generation, so a refresh or `invalid_grant` answer from the older grant never overwrites or clears it. Display lists (top items, recently played) cache only the displayed fields, never the raw response.
- Cache expiry 7 days (O5); snapshot expiry 30 days (O4); sweeper runs at startup and daily.

## 6. Error handling and degradation (NFR-09, NFR-15)
| Situation | Behaviour |
|---|---|
| Offline | All U/S/E features work; embeds show placeholder; short links queued |
| oEmbed fails | Add dialog without title prefill |
| Spotify app not running | Logger idle; tray tooltip says so |
| SMTC unavailable | Logger shows error state; rest unaffected |
| API 401/403 | Token refresh / “not authorized (Premium or allow-list?)” message |
| `invalid_grant` | Reconnect prompt |
| 429 | Back off per `Retry-After`; UI shows “Spotify busy” |
| Import file not recognized | Abort transaction; report file and field |

## 7. Security and privacy
- No telemetry, no analytics SDKs, no crash upload.
- Rust’s only outbound hosts are `open.spotify.com`, `spotify.link`, `api.spotify.com` and `accounts.spotify.com`, enforced by a single HTTP module with an allow-list.
- Embed iframes are loaded by WebView2 and reach whatever Spotify hosts the embed itself uses (for example its image CDN). That traffic is Spotify’s widget, contains no user data from the library, and is outside the Rust allow-list.
- Tauri capabilities: UI gets only the app’s own commands; no shell, no fs plugin.
- Tokens never logged; logs contain no personal notes.
- Input sanitization: `src/api.ts` cleans all free text before any command (NFC, no control or bidi/zero-width characters, trimmed, length caps in `src/lib/sanitize.ts`), and Spotify URLs are built only from URIs matching TSD §4.1. Rust validates again at its boundary (M3); the UI is never the only check.

## 8. Testing
| Level | What |
|---|---|
| Unit (Rust) | Link parser (table of real link forms), play rule, normalization, reconciliation, dedupe hash, time-zone conversion, backup rotation, promotion rule |
| Fixtures | Synthetic export folder matching 4.8; replaced/extended with anonymized structure of the real export in M4 |
| Performance | Generator: 10k items (M3 check: search, navigation, start); plus 500k events and a 10-year synthetic export (M4 check: queries < 200 ms, import < 60 s). Run with `cargo test --release -- --ignored perf` |
| Integrity | Kill the process during a large write and during backup; library opens and passes `PRAGMA integrity_check` (NFR-05) |
| Network | Capture outbound connections during idle and during normal use; only allow-listed hosts plus embed traffic appear (NFR-07) |
| Manual | Per-milestone acceptance checklist in [MILESTONES.md](MILESTONES.md) |

## 9. Repository layout
```
spotify-companion/
  BRD.docx         source of truth (humans); BRD.md generated copy (agents)
  CLAUDE.md        agent guide: playbooks, dev flow, proof
  .claude/         skills/ (playbooks), agents/ (subagents), settings.json (Stop-hook proof)
  scripts/         brd_to_md.py, check.py (universal proof)
  docs/            TSD.md DESIGN.md API.md CAPABILITIES.md MILESTONES.md USER_ACTIONS.md SOURCE_MAP.md kb/
                   plans/ (one plan per milestone, written before it starts)
                   EXPORT_FORMAT.md (written in M3, FR-020)
  src/             React UI (pages/, components/, styles/ (index.css → Tailwind + tokens.css @theme + app.css recipes), api.ts = typed invoke wrappers, mocks/ until M6/M7)
  src-tauri/
    src/           main.rs, commands.rs, db/, links.rs, library/, capture.rs,
                   logger/, importer/, insights/, backup.rs, spotify/ (R3), net.rs
    migrations/    0001_init.sql, ...
    tests/fixtures/
```

## 10. Open technical items
| # | Item | Resolve in |
|---|---|---|
| T1 | ~~Export file and field names (4.8)~~ Resolved 19 Oct 2026 for streaming history ([KB](kb/spotify.md#extended-streaming-history-format-t1--verified-2026-10-19)). Account data `Playlist*.json` not received, still ⚑ in 4.8 | M4 |
| T2 | ~~NSIS checkbox relabel and update-mode detection in the pre-uninstall hook (4.11)~~ Resolved 20 Oct 2026 ([KB windows-tauri.md](kb/windows-tauri.md#tauri-2)) | M6 |
| T3 | Re-check redirect-URI rules just before registering the app (5.2); confirmed 18 Oct 2026; dynamic loopback port verified with a real login 19 Oct 2026 (registered `:8888`, `:58163` accepted) | M5 |
| T4 | ~~Incognito (InPrivate) WebView2 works with Spotify embeds and keeps no disk cache (§1)~~ Resolved 18 Oct 2026: yes ([KB](kb/windows-tauri.md#webview2)); re-check in the real app window in M2 | M2 |
