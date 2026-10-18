# Milestone Specifications — Runout

Delivery order (BRD D30): **design → UI on mock data → backend → integration**, as strict phases: the backend phase (M3–M5) starts only when M2 is done; the integration phase (M6–M7) starts only when M3, M4 and M5 are all done. See [plans/README.md](plans/README.md).

Spec-driven rule: before a milestone starts, its specs are current (BRD, [TSD](TSD.md), [DESIGN](DESIGN.md), [CAPABILITIES](CAPABILITIES.md)) and its plan exists at `docs/plans/M<n>-<slug>.md` (M1–M7 written 18 Oct 2026). The UI ↔ Rust command surface is fixed in [API.md](API.md). A milestone is **done** only when every acceptance check passes. `U-n` = a hard stop that needs you ([USER_ACTIONS.md](USER_ACTIONS.md)). `CAP-n` = [capability](CAPABILITIES.md). `SC-n` = [screen](DESIGN.md#6-screen-inventory-m1-designs-all-of-these).

| Milestone | Phase | Releases | Capabilities | Your hard stops | Effort |
|---|---|---|---|---|---|
| M0 Foundations | setup | — | — | U-1 | done |
| M1 Design | design | MVP, R2, R3 | all (SC-01–13) | U-9 | ~1 week |
| M2 UI build | UI | MVP, R2, R3 | all, on mock data | — | 3–4 weeks |
| M3 Backend: MVP core | backend | MVP | CAP-01–17, 35 | — | 3–4 weeks |
| M4 Backend: listening and insights | backend | R2 | CAP-18–27 | U-2 | 3–4 weeks |
| M5 Backend: Spotify connected | backend | R3 | CAP-28–34 | U-6, U-7, U-8 (opt.) | 3–4 weeks |
| M6 Integration: MVP release | integration | MVP | CAP-01–17, 35 | U-3, U-4, U-5 | 1–2 weeks + 2 weeks daily use |
| M7 Integration: R2 and R3 | integration | R2, R3 | CAP-18–34 | — | ~2 weeks |
| M8 Future candidates | — | — | per item | per item | as needed |

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
- [ ] **U-9** designs approved.

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
- [ ] Every SC-n matches its approved board (side-by-side review).
- [ ] Journeys A–H clickable end to end on mock data.
- [ ] `src/` contains no `fetch`, SQLite or network code; only `api.ts` and embed iframes (TSD §1 rule).
- [ ] T4 resolved: embeds work in incognito web view and leave no HTTP cache in `EBWebView`.
- [ ] All text passes 4.5:1 contrast (NFR-12); every action reachable by keyboard.
- [ ] `python scripts/check.py` exits 0.

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
