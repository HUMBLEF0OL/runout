# Plan: M4 Backend — listening and insights

Specs: [MILESTONES M4](../MILESTONES.md#m4--backend-listening-and-insights) · [API.md §4](../API.md#4-r2-commands-backend-m4--switched-in-m7) · TSD §3 (R2 tables), §4.4–4.9, §6 · BRD FR-026–034, NFR-04, 17 · KB [spotify.md](../kb/spotify.md) (export), [windows-tauri.md](../kb/windows-tauri.md) (SMTC).
Agents: `rust-core`; `spotify-researcher` for T1/O2 (step 4).

**Entry:** M3 done. UI is already built (M2): implement [API.md](../API.md) as the UI expects it; no page changes (see [plans README](README.md#rules-for-backend-milestones-m3m5)). **U-2** (export files arrived) needed only for step 4; everything else runs on synthetic fixtures meanwhile.

**Known gaps owned here:** G-1, G-7, G-8. See [MILESTONES Known gaps](../MILESTONES.md#known-gaps); remove each row when fixed.

## Steps
1. **SMTC spike, then logger (WI 1).** Spike binary in `src-tauri/examples/smtc_spike.rs`: subscribe via `windows` crate, print 3 real Spotify tracks with accumulated `ms_played` (pause excluded). Then `logger/`:
   - `session.rs` — session manager subscription, allow-list on `SourceAppUserModelId` (default `Spotify`).
   - `accumulator.rs` — pure state machine: `Playing/Paused/Stopped` transitions + timestamps → `ms_played`; seeks add nothing; close on track change or session end; drop events < 1000 ms.
   - `poll.rs` — 5 s timeline fallback.
   - Pause logging (until resumed / 1 h), autostart via `tauri-plugin-autostart`, tray tooltip states, `logger-status` + `play-logged` events.
   - Tests: accumulator table (play→pause→play, seek, track change mid-pause, < 1 s noise, 29.9 s vs 30 s play rule).
2. **Migration (WI 2).** `0003_listening.sql`: `listening_event` (generated `counts_as_play`, `dedupe_hash UNIQUE`, partial unique index `one_to_one_reconcile`), `import_batch`, `exported_playlist*`. Test: v2 DB with data upgrades to v3 intact.
3. **Importer skeleton (WI 3).** `importer/`: `source.rs` (folder or zip, file-type detection by name pattern), `preview.rs` (counts only, no writes), `run.rs` (one transaction, `import_batch` with checksum → re-importing the same files adds 0).
4. **Real format (WI 4, T1).** After U-2: `verify-assumption` on file and field names; adjust `streaming.rs` field allow-list; record in KB + TSD §4.8. Build `tests/fixtures/export_synthetic/` from the **structure only** (random values).
5. **Minimization + transforms (WI 5).** Parse into an explicit struct (serde with unknown fields ignored); drop any `ip_addr*`, `user_agent*`, `username`; skip null `spotify_track_uri`; UTC → local with Windows time-zone rules at that date (`windows` crate `SystemTimeToTzSpecificLocalTimeEx` or `chrono-tz` with the mapped zone — pick in spike, record in KB); `private_session = incognito_mode`.
   - Tests: DST boundary conversion; a row containing `ip_addr_decrypted` never reaches the DB (query check over every TEXT column).
6. **Dedupe, reconcile, match (WI 6).** `dedupe_hash = sha256(ts|uri|ms_played)` insert-or-ignore. `normalize.rs` (TSD §4.6 regex list). `reconcile.rs` (TSD §4.7: same `norm(track)`, |Δt| ≤ 120 s, artist overlap then smallest gap, one-to-one). `matching.rs` (uri first, then track name + artist tie-break; else unmatched). `link_play` writes the manual link.
   - Tests: overlap fixture with featured-artist, compilation, back-to-back repeat → no double counts; matching table.
7. **Account data (WI 7).** `playlists.rs`: only `Playlist*.json` → `exported_playlist*`, matched across exports by name; `YourLibrary.json` ignored. Check-up queries: duplicates within playlist, low-rated (≤ 2★), never played.
8. **Insights (WI 8).** `insights/` as SQL views + small Rust assemblers per API.md §4; every response is `Sourced<T>`. Base view excludes reconciled logger events and private sessions unless the setting is on. Unfinished albums only where linked `on_album` tracks exist. Year in review per chosen year. Module has no handle to the cache DB (constructor takes only the library connection).
   - Tests: 3 insights checked against hand-computed fixture totals; year in review total equals a manual sum.
9. **Generators (WI 9).** `#[ignore]` perf: 500k events + 10-year synthetic export; assert import < 60 s, each insight query < 200 ms (add indexes where needed, note them in TSD §3).
10. **Commands + record.** Register API.md §4 commands; SOURCE_MAP rows → `built`; KB facts (export format, SMTC behaviour); `close-milestone M4`.

## Parallelism
1 runs alongside 2→3→5→6→7→8. Step 4 slots in whenever U-2 lands; until then the synthetic fixture follows TSD §4.8’s expected fields. 9 after 8.

## Rules
- No IP, user agent or username in `library.db`, ever (acceptance query check).
- Insights never read `spotify_cache.db`; a grep check in `check.py` fails on `spotify_cache` under `insights/`.
- Fixtures carry structure only; never commit real export values (CLAUDE.md).

## Risks
| Risk | Response |
|---|---|
| U-2 late (export can take up to 30 days) | Build on synthetic fixture; WI 4 is the only blocked item; keep other milestones moving |
| SMTC misses events for Spotify | 5 s poll fallback; spike decides before building the full logger |
| Matching ambiguity high on real data | Unmatched list + manual link; spot-check 20 (FR-030) |
| Time-zone history lookups slow on 500k rows | Cache offset per (zone, date) in a hash map during import |

## Done when
MILESTONES M4 acceptance passes; `python scripts/check.py` exits 0; 24 h logger run within NFR-17 recorded.
