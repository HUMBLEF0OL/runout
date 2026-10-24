# Milestone Specifications — Runout

Delivery order (BRD D30): **design → UI on mock data → backend → integration**, as strict phases: the backend phase (M3–M5) starts only when M2 is done; the integration phase (M6–M7) starts only when M3, M4 and M5 are all done. See [plans/README.md](plans/README.md).

Spec-driven rule: before a milestone starts, its specs are current (BRD, [TSD](TSD.md), [DESIGN](DESIGN.md), [CAPABILITIES](CAPABILITIES.md)) and its plan exists at `docs/plans/M<n>-<slug>.md` (M1–M7 written 18 Oct 2026). The UI ↔ Rust command surface is fixed in [API.md](API.md). Efforts assume agentic development (BRD D31): build-and-review hours, plus real-world waits agents can't shorten. A milestone is **done** only when every acceptance check passes. `U-n` = a hard stop that needs you ([USER_ACTIONS.md](USER_ACTIONS.md)). `CAP-n` = [capability](CAPABILITIES.md). `SC-n` = [screen](DESIGN.md#6-screen-inventory-m1-designs-all-of-these).

| Milestone | Phase | Releases | Capabilities | Your hard stops | Effort (agentic, BRD D31) |
|---|---|---|---|---|---|
| M0 Foundations | setup | — | — | U-1 | done |
| M1 Design | design | MVP, R2, R3 | all (SC-01–13) | U-9 | approved 18 Oct 2026 |
| M2 UI build | UI | MVP, R2, R3 | all, on mock data | — | done 18 Oct 2026 |
| M3 Backend: MVP core | backend | MVP | CAP-01–17, 35 | — | done 18 Oct 2026 (about 3 h; was estimated 3–4 weeks) |
| M4 Backend: listening and insights | backend | R2 | CAP-18–27 | U-2 | done 20 Oct 2026 |
| M5 Backend: Spotify connected | backend | R3 | CAP-28–34 | U-6, U-7, U-8 (opt.) | done 20 Oct 2026 (build ~4 h + live Studio check; was estimated 4–6 h after the O1 gate) |
| M6 Integration: MVP release | integration | MVP | CAP-01–17, 35 | U-3 dropped, U-4 done, U-5 (parallel, D32) | done 21 Oct 2026 (code 20 Oct; installed-build run + fixes 21 Oct) |
| M7 Integration: R2 and R3 | integration | R2, R3 | CAP-18–34, 36 | U-10 done | done 22 Oct 2026 (build + 8 review rounds 21 Oct; live runs 21–22 Oct) |
| M8 Stabilization & UI Polish | stabilization | all | — | — | per plan (taste skill UI + gap fixes) |
| M9 Interactive UI refinement | UI | all | — | — | per screen (Settings done 24 Oct 2026) |

### Status (24 Oct 2026)
- **M9 started 24 Oct 2026** (user: refine the UI with taste + modern UI skills, highly interactive elements with animation, Settings first). DESIGN §2 motion dial 5 → 7. WI 1 Settings done: tabs (one section at a time, spring underline, panel slides in, `?tab=` deep links), toggle switches with rollback, pointer spotlight, count-up metrics, animated disclosure, copy pairing code. WI 2 done: switches for every on/off setting app-wide, spring check-mark checkboxes, spring segment indicators; new `check.py` "hover keeps selected". Native CSS/JS only, no new dependency. check.py green, journeys 31/31; unrelated screens' screenshots differ by timing noise only. Consistency sweep 24 Oct 2026 (external audit, 18 findings + user: tabs/toggles/buttons consistent app-wide; three parallel agents by page group): every tablist draws as tabs with a sliding panel (`useSlide`), every on/off is `Switch`, one button rule (DESIGN §5 Buttons), shared `CountUp` in `Stat`, `CopyButton` on every value typed elsewhere, radii on the 3/6/10/16 scale, one danger-frame token, design-system page updated; new `check.py` "control consistency" (radius scale, aria-pressed labels) and "hover keeps selected" now covers box-shadow/border-color. Next: WI 3 (Home / Now playing) per [plan](plans/M9-motion-ui.md).
- **M8 build installed 24 Oct 2026** (`ebf9c02`, user-approved): data backed up first to `%LOCALAPPDATA%\Runout-preM8-backup-20261024` (164 files, 207 MB), NSIS `/S` over the 22 Oct install, launched; real DB migrated to `user_version` 7 (`backlog_history` present), `quick_check` ok, 180,302 listening events and 7 records intact. U-5 daily use now runs on the M8 build. M8 waits only on U-5 and G-40.
- **M8 23 Oct 2026**: WI 1–4 done on `dev` (`68b23e3`, `2b8de12`). WI 2: Rust hardening audit, 3 defects fixed with tests. WI 3: five drawn states, sliding segment, row enter. WI 4: BRD §4.4 candidates evaluated in the plan; the user picks (recommendation: listening analytics first, then Markdown/CSV export). External audits (spec + UI taste): 16 findings, all fixed except the G-44 low items. A re-audit found 3 more (corrupt deflate stream mapped to `io`, `awake_since` on a tick that doesn't observe, empty-page paging cycles), fixed with tests. Acceptance 2/5 ticked. The other three wait on U-5 daily use and G-40 (live sign-in). G-38 closed by a side-by-side netwatch run (no WebView2 calls). New gaps: G-42, G-43, G-44.
- **M8 gap re-check 22 Oct 2026**: of the 10 gaps closed in `e0b93be`, G-6 (by design), G-16, G-31, G-34, G-39, G-41 are closed; G-16 had a bug (every new backlog item counted as moved) and G-39/G-41 were re-fixed in code; G-33, G-37 accepted; G-38 (switches unverified) and G-40 (never run live) reopened. Still open: G-21, G-27, G-28, G-32, G-38, G-40 (daily use, U-5).
- **M7 closed 22 Oct 2026**: acceptance 7/7 with evidence. WI 1–4 and 6 built (`f01432a`…); external review: 8 spec-reviewer rounds, every finding fixed; installed-build runs on real data: journeys F 6/6, G 8/9 + FR-042 live, O offline 7/7, H 4/4; U-10 Inbox phone test done. Live-only bugs found and fixed: rating arrows dropped presses, Mode switch focus, HTTP 411 on body-less Spotify writes, misleading connect error. Deferred with owners (M8 candidates): G-21, G-27, G-28 (rest), G-32 (`bulk_add_album_tracks`), G-39. Next: BRD §15 metrics review with the user after U-5 daily use; M8 candidates get their own spec + plan.
- **Phase:** backend phase done (M3, M4, M5 closed); **M6 closed 21 Oct 2026** (acceptance 10/11 with evidence; the 11th, U-5 daily use, runs beside M7 and doesn't gate it, D32). Next: M7. Branch `dev` → merge to `master`; commits only, no remote.
- **M6 installed-build run** 21 Oct 2026 (`scripts/installed/`, the user's data moved aside and restored): journeys A–E, H 23/23 on real data, 60+ real items, NFR-04 on 10k (start ≤ 585 ms, navigation ≤ 355 ms), offline, network capture, all three uninstall cases. Six defects found only on the installed build and fixed: the build ran on mocks, "Open Spotify" did nothing (Tauri drops new windows → `open.rs`), the idle app probed Spotify every 30 s, an offline-banner flash at start, an open Backlog missed phone shares, WebView2 idle calls to Microsoft; plus a native tray and single-instance (G-14). Commits `b36df3d`…`efae829`; spec reviews after each round, findings fixed. One test backup of the user's (18 Oct, dev data) was rotated out by the max-3 rule during the run; its library is intact.
- **M0** done. **M1** designs approved (U-9, 18 Oct 2026); its other acceptance lines were never ticked on the boards themselves, and M2 met them in code (states per screen, `tokens.test.ts` contrast, compliance review with no open findings).
- **M2** done and closed: all 13 screens + design-system page on mock data, Tailwind v4, `src/api.ts` = full API.md contract behind mocks, input sanitization, T4 verified in the real window. Proof: `python scripts/check.py` (92 Vitest tests) and `node scripts/journeys.mjs` (30/30 keyboard-only steps, journeys A–H). Commits `9cd6c2e`, `df6b155`, `fc12844`.
- **Contract additions made in M2** (backend must implement): `create_playlist_from_items`; dashboard tile fields; `Unmatched.candidates`; `CollectionInsights` stat fields; `NewItem.artist_name/notes/promoted_fields`; `ItemPatch.artist_name`; `ItemSummary.album_name/added_via/recommended_by`; `ItemDetail.track_list` (all in API.md).
- **Known gaps:** see [Known gaps](#known-gaps) below (G-2 to G-5 closed in M3; G-1, G-7, G-8 filled in Rust in M4).
- **Your open hard stops:** U-1/U-2 done 19 Oct 2026 (Extended Streaming History only; Account data export not yet received, needed for M4 WI 7); U-6/U-7 done (live Studio check passed 20 Oct 2026); U-4 done 20 Oct 2026, U-3 optional (D33: phone capture is the Runout Inbox); U-5 (2 weeks daily use) runs in parallel with M7 and is the MVP's final sign-off (BRD D32); U-10 Inbox live test in M7.
- **M3 closed** 18 Oct 2026: all 8 acceptance lines ticked with evidence; four external spec reviews, every finding fixed. Contract changes in M3: `ItemSummary.notes`, `NewItem.album_id`, `ItemPatch.type`, `search` `type`, `recent_activity` dropped.
- **M3 post-close audit** 19 Oct 2026: 3 rounds of parallel spec reviews + 2 verification passes, 36 findings fixed (FTS secure-delete migration `0002`, restore/move safety copies, capture off mid-request, link-retry races, settings load, input clamps); check.py + journeys 30/30 green.
- **M4 closed** 20 Oct 2026: logger, importer and insights on `dev` (merge `d0ea598`); perf `4ad5a14` + 0005 rollup `ec01edf` (G-17); external audits `4f53129`, `df92324`, `e92ae82`; real export verified (WI 4, `af65b58`/`44c4ef5`) and imported (`3223b36`: 180,149 plays, re-import adds 0, no forbidden fields); SMTC 3 live tracks (`d3225fe`); 22 h 53 min logger run within NFR-17. Acceptance 9/9 ticked with evidence (20-play spot-check on real data found and fixed a matching bug, 20 Oct). Deferred with owners: G-21, G-27–G-29. BRD O2 partly resolved (S14: export took ~1 day; frequency open).
- **M5 closed** 20 Oct 2026: acceptance 6/6 with evidence (2 lines live on the user's test playlist); WI 1–7 done; external audit: 3 rounds (19 Oct), r4, redesign + 3 rounds, r5 (worktree fixers), final 3-lens round (10 confirmed, fixed in `88392fc`, diff re-audit clean); check.py green, journeys 30/30. BRD O1, O4, O5 resolved. Deferred with owners: G-22, G-23, G-32 (M7), G-24 (M7), G-31, G-33 (M8). History below.
- **M5 code done** 19 Oct 2026: WI 1–7 built on `dev` (`src-tauri/src/spotify/`, stub-tested, never the real API); 3 external audit rounds, 40 confirmed findings fixed (single-flight refresh, disconnect/revoke purge the cache, reconnect marker survives restart, Studio lock, undo conflict check + unclear-write handling, one undo per playlist); check.py green. Acceptance 4/6 ticked (O1, code check, refresh rotation/reconnect, disconnect). G-24 (duplicates across playlists) deferred to M7. **Live run 20 Oct 2026** (test playlist, `studio_roundtrip`): the write landed but `GET /playlists/{id}?fields=snapshot_id` stayed stale 30 s+, so undo hit a false `conflict`; apply/undo now compare track URIs, never `snapshot_id` (`f27081e`, `efacba1`, audit r2). Items-read freshness verified live (G-30 closed). Audit r3: a failed apply/undo now reloads the draft and undo record (since r5 only after Studio's own apply/undo, and not when the error means nothing was written), undo-specific conflict copy (G-25 closed), undo waits 2 min (`spotify_busy`) before trusting a read of the list from before an unanswered write, `--probe` reads via `studio::current_uris`.
- **M5 live acceptance passed** 20 Oct 2026 on the user's test playlist (`studio_roundtrip`): round trip (cut 3, dedupe 1, apply, undo → `identical to the original: true`) and `--conflict` (apply blocked once the Web API showed the edit, ~9 s after it was made in the Spotify app). `--probe` reorder: `GET /playlists/{id}/items` showed the move at once; every `snapshot_id` read lagged ~33–36 s. Acceptance 6/6.
- **Next:** U-5 daily use (MVP final sign-off, BRD D32) and Account data export delivery (G-27).

### Known gaps
Found while building and closing M2 (18 Oct 2026). None blocks a release criterion. "Contract" = needs an API.md change first (API.md §7), then `api.ts`, the mock and Rust together. Remove a row when it's fixed; say in the commit which G-n it closes.

| # | Gap | Where | Why it's open | Fix in |
|---|---|---|---|---|
| G-11 | Embeds in mock data show Spotify's "page not found" | mock data | Mock Spotify IDs are random by rule (no real IDs committed); a real pasted link renders fine | won't fix (by design) |
| G-12 | Embeds play 30-second previews, not full tracks | WebView2 InPrivate | Incognito keeps no Spotify login (T4, `docs/kb/windows-tauri.md`); accepted trade-off for no disk cache | accepted |
| G-13 | Digits 1–5 don't set status while focus is on the rating slider | `src/hooks/useShortcuts.ts` | Arrow keys own the slider; digits there would be ambiguous | by design |
| G-21 | Logger replay detection relies on Spotify moving the SMTC timeline position on repeat; if it doesn't, a repeat is one long play (old behaviour) | `src-tauri/src/logger/accumulator.rs` | Not yet exercised in the running app | M8 candidate (observe a repeat-one play in daily use, U-5) |
| G-27 | `Playlist*.json` (Account data export) never checked against a real file; playlist import + check-up proven on the synthetic fixture only | `src-tauri/src/importer/playlists.rs`, `source.rs` | Account data export not received (U-1 note); when it arrives: verify file/field names (T1 rest), adjust parser, import into a temp copy | M8 candidate (when the Account data export arrives) |
| G-28 | In-app logger never run live: the 24 h run used `examples/smtc_spike.rs` (same session + accumulator, no DB writes); DB writes, `play-logged`, autostart, and the tray (built 20 Oct 2026, `src-tauri/src/tray.rs`; tooltip, pause/resume, close-to-tray, quit unit-tested only for the tooltip text) unexercised in the running app | `src-tauri/src/logger/`, `src-tauri/src/tray.rs` | App UI runs on mocks until M6/M7 switch-over | M8 candidate (DB writes + `play-logged` verified live 22 Oct 2026 on the installed build; autostart, tray pause/resume and shutdown save still to see in daily use, U-5) |
| G-33 | Apply and undo wait a fixed 2 minutes after a playlist write with an unknown outcome; a write Spotify finishes later than that could still land after Runout acts | `src-tauri/src/spotify/studio.rs` (`UNSURE_WAIT`) | Both re-read and compare the items right before writing; no Spotify signal says a pending write is done | accepted (revisit only if a late write is ever seen) |
| G-37 | Real `spotify.link` short-link resolution never exercised; `links::resolve_short` verified only on a synthetic code | `src-tauri/src/links.rs` | The user's Spotify apps share full `open.spotify.com` links, even to WhatsApp (21 Oct 2026) | accepted (paste a real `spotify.link` if one ever appears) |
| G-40 | Inbox optional imports (saved albums via `GET /me/albums`, followed-artists diff via `GET /me/following`) and the `user-follow-read` re-approval flow only stub-tested | `src-tauri/src/spotify/inbox.rs`, `auth.rs` | Both options are off by default; the U-10 live test ran with them off | M8 (turn each on once, sign in again, save an album / follow an artist, Check now; U-5) |
| G-32 | Autofill, album bulk-add, playlist from filter/items, top items, recent plays and library save/remove have only run against stub HTTP responses; only sign-in and Playlist Studio ran live (20 Oct 2026) | `src-tauri/src/spotify/{autofill,create,discover,library_sync}.rs` | M5 acceptance covers Studio + auth only; the UI still calls mocks for R3 | M8 candidate (all live 21–22 Oct 2026 except `bulk_add_album_tracks`: first real use will show it) |
| G-42 | Logger: a PC that sleeps *during* a long db-lock wait (import/backup) counts that sleep as play time; sleeps between ticks are clamped (`MAX_GAP_MS`) | `src-tauri/src/logger/accumulator.rs` (`awake_since`) | Needs a monotonic-vs-wall-clock compare; both events together are rare | M9 candidate (if a too-long play is ever seen) |
| G-43 | Studio apply doesn't cap `keep` at 10,000 tracks | `src-tauri/src/spotify/studio.rs` | Only reachable if the UI sends more than the draft holds | M9 candidate |
| G-44 | UI audit low items left: inner 8 px radius on segment buttons, `.btn.s`, badges and menu items (nested radius = outer − padding); `.rail-box` and `.steps li` plain surface fills; recipe hex values for the embed placeholder and `.btn-light` hover | `src/styles/app.css` | Deliberate or cosmetic; DESIGN §3 radius table lists outer shapes only | accepted (revisit in a light-theme pass) |
---

## M0 — Foundations

**Goal:** a running empty app, the proof script green, and the Spotify exports on their way.

Work items
1. `git init`; `.gitignore`; scaffold Tauri 2 + React + TS + Vite with bundle identifier `app.runout.desktop`. *(done 18 Oct 2026)*
2. `scripts/check.py` green on the empty scaffold. *(done)*

Acceptance
- [ ] App launches in dev mode.
- [ ] `python scripts/check.py` exits 0.
- [ ] Documents reviewed and consistent (BRD §14 M0 exit).
- [ ] **U-1 done**: both Spotify exports requested.

## M1 — Design

**Goal:** every screen designed in direction D · Pulse, walked through, and approved before any UI code.

Work items
1. Design spec `docs/DESIGN.md`: principles, tokens, type, components, screen inventory, compliance rules. *(done)*
2. Design every screen SC-01 to SC-13 in the design prototype (`docs/design/prototype/`), driven by the shared mock dataset, each with its loading, empty, offline and error state.
3. Walkthrough: one board per BRD journey A–H showing the screens in order (or links between boards).
4. Compliance pass on the designs (`compliance-review`): attribution, embeds unaltered, ≤ 20 artwork per set, no source-A data in insights, no Spotify look-alike.

Acceptance
- [ ] Every SC-n in DESIGN.md §6 has a board; every CAP-01–35 appears on at least one screen.
- [ ] Every screen shows empty, offline and error states.
- [ ] Contrast of every text token pair ≥ 4.5:1.
- [ ] Compliance pass has no open findings.
- [x] **U-9** designs approved (18 Oct 2026).

## M2 — UI build (mock data)

**Goal:** the whole app clickable in Tauri, built from DESIGN.md, backed by typed mock data.

Work items
1. Tokens (`src/styles/tokens.css`), self-hosted fonts, Phosphor icons; shell (rail, top bar), routing.
2. Components from DESIGN.md §5, each with Vitest for its logic (rating keyboard, link detection on paste, freshness text).
3. `src/api.ts` typed wrappers per [API.md](API.md) with a mock implementation (`src/mocks/`) behind the same types the Rust commands will return; switching to real commands changes no page code.
4. Screens SC-01 to SC-13, including empty, offline and error states.
5. Embed slot: derived iframe URL, lazy mount, placeholder offline; incognito web view (TSD §1, **T4**).
6. Keyboard shortcuts: `Ctrl+N` add, `Ctrl+K` search, `1–5` status on focused item.
7. Accessibility pass: keyboard-only walkthrough; WCAG 2.1 AA contrast (4.5:1).

Acceptance
- [x] Every SC-n matches its approved board (side-by-side review). *Evidence: per-screen headless captures compared with `docs/design/screens/` by each build agent; Tailwind migration pixel-diffed against pre-migration captures (0 px on settings, library, search, item); approved by the user 18 Oct 2026 with U-9.*
- [x] Journeys A–H clickable end to end on mock data. *Evidence: `node scripts/journeys.mjs` → 30/30 steps PASS, keyboard only, screenshots in `docs/design/screens/m2-journeys/` (18 Oct 2026).*
- [x] `src/` contains no `fetch`, SQLite or network code; only `api.ts` and embed iframes (TSD §1 rule). *Evidence: `check.py` "ui purity" check (no fetch/XHR/WebSocket/sqlite/plugin imports; only `src/api.ts` imports `@tauri-apps/api`) passes.*
- [x] T4 resolved: embeds work in incognito web view and leave no HTTP cache in `EBWebView`. *Evidence: wry probe + real `runout.exe` window (real album embed rendered, logged-out "Preview"); after exit `Cache_Data` empty, no written file names a Spotify host; `docs/kb/windows-tauri.md` T4.*
- [x] All text passes 4.5:1 contrast (NFR-12); every action reachable by keyboard. *Evidence: `src/styles/tokens.test.ts` (34 token pairs ≥ 4.5:1); `journeys.mjs` M1–M3: reduced motion honoured, every rendered button has an accessible name, focus ring at all 65 keyboard stops; two focus bugs fixed (dialog autofocus, input outline).*
- [x] `python scripts/check.py` exits 0. *Evidence: ALL CHECKS PASS at close, 18 Oct 2026 (92 Vitest tests).*

## M3 — Backend: MVP core

**Goal:** the MVP’s Rust core complete and tested, exposing the commands `api.ts` expects.

Work items
1. Rust skeleton per TSD §9; migration runner; `0001_init.sql` (MVP tables + `usage_event`).
2. `links.rs` — link parser (TSD §4.1) incl. short-link resolve; offline queue in `unresolved_capture`.
3. `net.rs` — allow-listed HTTP client; `online()` check.
4. `library/`: item CRUD, relationships (by artist, on album, version of), duplicate block with “open existing” (FR-004), statuses + history, flags, priority, journal fields, tags (rename, merge), backlog order, saved filters, FTS5 search, promotion rule and provenance (TSD §4.2), “name needed” items.
5. Rediscovery and dashboard queries.
6. `capture.rs` listener (TSD §5.1).
7. Export (JSON with provenance) and `docs/EXPORT_FORMAT.md` (FR-020).
8. `backup.rs` (TSD §4.10): weekly + manual, default folder `Documents\Runout\Backups`, rotation 3, size warning, restore with verification.
9. Delete record / delete library; `--uninstall-cleanup` command (TSD §4.11).
10. Usage counters (TSD §4.12, FR-045).
11. Tauri commands matching [API.md](API.md) §3 and the `api.ts` types; `check.py` compares command names.
12. Performance generator (10k items).

Acceptance
- [x] `cargo test` green, incl. ≥ 15 link-parser cases, promotion rule, duplicate block, tag merge, backup rotation ×4. *(18 Oct 2026: `cargo test` lib + integrity + persistence green; `links::tests::parse_table` 35 cases; `provenance` promotion tests; `items` duplicate block; `tags::merge_keeps_every_item_tagged_once`; `backup` rotation ×4 keeps newest 3)*
- [x] Every `api.ts` MVP call has a Rust command with matching types. *(43 §3 commands in `commands.rs`, enforced by check.py `commands`; serde structs compared field by field in the pre-close review; `commands::tests::invoke_keys_match_api_ts` proves the invoke keys)*
- [x] NFR-04 on 10k items: search < 200 ms; search finds a word that appears only in a review. *(`tests/perf.rs` release: worst search 24.9 ms, list page 9.9 ms, start 48 ms; review-only word 1 hit in 0.3 ms)*
- [x] All data persists across restart; merging two tags keeps every item tagged once. *(`tests/persistence.rs`: file DB written, dropped, reopened, full ItemDetail + filters/tags/counts/search equal; tag merge test above)*
- [x] Kill during a large write and during backup → library passes integrity check (NFR-05). *(`tests/integrity.rs` release: killed mid-write at 400/1300/2700 ms and mid-backup at 30/250/700 ms, `integrity_check` ok every time)*
- [x] Listener: wrong code ×5 disables it; non-LAN request rejected; 11th request in a minute rejected; body > 2 KB rejected; non-Spotify link rejected (NFR-18). *(`capture` tests for each rule plus socket tests: 413 before read, 411 chunked, 408 stalled, lockout closes the port; std listener with timeouts, G-17)*
- [x] Backup size recorded against the NFR-11 estimate; restore of the oldest verifies counts. *(`docs/kb/perf-integrity.md`: 10k library 13 MB → 4.2 MB zip vs 10–30 MB estimate; `backup::tests::restore_of_oldest_verifies_counts`)*
- [x] `docs/EXPORT_FORMAT.md` exists and matches an actual export file. *(`export::tests::doc_matches_a_real_export` compares the doc's keys with a real export)*

## M4 — Backend: listening and insights

**Goal:** a complete, de-duplicated listening history and the insight queries over it.

Work items
1. SMTC watcher spike, then `logger/` (TSD §4.5): allow-list, accumulation, 5 s fallback poll, autostart option, **Pause logging**. — **done** (spike covered one live track; see acceptance)
2. `0003_listening.sql`. — **done**
3. `importer/`: folder/zip → detect file types → preview counts → import in one transaction. — **done**
4. Verify real export format (**T1**, needs **U-2**) and adjust parser; anonymized fixture from its *structure* only. *(done: streaming history verified and importer adjusted `af65b58`/`44c4ef5`; `Playlist*.json` still unverified until the Account data export arrives)*
5. Minimization (drop IP/user agent/username), skip non-music rows, UTC→local, private-session flag. — **done**
6. Dedupe hash; one-to-one reconciliation (TSD §4.7); matching to items (TSD §4.6) and the unmatched list. — **done**
7. Account-data import (playlists only) and read-only playlist check-up queries. — **done** on a synthetic fixture (real Account data export not yet received, U-1)
8. `insights/` SQL views (TSD §4.9) with sources and freshness; unfinished albums only for albums with a track list; year in review; private-session setting. — **done**
9. 500k-event + 10-year synthetic export generator. — **done** (`tests/perf_listening.rs`, `4ad5a14`)

Acceptance
- [x] SMTC spike logs 3 consecutive real Spotify tracks with plausible `ms_played` (pause excluded). *(19 Oct 2026, `examples/smtc_spike.rs` release build: 3 consecutive tracks closed at 54.4 s (joined mid-track), 180.3 s and 193.7 s; no pause occurred live, pause exclusion proven by `logger::accumulator::tests::accumulation_table`)*
- [x] Real lifetime history imports; second import adds 0 rows. *(19 Oct 2026, `examples/real_import.rs` on a temp copy of the library (deleted afterwards): 180,149 plays added, 48 duplicates merged, 18.7 s; second import added 0; forbidden-field check over every column and TEXT value: 0 hits)*
- [x] No IP/user agent/username anywhere in `library.db` (query check). *(`importer::run` test `forbidden_fields_never_reach_the_db` walks every TEXT column)*
- [x] Overlap test: no double counts, incl. featured-artist, compilation and back-to-back repeat fixtures. *(`importer::run` `overlap_has_no_double_counts`; logger `same_track_replayed_back_to_back_is_two_plays`)*
- [x] Matching: spot-check 20 matched plays, all correct (FR-030). *(20 Oct 2026, temp DB from the real export (deleted afterwards): your 20 most-played tracks entered as hand-typed records (name + artist, no URI), export re-imported → first pass 12/20: a lone name match took same-titled songs by other artists ("Rose", "Silence"); fixed in `importer/matching.rs` (lone match needs artist overlap, TSD §4.6, table test) → 20/20: every matched play is that song by that artist (4 span 2–3 releases of the same song))*
- [x] NFR-04: 10-year import < 60 s; over 500k events filter results < 200 ms and screen navigation < 500 ms (BRD NFR-04 wording). *(met in `tests/perf_listening.rs`, release, worst of 5, 19 Oct: import 21.4 s; screen loads 7d/30d/12m/all 38/67/362/58 ms, collection ≤ 28 ms, take 433 ms, year review 139 ms, record panel 100/100 ms (< 500 ms); `list_plays` 7d/30d/all 2–6 ms, `list_unmatched` 17 ms (< 200 ms); all-time reads via the 0005 rollup, G-17. [KB](kb/perf-integrity.md))*
- [x] Logger runs 24 h within NFR-17; paused time logs nothing. *(19–20 Oct 2026: `examples/smtc_spike.rs` (logger session + accumulator, release build) ran 22 h 53 min: CPU 8 s total ≈ 0.01% avg (< 1%), memory 6.8 MB, peak 16.5 MB (< 50 MB), 248 handles; 78 plays closed, 15 pauses seen, longest play 7.1 min, plays spanning a pause 18–269 s, so paused time added nothing; pause/off writing nothing unit-tested. The in-app logger's own live run (DB writes, tray, autostart) is G-28)*
- [x] Three insights hand-verified against raw data; year in review matches a manual total (FR-034). *(`listening_week_matches_hand_totals`, `collection_matches_hand_counts`, `take_matches_hand_counts`, `year_review_equals_manual_sum` on the hand-built `insights::fixture`)*
- [x] Code check: `insights/` has no reference to the Spotify cache DB. *(`check.py` "insights no cache")*

## M5 — Backend: Spotify connected

**Goal:** the Spotify Web API layer, safe and compliant.

Gate (before any R3 code): **U-6** developer app created → one API call → confirms the Student Premium plan works (**O1**). If it fails, R3 stops and the decision returns to you. Optional **U-8** (O7).

Work items
1. Re-check **T3**; PKCE login with dynamic loopback port (**U-7** first login); Credential Manager incl. refresh-token rotation; status; reconnect on `invalid_grant`; disconnect deletes `spotify_cache.db`. *(done 19 Oct 2026)*
2. `spotify_cache.db` (cache entries + playlist snapshots); 7-day cache / 30-day snapshot sweeper (O4, O5). *(done 19 Oct 2026)*
3. Playlist Studio logic: list own/collab playlists → import → duplicates within/across → draft → diff → re-read the playlist's tracks and abort if they differ from the draft (track content, not `snapshot_id`: stale on GET, verified 20 Oct 2026) → apply → 30-day undo snapshot. Ratings and tags only, no listening data (BRD 11.1, D27). *(done 19 Oct 2026; duplicates across playlists deferred: G-24)*
4. Playlist from saved filter. *(done 19 Oct 2026)*
5. Autofill (cache → confirm → promote); album track bulk-add. *(done 19 Oct 2026)*
6. Top items and recent plays commands (display only; add-to-backlog via promotion rule). *(done 19 Oct 2026)*
7. Save to / remove from Spotify library. *(done 19 Oct 2026)*

Acceptance
- [x] O1 resolved. Student Premium works: PKCE login + `GET /me` → 200 `product=premium`, 19 Oct 2026 (KB spotify.md, commit `5b92162`). BRD S1/O1 updated (Change History P3).
- [x] Playlist round-trip on a **test playlist**: cut 3, dedupe 1, apply, verify in Spotify, undo, verify restored. Live 20 Oct 2026, user's test playlist, `cargo run --example studio_roundtrip -- spotify:playlist:<id>` at `d416796`: 4 removed, user checked the cut in Spotify, undo → `undone: 15 tracks, identical to the original: true`. Earlier runs the same day found and fixed: OAuth `state` with `~` failing the callback compare (`d34872e`), stale `snapshot_id` reads giving undo a false conflict (`f27081e`). Stub: `studio::tests::snapshot_is_stored_before_the_write_and_undo_restores_it`, `stale_snapshot_id_after_the_write_does_not_block_undo`.
- [x] Change the playlist in Spotify after drafting → apply blocked (pre-apply check). Live 20 Oct 2026, `--conflict` at `1ce339d`: user moved/removed a track in the Spotify app; the Web API showed it after ~9 s; apply → `PASS: apply blocked with conflict; nothing was written.` An edit the Spotify app hasn't synced yet is invisible to the Web API, so to the check (accepted, TSD §5.2; a first run that applied at once went through for this reason). Stub: `studio::tests::changed_items_are_a_conflict_whatever_the_snapshot_id`.
- [x] Code check: Playlist Studio never queries `listening_event` or `exported_playlist*`. `scripts/check.py` `check_spotify_rules` fails on either name anywhere in `src-tauri/src/spotify/`; green 20 Oct 2026.
- [x] Refresh that returns a new refresh token stores it; revoked token → reconnect state. Tests `auth::tests::expired_token_refreshes_and_stores_the_rotated_refresh_token`, `invalid_grant_deletes_tokens_and_asks_to_reconnect`, `concurrent_callers_share_one_refresh`, `stale_invalid_grant_keeps_newer_tokens`, `mod::tests::reconnect_survives_restart_and_purges_the_cache`.
- [x] Disconnect → cache DB (incl. snapshots) gone immediately; backups contain no Spotify-provided data. Tests `cache::tests::disconnect_leaves_no_file_and_no_tokens`, `backup_zip_contains_no_cache_content`, `mod::tests::disconnect_during_a_request_leaves_no_file_and_no_tokens`, `refresh_answer_after_disconnect_is_dropped`, `studio::tests::disconnect_waits_for_a_running_apply`.

## M6 — Integration: MVP release

**Goal:** MVP screens on real data, installed, and used daily.

Work items
1. Switch MVP calls in `api.ts` from mocks to Rust commands; remove MVP mocks.
2. Phone capture end to end; Settings shows LAN IP, port, pairing code; Shortcut instructions screen.
3. NSIS installer; pre-uninstall hook; relabelled checkbox (TSD §4.11, **T2**).
4. Success-metrics view on real usage counters.
5. Offline, network-capture and performance checks on the installed build.

Acceptance
- [x] All BRD §13.1 MVP criteria pass. *(installed NSIS build, 21 Oct 2026, `scripts/installed/`: create/edit/delete, URL/URI detection, short link queued offline; statuses + revisit + history, rating/notes/tags persisted across 10+ relaunches; search/filters/backlog/rediscover/dashboard on real data and 10k (NFR-04 below); embeds on detail pages, artwork sets via the shared `ArtTiles` (≤ 20, ends with Open Spotify), offline placeholder; optional Wi-Fi capture accepted a real LAN POST with the code (journey B); backup (max 3 rotation seen live, zip) + verified restore (journey H); export unchanged since M3 (`export.rs` tests; save picker not driven); uninstall per FR-024 (below); offline run below. Fixed on the way: `5b69530`, `8474179`)*
- [x] Journeys A–E and H pass on real data. *(`scripts/installed/journeys.mjs` on the installed app, keyboard only: 23/23, 36 focus stops with a ring; real oEmbed prefill + embeds; B = real LAN `POST /capture`. A8 offline short link covered by the offline run. Found and fixed: an open Backlog missed a phone share until navigation, `8474179`)*
- [x] Save 50 real items of mixed types (incl. 5 short links, 2 while offline that resolve later); every item opens in the Spotify app. *(user decision 21 Oct 2026: the 10 real links the user shared (desktop, phone, WhatsApp) stand in for the 5 short links, because Spotify issues this user no `spotify.link`; checking a real one if it ever appears stays as G-37. 60+ real items via Quick add (16 albums, 26 tracks incl. 10 links the user shared with `si`/`utm`/`rowId` stripped, 18 artists), 2 saved offline with typed names; Open Spotify href correct 48/48 and a live click brought the Spotify app (Microsoft Store build) to the front after `open.rs` (`5b69530`: Tauri had dropped the link). Short links: the user's Spotify app shares full `open.spotify.com` links even via WhatsApp, so no real `spotify.link` was available; a synthetic one queued offline and failed cleanly online)*
- [x] Pasting an existing link is blocked and “open existing” opens it; an episode link is rejected with a clear message; two album versions linked as `version_of`. *(installed build: every re-pasted link → "Already saved … Open existing" → item page; episode → "This is a podcast episode. Runout keeps albums, tracks and artists."; item 15 `version_of` item 48 via the item editor's link picker)*
- [x] No oEmbed response shape (`"html":`, `thumbnail_url`) in any file under app data. *(after real prefill + embed use: 0 hits in 260 files / 72 MB under `%APPDATA%` and `%LOCALAPPDATA%pp.runout.desktop`, EBWebView included)*
- [x] Attribution present on every screen that shows Spotify-sourced content. *(installed build: Quick add "Suggested by Spotify" + Open Spotify (A2), item pages Open Spotify 48/48, Rediscover artwork set Spotify logo + Open Spotify (E3); Backlog/Library/Home artwork use the same `ArtTiles`, which always ends with `<Attribution />`)*
- [x] Network capture: idle app makes no outbound connections; normal use reaches only allow-listed hosts plus embed traffic (NFR-07). *(`scripts/installed/netwatch.ps1`, per-connection TCP of runout.exe + its WebView2 children. Found: the online probe polled `open.spotify.com` every 30 s forever; now OS network-change events (`5b69530`). Idle 100 s after start: runout.exe 1 connection (the start-up probe), none after. Normal use: runout.exe → open.spotify.com, spotify.link only; WebView2 → Spotify embed hosts (embed/embed-cdn/encore/i.scdn/image-cdn/spclient/pixel, the embed's own Sentry). WebView2's idle edge.microsoft.com call removed with background-networking switches (`8474179`); one WebView2 start-up call to a Microsoft 365 front door remains: G-38)*
- [x] Uninstall, box unticked: cache, tokens and `EBWebView` gone; library and backups kept. Box ticked: library also gone; backups kept. Update-mode reinstall keeps the Spotify connection. *(installed build on the dev PC with the user's data moved aside, dummy `Runout/spotify` credential + cache file: unticked (`/S`) → program, cache, EBWebView, token gone; library + 3 backups kept. Ticked (real dialog, checkbox labelled "Also delete my music library") → also the whole library folder gone; backups kept. `/S /UPDATE` → exe replaced, credential, library, EBWebView kept. T2 closed)*
- [x] NFR-04 on the installed build: navigation < 500 ms, start < 3 s. *(10k fixture (`tests/common` generator) via `library_path`: start worst 585 ms of 5 cold starts; navigation worst 355 ms (Backlog), Library 50, Rediscover 67, Settings 34; search warm worst 17 ms, filter 29 ms (first cold search 346 ms); search page render 374 ms)*
- [x] ~~**U-3** Shortcut share lands in backlog as “name needed”~~ dropped by the user 21 Oct 2026 (phone capture is the Runout Inbox, D33, M7 WI 4); **U-4** firewall allowed: done 20 Oct 2026. The Wi-Fi listener itself took a real LAN `POST /capture` with the pairing code in the installed journey B (21 Oct 2026); G-34 tracks only the never-built Shortcut.
- [ ] **U-5** two weeks of daily use; success metrics (BRD §15) reviewed → MVP final sign-off. Not needed to close M6 or start M7 (BRD D32, user decision 20 Oct 2026). *(open: waiting on U-5)*

## M7 — Integration: R2 and R3

**Goal:** listening, insights and Spotify screens on real data; phone capture through the Runout Inbox.

Work items
1. Switch R2 calls (logger status, import, plays, insights, check-up) to Rust; remove mocks.
2. Switch R3 calls (connection, Playlist Studio, playlist from filter, autofill, From Spotify, library save) to Rust; remove mocks.
3. Compliance pass on the running app (`compliance-review`).
4. Runout Inbox (FR-046, CAP-36, D33, TSD §5.3): create on connect, check at start / 15 min / on demand, import as “name needed”, album batches → album item, remove imported entries; Settings section (state, Check now, Create Inbox, optional saved albums / followed artists; DESIGN §8 D33). Contract to API.md first. Verify T5 first; journey B moves to the Inbox; live test U-10.

Acceptance
- [x] All BRD §13.2 and §13.3 criteria pass. *(22 Oct 2026. **13.2:** logger records desktop plays live on the installed build (`logger_last_event` 2026-10-22T03:47Z, `source: logger` rows; ≥30 s rule unit-tested, M4 24 h run); import complete + idempotent + skips podcasts + drops IP/UA/username (journey F: 180,149 plays, 72 podcasts skipped, re-import adds 0; M4 tests); reconciliation no double counts (M4 acceptance); insights hand-checked (M4 20-play spot-check), sources + freshness + local time (F2); no source A in insights (check.py `insights no cache`, compliance r1); playlist check-up finds duplicates (M4 synthetic export, real Account export G-27); import 13.7 s < 60 s, Year in review renders (F2). **13.3:** O1 resolved; login live 22 Oct, reconnect + disconnect/deletion (M5 acceptance); Promotion Rule expiry (M3/M5 tests); playlist edit round trip with preview/confirm/undo (journey G live); playlist from filter (live, 3/3 tracks); top items + recent plays display, Add to backlog promoted (G5/G6), library save round trip (track + album, after the 411 fix); Runout Inbox (line below); core + R2 with Spotify disconnected and offline (journey O 7/7); design guidelines (compliance review r1, attribution check in check.py))*
- [x] Journeys F and G pass on real data. *(F passed 21 Oct 2026 on the installed build: import of the real Extended Streaming History, 180,149 plays (48 duplicates), re-import adds 0, insights, recent plays save + rate, unmatched link: 6/6. The native file dialog is not driven (F1 calls `preview_import`/`run_import`, the commands the page calls after the pick). G passed 21 Oct 2026 on the installed build, signed in, on the user's own 15-track test playlist (5 duplicates): Studio cut 3 + dedupe 1 by keyboard → preview → apply → read back from Spotify → Undo → original list again; Check other playlists marks tracks found elsewhere (G-24); 8/9 steps, G7 (FR-042) skipped for lack of an album item; FR-042 then passed 22 Oct by direct `save_to_spotify_library`/`library_contains`/`remove_from_spotify_library` calls on the Inbox album, after the 411 fix, see the FR-041/042 line.)*
- [x] Every insight shows sources and freshness. *(21 Oct 2026, installed build on the real export: journey F2 walks all four Insights tabs by keyboard, every card has a "From:" tag, none "Sources unknown.", each listening source shows its real date (export to 17 Oct 2026), rail freshness box too; `insightcard.test.tsx` fails a card without sources/freshness)*
- [x] Top items and recent plays: “Add to backlog” creates a promoted record (FR-041); save then remove an album in the Spotify library (FR-042). *(22 Oct 2026, installed build, signed in: journey G5/G6 Add to backlog from top tracks and recent plays → promoted records; album save → `library_contains` true → remove → false, on an album not previously saved. First live save hit **HTTP 411**: body-less PUT/DELETE sent no `Content-Length`; client now sends `Content-Length: 0` (stub transports never showed it). Track save round trip too)*
- [x] Unplug network → all MVP and R2 features work. *(21 Oct 2026, installed build with a dead proxy for Rust and WebView2 (`launch({offline:true})`), real imported history: journey O 7/7: offline banner, Backlog, Item, Insights with sources, Recent plays, Rediscover, Settings (Spotify/Inbox say offline), From Spotify and Studio show the offline/connect state)*
- [x] `src/mocks/` holds no code used by the shipped build. *(21 Oct 2026: check.py "no mocks in build" builds the production bundle with sourcemaps and fails on any `src/mocks` source; proven red with one `devState` import; `useRelease` no longer imports `devState` in a real build)*
- [x] Runout Inbox: connecting creates a private “Runout Inbox”; a track added to it on the phone (mobile data, not the home Wi-Fi) lands in the backlog as “name needed” and leaves the Inbox; adding a whole album lands one album item; deleting the Inbox shows **Create Inbox**, never a silent re-create. *(Live 22 Oct 2026, installed build: connect created the private Inbox (`ready`); a whole album added on the phone on mobile data → one name-needed album item (`added_via inbox`), Inbox emptied (0 entries left); Inbox deleted in Spotify → Check now `not_found` "The Runout Inbox is gone…", state `missing`, Settings shows **Create Inbox**, no playlist re-created (0 named Runout Inbox); recreated with **Create Inbox**, a single track added on the phone → one name-needed track item (`added_via inbox`), Inbox emptied (0 left). Note: the phone must be online (mobile data) for Spotify to sync the add)*

## M8 — Stabilization, Complete Functionality & UI Polish (Taste Skill)

**Goal:** Stabilize the entire application, ensure all end-to-end functionality works reliably with zero friction on real data, resolve remaining known gaps, and elevate/streamline the UI using the taste skill (`DESIGN.md` §2).

### Work items
1. **Application Stabilization & Core Hardening:**
   - Audit and harden all end-to-end user journeys (A through H, O) on real data and active sessions.
   - Resolve open known gaps slated for M8 (e.g., G-6 shelf entity/saved filter, G-16 backlog history metrics, G-31 playlist export matching, G-33 apply/undo timeout bounds, G-34 phone capture fallback, G-37 link resolution, G-38 WebView2 networking).
   - Solidify error handling, recovery paths, edge-case states, network disconnect/reconnect transitions, and background sync stability.
2. **End-to-End Functionality Verification:**
   - Verify every single feature works predictably without regressions: Backlog CRUD & tagging, Extended Streaming History & Account Data import, SMTC play logging, Insights aggregations, Playlist Studio dedupe/reorder/apply/undo, Runout Inbox automated capture, and Spotify Library sync.
   - Address any discrepancies or rough edges surfaced during daily use (U-5).
3. **UI Streamlining & Aesthetic Polish (Taste Skill):**
   - Apply taste skill dials from `DESIGN.md` §2: `DESIGN_VARIANCE 5 · MOTION_INTENSITY 5 · VISUAL_DENSITY 6`.
   - Ensure tactile depth across the entire app (3 elevation layers: canvas, raised, overlay with subtle hairlines, top highlights, and soft background-tinted shadows; strictly avoiding neon/glassmorphism).
   - Verify all interactive controls strictly provide five visible states: default, hover, pressed, focus, and disabled (plus loading states where applicable).
   - Refine typographic hierarchy and contrast: high-contrast JetBrains Mono for numbers, durations, and counts against muted labels; 800-weight section titles with tight tracking.
   - Streamline visual density and layout cohesion across screens (quiet dark graphite background, calm notebook feel, ambient Now-playing card radial tint).
   - Purposeful motion audit: verify feedback on press, smooth segment transitions, and polite status pulses without noisy animations.
4. **Future Candidate Evaluations (BRD §4.4):**
   - Scope and prioritize candidates (e.g. deep listening analytics, markdown/CSV export, local sync) once core stabilization is satisfied.

### Acceptance
- [ ] Zero unhandled errors or blank states across any screen or user journey on real data. *(open: waiting on U-5. 23 Oct 2026 evidence so far: journeys 31/31 on mock data incl. loading/empty/offline/error states; WI 2 Rust audit found and fixed 3 defects (sleep counted as play time, paging loop on a malformed answer, vague damaged-zip error) with tests; insights on sparse/empty DBs give finite answers (`sparse_libraries_give_finite_answers`); no `unwrap` on external data)*
- [ ] All remaining targeted known gaps (G-6, G-16, G-31, G-33, G-37, G-38) resolved or formally closed with evidence. *(re-checked 22 Oct 2026: G-6 by design (FR-013 = saved filters); G-16 fixed (`0007_backlog_history`, test `moved_after_14d_keeps_items_taken_out_while_planned`); G-31 fixed with a ceiling (track-count tie-break); G-33, G-37 accepted (rows kept); also closed: G-34 (Shortcut dropped with U-3), G-39 (60 s batch gap, `plan_collapses_album_batches_only`), G-41 (test reads before sending). Open: G-38 (switches unmeasured), G-40 (not run live))* *(G-38 closed 23 Oct 2026: a release build of `375206b` ran beside the installed app under its own identifier (`app.runout.netcheck`) and its own credential name, so it never touched the user's data or Spotify login. `netwatch.ps1 -RootPid` over 90 s from start showed no WebView2 connections at all, only runout.exe → `open.spotify.com` (the online probe; atc.spotify.map.fastly.net). The switches stay. Open, G-40: needs the user's Spotify sign-in, U-5)*
- [ ] Full functionality verified end-to-end: Backlog, Logger, Importer, Insights, Playlists/Studio, Runout Inbox, and Settings. *(open: waiting on U-5 for the live remainder: G-28 autostart/tray, G-32 `bulk_add_album_tracks`, G-40 Inbox options, G-21 repeat-one. Everything else ran live in M6/M7 installed-build runs; journeys A–H 31/31; cargo test 247 passed)*
- [x] UI passes design audit against taste skill criteria: tactile elevation tokens (`--elev-0`, `--elev-1`, `--elev-1-hover`), 5 states on every interactive control, contrast ratios >= 7:1 for text/muted, and purposeful micro-interactions. *(23 Oct 2026: WI 3 added hover/pressed/disabled to icon buttons, segments, chips, menu, rating, selbar, inputs, cards, tiles, plus the sliding segment indicator and row enter (`68b23e3`). An external UI taste audit gave 12 findings, all fixed in `2b8de12` (list-link states, 8% Now-playing tint, stat lift only when linked, off-token colors moved into tokens/recipes, mono dates/counts) except the low items in G-44. Re-audit clean. Contrast: the final re-audit found the old ≥ 4.5 test didn't back the 7:1 claim (muted reached only 6.2:1 on hover fills). Muted is now `#B6BBC3`, and `tokens.test.ts` "body text contrast ≥ 7:1" checks text, text-2 and muted on every bg/rail/surface token. Journey M4 guards selected-segment legibility on hover.)*
- [x] `python scripts/check.py` and `node scripts/journeys.mjs` pass clean with 0 warnings or errors. *(23 Oct 2026: ALL CHECKS PASS; journeys 31/31 steps, 66 keyboard stops with focus rings)*

## M9 — Interactive UI refinement (taste skill, motion)

**Goal:** refine every screen with purposeful, highly interactive motion at `MOTION_INTENSITY 7` (DESIGN §2–3), screen by screen, without changing the contract, Rust or scope. Plan: [plans/M9-motion-ui.md](plans/M9-motion-ui.md).

### Work items
1. **Settings (SC-13):** tabs (one section at a time, `?tab=`), toggle switches (optimistic, roll back on failure), pointer spotlight, count-up metrics, animated Wi-Fi disclosure, copy pairing code. *(done 24 Oct 2026)*
2. **App-wide controls:** switches for on/off settings, spring check-mark checkboxes, spring segment indicators. *(done 24 Oct 2026)*
3. **Home / Now playing, Backlog, Insights, Playlist Studio, Item:** one screen per step, scoped recipes, before/after screenshot diff.
4. **Review:** `spec-reviewer` against DESIGN §2–3 and accessibility; findings fixed.

### Acceptance
- [ ] Every screen refined; each new motion listed in DESIGN §3 with its reason. *(Settings done 24 Oct 2026)*
- [ ] Everything instant under `prefers-reduced-motion` (journey M1), focus rings and accessible names intact (M2, M3).
- [ ] No side effects: unrelated screens' journey screenshots differ by timing noise only. *(Settings: checked 24 Oct 2026)*
- [ ] `python scripts/check.py` and `node scripts/journeys.mjs` pass.
