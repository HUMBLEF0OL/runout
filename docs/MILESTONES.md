# Milestone Specifications — Runout

Delivery order (BRD D30): **design → UI on mock data → backend → integration**, as strict phases: the backend phase (M3–M5) starts only when M2 is done; the integration phase (M6–M7) starts only when M3, M4 and M5 are all done. See [plans/README.md](plans/README.md).

Spec-driven rule: before a milestone starts, its specs are current (BRD, [TSD](TSD.md), [DESIGN](DESIGN.md), [CAPABILITIES](CAPABILITIES.md)) and its plan exists at `docs/plans/M<n>-<slug>.md` (M1–M7 written 18 Oct 2026). The UI ↔ Rust command surface is fixed in [API.md](API.md). A milestone is **done** only when every acceptance check passes. `U-n` = a hard stop that needs you ([USER_ACTIONS.md](USER_ACTIONS.md)). `CAP-n` = [capability](CAPABILITIES.md). `SC-n` = [screen](DESIGN.md#6-screen-inventory-m1-designs-all-of-these).

| Milestone | Phase | Releases | Capabilities | Your hard stops | Effort |
|---|---|---|---|---|---|
| M0 Foundations | setup | — | — | U-1 | done |
| M1 Design | design | MVP, R2, R3 | all (SC-01–13) | U-9 | approved 18 Oct 2026 |
| M2 UI build | UI | MVP, R2, R3 | all, on mock data | — | done 18 Oct 2026 |
| M3 Backend: MVP core | backend | MVP | CAP-01–17, 35 | — | 3–4 weeks |
| M4 Backend: listening and insights | backend | R2 | CAP-18–27 | U-2 | 3–4 weeks |
| M5 Backend: Spotify connected | backend | R3 | CAP-28–34 | U-6, U-7, U-8 (opt.) | 3–4 weeks |
| M6 Integration: MVP release | integration | MVP | CAP-01–17, 35 | U-3, U-4, U-5 | 1–2 weeks + 2 weeks daily use |
| M7 Integration: R2 and R3 | integration | R2, R3 | CAP-18–34 | — | ~2 weeks |
| M8 Future candidates | — | — | per item | per item | as needed |

### Status (18 Oct 2026)
- **Phase:** backend phase, M3 in progress. Branch `dev` → merge to `master`; commits only, no remote.
- **M0** done. **M1** designs approved (U-9, 18 Oct 2026); its other acceptance lines were never ticked on the boards themselves, and M2 met them in code (states per screen, `tokens.test.ts` contrast, compliance review with no open findings).
- **M2** done and closed: all 13 screens + design-system page on mock data, Tailwind v4, `src/api.ts` = full API.md contract behind mocks, input sanitization, T4 verified in the real window. Proof: `python scripts/check.py` (92 Vitest tests) and `node scripts/journeys.mjs` (30/30 keyboard-only steps, journeys A–H). Commits `9cd6c2e`, `df6b155`, `fc12844`.
- **Contract additions made in M2** (backend must implement): `create_playlist_from_items`; dashboard tile fields; `Unmatched.candidates`; `CollectionInsights` stat fields; `NewItem.artist_name/notes/promoted_fields`; `ItemPatch.artist_name`; `ItemSummary.album_name/added_via/recommended_by`; `ItemDetail.track_list` (all in API.md).
- **Known gaps:** see [Known gaps](#known-gaps) below (G-2 to G-5 closed in M3).
- **Your open hard stops:** U-1 (request Spotify exports) not recorded as done; U-2 needed by M4; U-6/U-7 by M5; U-3/U-4/U-5 by M6.
- **M3** steps 1–12 built and through three external spec reviews (all findings fixed); 43 §3 commands registered and checked by `check.py`.
- **Next:** G-17 fix (std listener), then step 13 (record) and `close-milestone M3`.

### Known gaps
Found while building and closing M2 (18 Oct 2026). None blocks a release criterion. "Contract" = needs an API.md change first (API.md §7), then `api.ts`, the mock and Rust together. Remove a row when it's fixed; say in the commit which G-n it closes.

| # | Gap | Where | Why it's open | Fix in |
|---|---|---|---|---|
| G-1 | Album page "Tracks you've added" has no Played / Not played column | `src/pages/Item.tsx` | `ItemDetail.track_list` has no per-track play data (contract) | M4 (listening data) |
| G-6 | Rediscover "New shelf" saves a saved filter (chip) rather than a separate shelf type | `src/pages/Rediscover.tsx` | No shelf entity in the contract; saved filter covers it for now | M8 candidate |
| G-7 | Recent plays: no "album" suffix next to an album's rating (board shows it) | `src/pages/Plays.tsx` | `Play.item` is the track; album rating not in the payload (contract) | M4 |
| G-8 | Insights "Unfinished albums" lacks the "N other albums can't be checked" count | `src/pages/Insights.tsx` | `TakeInsights` has no count of albums without a track list (contract) | M4 |
| G-9 | Playlist Studio `conflict` error ("playlist changed on Spotify, reload draft") never seen on screen | `src/pages/Playlists.tsx` | The mock never returns `conflict`; add a dev-switcher trigger or test it against the real API | M5 |
| G-10 | Quick add prefills a title only for the sample links | `src/mocks/index.ts` | Mock oEmbed is a fixed table; real `prefill_title` comes with the backend | M3 |
| G-11 | Embeds in mock data show Spotify's "page not found" | mock data | Mock Spotify IDs are random by rule (no real IDs committed); a real pasted link renders fine | won't fix (by design) |
| G-12 | Embeds play 30-second previews, not full tracks | WebView2 InPrivate | Incognito keeps no Spotify login (T4, `docs/kb/windows-tauri.md`); accepted trade-off for no disk cache | accepted |
| G-13 | Digits 1–5 don't set status while focus is on the rating slider | `src/hooks/useShortcuts.ts` | Arrow keys own the slider; digits there would be ambiguous | by design |
| G-14 | Tray menu and uninstall dialog have no React screens | native (Rust tray, NSIS) | They aren't WebView UI (DESIGN §6 reference only) | M6 |
| G-15 | Spotify credential must be created with target exactly `Runout/spotify` (`keyring` `Entry::new_with_target`) or `--uninstall-cleanup` won't find it | `src-tauri/src/spotify/`, `cleanup.rs` | Credential is written in M5; cleanup (M3) deletes by that target ([KB](kb/windows-tauri.md)) | M5 |
| G-16 | Success metric "moved after 14 days" can't see items that left the backlog while planned | `src-tauri/src/usage.rs` | Nothing records when an item entered or left the backlog; exact needs a backlog-history migration | M8 candidate |

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
- [ ] `cargo test` green, incl. ≥ 15 link-parser cases, promotion rule, duplicate block, tag merge, backup rotation ×4.
- [ ] Every `api.ts` MVP call has a Rust command with matching types.
- [ ] NFR-04 on 10k items: search < 200 ms; search finds a word that appears only in a review.
- [ ] All data persists across restart; merging two tags keeps every item tagged once.
- [ ] Kill during a large write and during backup → library passes integrity check (NFR-05).
- [ ] Listener: wrong code ×5 disables it; non-LAN request rejected; 11th request in a minute rejected; body > 2 KB rejected; non-Spotify link rejected (NFR-18).
- [ ] Backup size recorded against the NFR-11 estimate; restore of the oldest verifies counts.
- [ ] `docs/EXPORT_FORMAT.md` exists and matches an actual export file.

## M4 — Backend: listening and insights

**Goal:** a complete, de-duplicated listening history and the insight queries over it.

Work items
1. SMTC watcher spike, then `logger/` (TSD §4.5): allow-list, accumulation, 5 s fallback poll, autostart option, **Pause logging**.
2. `0002_listening.sql`.
3. `importer/`: folder/zip → detect file types → preview counts → import in one transaction.
4. Verify real export format (**T1**, needs **U-2**) and adjust parser; anonymized fixture from its *structure* only.
5. Minimization (drop IP/user agent/username), skip non-music rows, UTC→local, private-session flag.
6. Dedupe hash; one-to-one reconciliation (TSD §4.7); matching to items (TSD §4.6) and the unmatched list.
7. Account-data import (playlists only) and read-only playlist check-up queries.
8. `insights/` SQL views (TSD §4.9) with sources and freshness; unfinished albums only for albums with a track list; year in review; private-session setting.
9. 500k-event + 10-year synthetic export generator.

Acceptance
- [ ] SMTC spike logs 3 consecutive real Spotify tracks with plausible `ms_played` (pause excluded).
- [ ] Real lifetime history imports; second import adds 0 rows.
- [ ] No IP/user agent/username anywhere in `library.db` (query check).
- [ ] Overlap test: no double counts, incl. featured-artist, compilation and back-to-back repeat fixtures.
- [ ] Matching: spot-check 20 matched plays, all correct (FR-030).
- [ ] NFR-04: 10-year import < 60 s; queries over 500k events < 200 ms.
- [ ] Logger runs 24 h within NFR-17; paused time logs nothing.
- [ ] Three insights hand-verified against raw data; year in review matches a manual total (FR-034).
- [ ] Code check: `insights/` has no reference to the Spotify cache DB.

## M5 — Backend: Spotify connected

**Goal:** the Spotify Web API layer, safe and compliant.

Gate (before any R3 code): **U-6** developer app created → one API call → confirms the Student Premium plan works (**O1**). If it fails, R3 stops and the decision returns to you. Optional **U-8** (O7).

Work items
1. Re-check **T3**; PKCE login with dynamic loopback port (**U-7** first login); Credential Manager incl. refresh-token rotation; status; reconnect on `invalid_grant`; disconnect deletes `spotify_cache.db`.
2. `spotify_cache.db` (cache entries + playlist snapshots); 7-day cache / 30-day snapshot sweeper (O4, O5).
3. Playlist Studio logic: list own/collab playlists → import → duplicates within/across → draft → diff → re-read `snapshot_id` and abort on mismatch → apply → 30-day undo snapshot. Ratings and tags only, no listening data (BRD 11.1, D27).
4. Playlist from saved filter.
5. Autofill (cache → confirm → promote); album track bulk-add.
6. Top items and recent plays commands (display only; add-to-backlog via promotion rule).
7. Save to / remove from Spotify library.

Acceptance
- [ ] O1 resolved.
- [ ] Playlist round-trip on a **test playlist**: cut 3, dedupe 1, apply, verify in Spotify, undo, verify restored.
- [ ] Change the playlist in Spotify after drafting → apply blocked (pre-apply check).
- [ ] Code check: Playlist Studio never queries `listening_event` or `exported_playlist*`.
- [ ] Refresh that returns a new refresh token stores it; revoked token → reconnect state.
- [ ] Disconnect → cache DB (incl. snapshots) gone immediately; backups contain no Spotify-provided data.

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
