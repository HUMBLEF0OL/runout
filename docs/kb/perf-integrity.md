# Performance and integrity — measured

Format: fact — **status**, date, source. Targets: BRD NFR-04, NFR-05, NFR-11; test plan TSD §8.

## Reference run (M3 step 12)
Dev PC (Windows 11, release build, rusqlite 0.40.2 bundled), `cargo test --release -- --ignored --nocapture`. Library: `tests/common::library` — 10,000 items (1k artists, 3k albums, 6k tracks), 300 tags, 15,002 item_tags + 15,000 item_links, random-word reviews on ~half, notes on ~a third, random 22-char ids. Generation 4.9 s. Times are the worst of 5 warm runs; ranges = two full runs (the second with other builds on the PC).

- **Search < 200 ms** — **Verified**, 2026-10-18: review-only word 0.4–0.7 ms (1 hit, `field = review`); common word 5–8 ms; 2-letter prefix `ba` 20–25 ms; two-term prefix + type filter 31–68 ms; prefix + album filter 20–75 ms. All under NFR-04 (worst 75 ms); no index migration needed.
- **List page (50 rows) < 100 ms** — **Verified**, 2026-10-18: default (added_at) 0.6–1.3 ms; type + status by name 14–44 ms; tag + min_rating by rating 20–24 ms; backlog order offset 1000: 8–9 ms; modified desc 0.5–0.8 ms.
- **App-start equivalent** (cold `db::open` + migrate no-op + `get_counts` + `get_dashboard`) 67–78 ms — **Verified**, 2026-10-18. The dashboard returns all ~2k in-progress items at this size; fine at 78 ms, but cap it if a real library has thousands in progress. NFR-04 "start < 3 s" is for the installed app (M7).
- **Backup size (NFR-11)**: 10k library.db 12.9–13.1 MB → backup zip **4.1–4.2 MB** in 0.38–0.46 s — **Verified**, 2026-10-18. Below the BRD 10–30 MB "typical" estimate: text-only libraries compress ~3x. The estimate stays a safe upper bound for MVP; the size warning default (50 MB) is far off.

## Listening reference run (M4 WI 9)
Same PC, release build, `cargo test --release -- --ignored perf_listening --nocapture`. 10k library + 300k logger-style events inserted in time order (16.8 s) + a 10-year synthetic export (200k rows, 14 files, random values) = 500k events. Worst of 5 warm runs; the PC was shared with other builds, so runs varied up to 2x.
- **Import 10 years < 60 s** — **Verified**, 2026-10-19: 21.4 s with the 0005 rollup triggers (17.6 s before them; 300k direct inserts 16.8 → 21.5 s) for 200k rows (parse, match, dedupe, reconcile, one transaction). Was 35–78 s before two fixes: `cache_size` 64 MB at open (random `dedupe_hash` index inserts were ~3x slower with the 2 MB default) and a cached insert statement.
- **Screen loads < 500 ms** — **Verified** 2026-10-19 (after 0005): listening 7d/30d/12m/all 38/67/362/58 ms, collection 25–28 ms (all ranges), take 433 ms, year in review 139 ms, record panel 100 ms (busiest track) / 100 ms (its artist), freshness < 0.1 ms. Listening `all` was 3.9 s before the rollup (G-17).
- **Filter results < 200 ms** — **Verified** 2026-10-19: `list_plays` 30d tabs 2–6 ms, range `all` tabs 3–5 ms (was 295–354 ms), `list_unmatched` 17 ms (was 0.25–1.1 s).
- **G-17 fix** (0005): `listening_rollup` (one row per private flag × record × norm_key × names) and `listening_heat` (weekday × hour), kept by triggers on `listening_event`, so every writer stays in step. Range `all` reads them (~10k rows instead of 500k); `list_plays` pages walk the time index; `list_unmatched` takes groups from the rollup and each listed group's newest event id via the norm_key index. `insights::tests::rollup_equals_base_view` compares against `base` after link, unreconcile, private flip, record and event delete.
- What it took (TSD §3): `base` CTE `NOT MATERIALIZED` (used twice → SQLite materialized 500k rows); range `all` filters on `+ended_at_utc` (time-index row lookups were ~5x slower than a scan); record attribution via link pairs (`insights::SRC`) instead of per-event unions (take 44 s → 0.48 s); take in one statement; year filters bounded on utc; "new artists" via 0004 index `lower(artist_name), ended_at_utc` (857 → 53 ms).

## Integrity (NFR-05)
- `tests/integrity.rs` re-runs its own test binary as a child (`stress_child`, env-gated) and `TerminateProcess`es it. **Mid-write** (500-item transactions + FTS triggers) at 400/1300/2700 ms (500/2000/5500 items committed) and **mid-backup** (`backup_now` loop over the 10k library) at 30/250/700 ms: every time `PRAGMA integrity_check` = ok, FTS5 `integrity-check` passes, `db::open` works, and only whole batches are committed — **Verified**, 2026-10-18. Quick non-ignored variant: one mid-write kill at 300 ms (debug, < 1 s).
- A kill is a process crash, not power loss: OS file buffers still reach disk, so this does not prove durability across power cut (WAL + `synchronous=NORMAL` may lose the last commit then, never corrupt) — **Interpretation**, SQLite docs https://www.sqlite.org/pragma.html#pragma_synchronous
- Half-written backups are never listed: the zip is written as `<name>.zip.tmp` and renamed; a truncated `.zip` is skipped by `list_backups` (manifest unreadable) — **Verified**, 2026-10-18.

## Gotchas
- 2026-10-18: a killed backup left `runout-backup-*.zip.tmp` and the `.runout-backup-*.zip.db` snapshot behind forever (only the same-name snapshot was removed). `backup::rotate` now deletes those leftovers after the next good backup; asserted in `kill_mid_backup_three_offsets`.
