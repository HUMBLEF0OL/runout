# Milestone Specifications — Runout

Delivery order (BRD D30): **design → UI on mock data → backend → integration**, as strict phases: the backend phase (M3–M5) starts only when M2 is done; the integration phase (M6–M7) starts only when M3, M4 and M5 are all done. See [plans/README.md](plans/README.md).

Spec-driven rule: before a milestone starts, its specs are current (BRD, [TSD](TSD.md), [DESIGN](DESIGN.md), [CAPABILITIES](CAPABILITIES.md)) and its plan exists at `docs/plans/M<n>-<slug>.md` (M1–M7 written 18 Oct 2026). The UI ↔ Rust command surface is fixed in [API.md](API.md). Efforts assume agentic development (BRD D31): build-and-review hours, plus real-world waits agents can't shorten. A milestone is **done** only when every acceptance check passes. `U-n` = a hard stop that needs you ([USER_ACTIONS.md](USER_ACTIONS.md)). `CAP-n` = [capability](CAPABILITIES.md). `SC-n` = [screen](DESIGN.md#6-screen-inventory-m1-designs-all-of-these).

| Milestone | Phase | Releases | Capabilities | Your hard stops | Effort (agentic, BRD D31) |
|---|---|---|---|---|---|
| M0 Foundations | setup | — | — | U-1 | done |
| M1 Design | design | MVP, R2, R3 | all (SC-01–13) | U-9 | approved 18 Oct 2026 |
| M2 UI build | UI | MVP, R2, R3 | all, on mock data | — | done 18 Oct 2026 |
| M3 Backend: MVP core | backend | MVP | CAP-01–17, 35 | — | done 18 Oct 2026 (about 3 h; was estimated 3–4 weeks) |
| M4 Backend: listening and insights | backend | R2 | CAP-18–27 | U-2 | 4–6 h + export download (U-2) + 24 h logger run |
| M5 Backend: Spotify connected | backend | R3 | CAP-28–34 | U-6, U-7, U-8 (opt.) | done 20 Oct 2026 (build ~4 h + live Studio check; was estimated 4–6 h after the O1 gate) |
| M6 Integration: MVP release | integration | MVP | CAP-01–17, 35 | U-3, U-4, U-5 | 4–6 h + 2 weeks daily use (U-5) |
| M7 Integration: R2 and R3 | integration | R2, R3 | CAP-18–34 | — | 3–4 h |
| M8 Future candidates | — | — | per item | per item | as needed |

### Status (20 Oct 2026)
- **Phase:** M4 remains the formal backend gate; code-completable M6 preparation is underway as an explicit exception, not a closed integration phase. M6 WI 1 (MVP transport/events) and WI 3 (NSIS hook/config) are implemented. M6 audit (20 Oct 2026) fixed: the production build ran on mocks (now `.env.production` `VITE_MOCK=0`), R2/R3 mock screens and the mock "today" leaked into real mode (real mode now shows MVP only), the uninstall hook ran before the app was closed, and the custom NSIS English table was incomplete; new check.py `release build` guards all three. Installed-build runs (uninstall cases, offline, network, NFR-04) and U-3/U-4/U-5 evidence still open; MVP mock removal deferred (G-34). Branch `dev` → merge to `master`; commits only, no remote.
- **M0** done. **M1** designs approved (U-9, 18 Oct 2026); its other acceptance lines were never ticked on the boards themselves, and M2 met them in code (states per screen, `tokens.test.ts` contrast, compliance review with no open findings).
- **M2** done and closed: all 13 screens + design-system page on mock data, Tailwind v4, `src/api.ts` = full API.md contract behind mocks, input sanitization, T4 verified in the real window. Proof: `python scripts/check.py` (92 Vitest tests) and `node scripts/journeys.mjs` (30/30 keyboard-only steps, journeys A–H). Commits `9cd6c2e`, `df6b155`, `fc12844`.
- **Contract additions made in M2** (backend must implement): `create_playlist_from_items`; dashboard tile fields; `Unmatched.candidates`; `CollectionInsights` stat fields; `NewItem.artist_name/notes/promoted_fields`; `ItemPatch.artist_name`; `ItemSummary.album_name/added_via/recommended_by`; `ItemDetail.track_list` (all in API.md).
- **Known gaps:** see [Known gaps](#known-gaps) below (G-2 to G-5 closed in M3; G-1, G-7, G-8 filled in Rust in M4).
- **Your open hard stops:** U-1/U-2 done 19 Oct 2026 (Extended Streaming History only; Account data export not yet received, needed for M4 WI 7); U-6/U-7 done (live Studio check passed 20 Oct 2026); U-3/U-4/U-5 by M6.
- **M3 closed** 18 Oct 2026: all 8 acceptance lines ticked with evidence; four external spec reviews, every finding fixed. Contract changes in M3: `ItemSummary.notes`, `NewItem.album_id`, `ItemPatch.type`, `search` `type`, `recent_activity` dropped.
- **M3 post-close audit** 19 Oct 2026: 3 rounds of parallel spec reviews + 2 verification passes, 36 findings fixed (FTS secure-delete migration `0002`, restore/move safety copies, capture off mid-request, link-retry races, settings load, input clamps); check.py + journeys 30/30 green.
- **M4 in progress** 19 Oct 2026: logger, importer and insights merged on `dev` (`d0ea598`), check.py green; WI 9 perf `4ad5a14`; external audit fixes `4f53129`. Acceptance: 4/9 ticked with evidence (minimization query, overlap, hand-verified insights + year review, insights-no-cache). G-17 closed (all-time reads via 0005 rollup). BRD O2 partly resolved (S14: export took ~1 day; frequency open). Open: SMTC 3 live tracks, real lifetime import + re-import 0, 20-play spot-check, 24 h logger run.
- **M5 closed** 20 Oct 2026: acceptance 6/6 with evidence (2 lines live on the user's test playlist); WI 1–7 done; external audit: 3 rounds (19 Oct), r4, redesign + 3 rounds, r5 (worktree fixers), final 3-lens round (10 confirmed, fixed in `88392fc`, diff re-audit clean); check.py green, journeys 30/30. BRD O1, O4, O5 resolved. Deferred with owners: G-22, G-23, G-32 (M7), G-24 (M7), G-31, G-33 (M8). History below.
- **M5 code done** 19 Oct 2026: WI 1–7 built on `dev` (`src-tauri/src/spotify/`, stub-tested, never the real API); 3 external audit rounds, 40 confirmed findings fixed (single-flight refresh, disconnect/revoke purge the cache, reconnect marker survives restart, Studio lock, undo conflict check + unclear-write handling, one undo per playlist); check.py green. Acceptance 4/6 ticked (O1, code check, refresh rotation/reconnect, disconnect). G-24 (duplicates across playlists) deferred to M7. **Live run 20 Oct 2026** (test playlist, `studio_roundtrip`): the write landed but `GET /playlists/{id}?fields=snapshot_id` stayed stale 30 s+, so undo hit a false `conflict`; apply/undo now compare track URIs, never `snapshot_id` (`f27081e`, `efacba1`, audit r2). Items-read freshness verified live (G-30 closed). Audit r3: a failed apply/undo now reloads the draft and undo record (since r5 only after Studio's own apply/undo, and not when the error means nothing was written), undo-specific conflict copy (G-25 closed), undo waits 2 min (`spotify_busy`) before trusting a read of the list from before an unanswered write, `--probe` reads via `studio::current_uris`.
- **M5 live acceptance passed** 20 Oct 2026 on the user's test playlist (`studio_roundtrip`): round trip (cut 3, dedupe 1, apply, undo → `identical to the original: true`) and `--conflict` (apply blocked once the Web API showed the edit, ~9 s after it was made in the Spotify app). `--probe` reorder: `GET /playlists/{id}/items` showed the move at once; every `snapshot_id` read lagged ~33–36 s. Acceptance 6/6.
- **Next:** M4: real import + spot-check (reading the export needs your OK) → live SMTC run (3 tracks, pause, repeat: G-21) and 24 h logger run → `close-milestone`. M5 closed (see above); after M4 closes, the integration phase starts with M6 WI 1 (switch MVP calls in `api.ts` to Rust).

### Known gaps
Found while building and closing M2 (18 Oct 2026). None blocks a release criterion. "Contract" = needs an API.md change first (API.md §7), then `api.ts`, the mock and Rust together. Remove a row when it's fixed; say in the commit which G-n it closes.

| # | Gap | Where | Why it's open | Fix in |
|---|---|---|---|---|
| G-6 | Rediscover "New shelf" saves a saved filter (chip) rather than a separate shelf type | `src/pages/Rediscover.tsx` | No shelf entity in the contract; saved filter covers it for now | M8 candidate |
| G-10 | Quick add prefills a title only for the sample links | `src/mocks/index.ts` | Mock oEmbed is a fixed table; Rust `prefill_title` (oEmbed, M3) is built, check it with a real link when Quick add switches to it | M6 |
| G-11 | Embeds in mock data show Spotify's "page not found" | mock data | Mock Spotify IDs are random by rule (no real IDs committed); a real pasted link renders fine | won't fix (by design) |
| G-12 | Embeds play 30-second previews, not full tracks | WebView2 InPrivate | Incognito keeps no Spotify login (T4, `docs/kb/windows-tauri.md`); accepted trade-off for no disk cache | accepted |
| G-13 | Digits 1–5 don't set status while focus is on the rating slider | `src/hooks/useShortcuts.ts` | Arrow keys own the slider; digits there would be ambiguous | by design |
| G-14 | Tray menu and uninstall dialog have no React screens | native (Rust tray, NSIS) | They aren't WebView UI (DESIGN §6 reference only) | M6 |
| G-16 | Success metric "moved after 14 days" can't see items that left the backlog while planned | `src-tauri/src/usage.rs` | Nothing records when an item entered or left the backlog; exact needs a backlog-history migration | M8 candidate |
| G-21 | Logger replay detection relies on Spotify moving the SMTC timeline position on repeat; if it doesn't, a repeat is one long play (old behaviour) | `src-tauri/src/logger/accumulator.rs` | Not yet exercised in the running app | M4 (SMTC live check) |
| G-22 | Library sync sends URIs as a `?uris=` comma list with no body on `PUT/DELETE /me/library` and `GET /me/library/contains`; Spotify's expected shape isn't recorded in the KB | `src-tauri/src/spotify/library_sync.rs` (`paths()`/`write()`) | Stub-tested only; no real call yet | M7 (verify-assumption or first real call, then KB entry) |
| G-23 | Album expansion for "create playlist" calls `GET /albums/{id}/tracks`, which TSD §5.2's endpoint list doesn't name; Dev Mode availability unverified. | `src-tauri/src/spotify/create.rs`, `studio.rs` | Stub-tested only | M7 (verify, or switch to `GET /albums/{id}` + `tracks.next`; `studio_roundtrip` example confirms the field names) |
| G-24 | Studio flags duplicates within the loaded playlist only, not across the user's other playlists (BRD R3-1 "duplicates across playlists", M5 WI 3) | `src-tauri/src/spotify/studio.rs` (`dupes()`) | `dupe_of` is a track URI of the same list and the UI resolves it by position; naming another playlist needs a contract change (API.md §7) and one `items` read per playlist | M7 (spec decision on `dupe_of` shape, then implement) |
| G-26 | 20-play matching spot-check (FR-030) not done on real data | `src-tauri/examples/real_import.rs` | The real library had 0 records (UI on mocks until M6), so the real import matched nothing; re-run the example on a temp copy once the library has records and check 20 matches by hand | M7 |
| G-27 | `Playlist*.json` (Account data export) never checked against a real file; playlist import + check-up proven on the synthetic fixture only | `src-tauri/src/importer/playlists.rs`, `source.rs` | Account data export not received (U-1 note); when it arrives: verify file/field names (T1 rest), adjust parser, import into a temp copy | M7 (or when the export arrives) |
| G-28 | In-app logger never run live: the 24 h run used `examples/smtc_spike.rs` (same session + accumulator, no DB writes); DB writes, `play-logged`, tray tooltip, autostart and Pause logging from the tray unexercised in the running app | `src-tauri/src/logger/` | App UI runs on mocks until M6/M7 switch-over | M7 (with G-21) |
| G-29 | Rollup edge cases: an all-NULL artist group returns `''` where the base view returns NULL (`rank`, `list_unmatched`); the non-`all` heatmap query errors on an unparseable `ended_at_local` | `src-tauri/src/insights/` | Audit of G-17 (19 Oct 2026) rated both below the fix bar; no real data triggers them yet | M7 |
| G-31 | Playlist check-up's "Open in Studio" link matches the playlist by name (first match wins when two share a name) | `src/pages/Playlists.tsx` | The Account data export's `Playlist*.json` has no Spotify URI, so `list_exported_playlists` can't give one | M8 candidate (needs a contract change: match the export to a live playlist by name + owner + track list) |
| G-32 | Autofill, album bulk-add, playlist from filter/items, top items, recent plays and library save/remove have only run against stub HTTP responses; only sign-in and Playlist Studio ran live (20 Oct 2026) | `src-tauri/src/spotify/{autofill,create,discover,library_sync}.rs` | M5 acceptance covers Studio + auth only; the UI still calls mocks for R3 | M7 (switching R3 calls to Rust is the first real call of each; G-22/G-23 resolve there too) |
| G-33 | Apply and undo wait a fixed 2 minutes after a playlist write with an unknown outcome; a write Spotify finishes later than that could still land after Runout acts | `src-tauri/src/spotify/studio.rs` (`UNSURE_WAIT`) | No Spotify signal says a pending write is done; 2 min is far above the 20 s request timeout. Needs a timeout + 5xx on a real write to observe | M8 candidate (raise the bound or add a content re-check before undo's write if ever seen) |
| G-34 | MVP mock handlers and dataset not yet removed from `src/mocks/` (M6 WI 1); a `VITE_MOCK=0` build still bundles the mock chunk, though MVP mode never loads it | `src/mocks/`, `src/api.ts` (`loadMocks`) | Dev mode and journeys A–H still run the whole UI on mocks; R2/R3 mocks share the MVP dataset until their own switch-over | M7 (with "`src/mocks/` holds no code used by the shipped build") |

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
- [ ] Matching: spot-check 20 matched plays, all correct (FR-030). *(open, G-26: the real library has 0 records until the UI switches off mocks (M6), so the real import matched 0 plays; re-run `examples/real_import.rs` once the library has records — owner M7)*
- [x] NFR-04: 10-year import < 60 s; over 500k events filter results < 200 ms and screen navigation < 500 ms (BRD NFR-04 wording). *(met in `tests/perf_listening.rs`, release, worst of 5, 19 Oct: import 21.4 s; screen loads 7d/30d/12m/all 38/67/362/58 ms, collection ≤ 28 ms, take 433 ms, year review 139 ms, record panel 100/100 ms (< 500 ms); `list_plays` 7d/30d/all 2–6 ms, `list_unmatched` 17 ms (< 200 ms); all-time reads via the 0005 rollup, G-17. [KB](kb/perf-integrity.md))*
- [ ] Logger runs 24 h within NFR-17; paused time logs nothing. *(open: 24 h `smtc_spike` run in progress, ends 20 Oct ~19:50; pause/off writing nothing is unit-tested; the in-app logger's live run is G-28)*
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
- [ ] All BRD §13.1 MVP criteria pass.
- [ ] Journeys A–E and H pass on real data.
- [ ] Save 50 real items of mixed types (incl. 5 short links, 2 while offline that resolve later); every item opens in the Spotify app.
- [ ] Pasting an existing link is blocked and “open existing” opens it; an episode link is rejected with a clear message; two album versions linked as `version_of`.
- [ ] No oEmbed response shape (`"html":`, `thumbnail_url`) in any file under app data.
- [ ] Attribution present on every screen that shows Spotify-sourced content.
- [ ] Network capture: idle app makes no outbound connections; normal use reaches only allow-listed hosts plus embed traffic (NFR-07).
- [ ] Uninstall, box unticked: cache, tokens and `EBWebView` gone; library and backups kept. Box ticked: library also gone; backups kept. Update-mode reinstall keeps the Spotify connection.
- [ ] NFR-04 on the installed build: navigation < 500 ms, start < 3 s.
- [ ] **U-3** Shortcut share lands in backlog as “name needed”; **U-4** firewall allowed.
- [ ] **U-5** two weeks of daily use; success metrics (BRD §15) reviewed → go/no-go for M7.

## M7 — Integration: R2 and R3

**Goal:** listening, insights and Spotify screens on real data.

Work items
1. Switch R2 calls (logger status, import, plays, insights, check-up) to Rust; remove mocks.
2. Switch R3 calls (connection, Playlist Studio, playlist from filter, autofill, From Spotify, library save) to Rust; remove mocks.
3. Compliance pass on the running app (`compliance-review`).

Acceptance
- [ ] All BRD §13.2 and §13.3 criteria pass.
- [ ] Journeys F and G pass on real data.
- [ ] Every insight shows sources and freshness.
- [ ] Top items and recent plays: “Add to backlog” creates a promoted record (FR-041); save then remove an album in the Spotify library (FR-042).
- [ ] Unplug network → all MVP and R2 features work.
- [ ] `src/mocks/` holds no code used by the shipped build.

## M8 — Future candidates
Pick from BRD §4.4 after the M7 review. Each gets its own spec and plan before work starts.
