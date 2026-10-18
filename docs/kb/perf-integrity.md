# Performance and integrity — measured

Format: fact — **status**, date, source. Targets: BRD NFR-04, NFR-05, NFR-11; test plan TSD §8.

## Reference run (M3 step 12)
Dev PC (Windows 11, release build, rusqlite 0.40.2 bundled), `cargo test --release -- --ignored --nocapture`. Library: `tests/common::library` — 10,000 items (1k artists, 3k albums, 6k tracks), 300 tags, 15,002 item_tags + 15,000 item_links, random-word reviews on ~half, notes on ~a third, random 22-char ids. Generation 4.9 s. Times are the worst of 5 warm runs; ranges = two full runs (the second with other builds on the PC).

- **Search < 200 ms** — **Verified**, 2026-10-18: review-only word 0.4–0.7 ms (1 hit, `field = review`); common word 5–8 ms; 2-letter prefix `ba` 20–25 ms; two-term prefix + type filter 31–68 ms; prefix + album filter 20–75 ms. All under NFR-04 (worst 75 ms); no index migration needed.
- **List page (50 rows) < 100 ms** — **Verified**, 2026-10-18: default (added_at) 0.6–1.3 ms; type + status by name 14–44 ms; tag + min_rating by rating 20–24 ms; backlog order offset 1000: 8–9 ms; modified desc 0.5–0.8 ms.
- **App-start equivalent** (cold `db::open` + migrate no-op + `get_counts` + `get_dashboard`) 67–78 ms — **Verified**, 2026-10-18. The dashboard returns all ~2k in-progress items at this size; fine at 78 ms, but cap it if a real library has thousands in progress. NFR-04 "start < 3 s" is for the installed app (M7).
- **Backup size (NFR-11)**: 10k library.db 12.9–13.1 MB → backup zip **4.1–4.2 MB** in 0.38–0.46 s — **Verified**, 2026-10-18. Below the BRD 10–30 MB "typical" estimate: text-only libraries compress ~3x. The estimate stays a safe upper bound for MVP; the size warning default (50 MB) is far off.

## Integrity (NFR-05)
- `tests/integrity.rs` re-runs its own test binary as a child (`stress_child`, env-gated) and `TerminateProcess`es it. **Mid-write** (500-item transactions + FTS triggers) at 400/1300/2700 ms (500/2000/5500 items committed) and **mid-backup** (`backup_now` loop over the 10k library) at 30/250/700 ms: every time `PRAGMA integrity_check` = ok, FTS5 `integrity-check` passes, `db::open` works, and only whole batches are committed — **Verified**, 2026-10-18. Quick non-ignored variant: one mid-write kill at 300 ms (debug, < 1 s).
- A kill is a process crash, not power loss: OS file buffers still reach disk, so this does not prove durability across power cut (WAL + `synchronous=NORMAL` may lose the last commit then, never corrupt) — **Interpretation**, SQLite docs https://www.sqlite.org/pragma.html#pragma_synchronous
- Half-written backups are never listed: the zip is written as `<name>.zip.tmp` and renamed; a truncated `.zip` is skipped by `list_backups` (manifest unreadable) — **Verified**, 2026-10-18.

## Gotchas
- 2026-10-18: a killed backup left `runout-backup-*.zip.tmp` and the `.runout-backup-*.zip.db` snapshot behind forever (only the same-name snapshot was removed). `backup::rotate` now deletes those leftovers after the next good backup; asserted in `kill_mid_backup_three_offsets`.
