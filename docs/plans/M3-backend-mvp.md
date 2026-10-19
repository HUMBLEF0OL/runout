# Plan: M3 Backend — MVP core

Specs: [MILESTONES M3](../MILESTONES.md#m3--backend-mvp-core) · [API.md §3](../API.md#3-mvp-commands-backend-m3--switched-in-m6) · TSD §2, §3, §4.1–4.3, §4.10–4.12, §5.1, §6, §7, §8 · BRD FR-001–025, FR-044–045, NFR-04, 05, 07, 18.
Agent: `rust-core` for everything; `spec-reviewer` after steps 4, 7 and 11.

**Entry:** M2 done (API.md stable, `src/mocks/` passing). No hard stops in M3. UI is already built (M2): implement [API.md](../API.md) as the UI expects it; no page changes (see [plans README](README.md#rules-for-backend-milestones-m3m5)).

**Known gaps owned here:** G-2, G-3, G-4, G-5 (decide before step 1), G-10, plus the M2 contract additions listed in the MILESTONES Status block. See [MILESTONES Known gaps](../MILESTONES.md#known-gaps); remove each row when fixed.

## Steps
1. **Skeleton + DB (WI 1).** Crates: `rusqlite` (bundled, FTS5), `serde`/`serde_json`, `thiserror`, `chrono`, `sha2`, `zip`, `reqwest` (rustls, blocking off), ~~`tiny_http`~~ (replaced by `std::net::TcpListener`, G-17, TSD §2), `rand`, `tauri-plugin-dialog`. Modules per TSD §9. `db/`: open with WAL + `foreign_keys=ON`, migration runner over embedded `migrations/*.sql` using `PRAGMA user_version`. `0001_init.sql` = TSD §3 core tables + `item_fts` (+ triggers) + `usage_event`. `AppError` per API.md §1 with `From<rusqlite::Error>` etc.
   - Tests: fresh DB migrates to v1; running twice is a no-op; CHECK constraints reject a bad status and rating 0.7.
2. **Links (WI 2).** `links.rs`: TSD §4.1 table as a pure `parse(&str) -> Parsed`; `resolve_short(url)` via `net` (max 5 redirects, 5 s). Offline → row in `unresolved_capture`; `retry_unresolved` on `online-changed`.
   - Tests: ≥ 15 table cases (intl prefix, `?si=`, URI form, 21/23-char IDs, episode/show/audiobook rejected, playlist rejected as item, whitespace, `http://`, garbage). Reuse the M2 mock table so both sides agree.
3. **Net (WI 3).** `net.rs`: one `reqwest::Client`; allow-list `open.spotify.com, spotify.link, api.spotify.com, accounts.spotify.com` checked before every request; `online()` HEAD cached 30 s; emits `online-changed`.
   - Tests: request to a non-listed host returns `validation` without a network call.
4. **Library (WI 4).** `library/`: `items.rs` (CRUD, duplicate → `duplicate{existing_id}`), `links.rs` (`item_link`), `status.rs` (history, `explored_at`), `tags.rs` (rename, merge keeps each item tagged once, delete), `backlog.rs` (reorder, remove without delete), `filters.rs` (saved filters, `ItemFilter` → SQL), `search.rs` (FTS5 incl. review, notes, tags, linked artist names; snippet), `provenance.rs` (TSD §4.2: promoted vs user per field; name-needed items).
   - Tests: promotion rule (prefill confirmed → `promoted`; user edit → `user`), duplicate block, tag merge, status history order, search finds a word that appears only in a review.
5. **Rediscovery + dashboard (WI 5).** `get_rediscover`, `get_dashboard`, `get_counts` as single SQL queries. Stale backlog = planned and untouched ≥ 60 days (constant in one place).
6. **Capture listener (WI 6).** `capture.rs` per TSD §5.1: off by default, bind `0.0.0.0:47821`, 6-digit code, private-IP check, ≤ 2 KB body, 10/min rate limit, 5 wrong codes → disable + `capture-disabled`. Accepted link → name-needed item (+ `capture` usage event with duration) and `capture-received`.
   - Tests: each NFR-18 rejection in the M3 acceptance list (drive the handler fn directly; one real-socket smoke test).
7. **Export (WI 7).** `export_library`: JSON `{format_version, exported_at, items[{…, provenance}], tags, links, history, saved_filters}`; excludes `usage_event`. Write `docs/EXPORT_FORMAT.md` from an actual export of the 10k fixture.
8. **Backup (WI 8).** `backup.rs` per TSD §4.10: `VACUUM INTO` → zip + `manifest.json` → rotate 3 → size warning; weekly scheduler thread (checks last backup at start + daily); restore = verify sha256 + `integrity_check` → counts → swap, keep `library.db.pre-restore`.
   - Tests: rotation ×4 leaves newest 3; corrupted zip → `integrity` error and library untouched.
9. **Delete + uninstall cleanup (WI 9).** `delete_item`, `delete_library(confirm)`; `main.rs --uninstall-cleanup` deletes `spotify_cache.db`, the `Runout/spotify` credential and `EBWebView` (paths from Tauri path API; missing files are fine). Update-mode skip is wired in M6 (T2).
10. **Usage counters (WI 10).** `app_open` at start, `record_page_visit`, capture durations; `get_success_metrics` queries per BRD §15.
11. **Commands (WI 11).** `commands.rs` registers every API.md §3 command; thin wrappers only. Add a `check.py` step: command names in `commands.rs` ⊇ API.md §3 names, and `api.ts` names match (API.md §7).
12. **Performance + integrity (WI 12).** `tests/perf.rs` `#[ignore]` generator: 10k items, 30k tags/links; assert search < 200 ms, list page < 100 ms. Integrity: child-process test that is killed mid-write and mid-backup, then `integrity_check` passes (NFR-05).
13. **Record.** SOURCE_MAP rows → `built`; KB gotchas; TSD updated where reality differed. Then `close-milestone M3`.

## Parallelism
1 first. Then 2+3 together, 4 after 1, 5 after 4, 6 after 2, 7–10 after 4 in any order, 11 last, 12 after 11.

## Rules
- Nothing from oEmbed is persisted except a confirmed title (TSD §4.3); `prefill_title` returns and forgets.
- No `println!` of notes, reviews, tokens or links in logs (check.py secret scan).
- Fixtures synthetic only (`src-tauri/tests/fixtures/`).

## Risks
| Risk | Response |
|---|---|
| FTS5 over linked artist names needs denormalised content | Maintain via triggers on `item`, `item_tag`, `item_link`; test rename propagates |
| Kill-test flakiness on Windows | Retry kill at 3 offsets; assert integrity each time |
| Listener blocks shutdown | Non-blocking `std::net::TcpListener` polled with a stop flag (replaced `tiny_http`, G-17) |

## Done when
MILESTONES M3 acceptance passes; `python scripts/check.py` exits 0; `docs/EXPORT_FORMAT.md` exists.
