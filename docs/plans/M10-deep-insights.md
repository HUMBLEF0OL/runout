# Plan: M10 Deep listening insights (Insights v3)

Specs: [MILESTONES M10](../MILESTONES.md#m10--deep-listening-insights-insights-v3) · design and every definition: [design/M10-insights-v3.md](../design/M10-insights-v3.md) (mockup + screens in `docs/design/`) · TSD §4.9, §4.14, §4.13 · API.md §4 · BRD FR-031, 032, 047, 048, NFR-04, D35, O8.
Agents: `rust-core` (WI 2–5), `react-ui` (WI 6–8), `spec-reviewer` (each step), `spotify-researcher` (WI 9's T6, only after U-11).

**Status: ready (24 Oct 2026).** Next: WI 1 (contract).

**Entry:** M9 Home (WI 3) and Insights (WI 5) done, met 24 Oct 2026. M10 builds on their recipes (charts.tsx, `.hm-plot`, `Stat`, `useSlide`). It runs alongside M9's Item screen and review. M10 recipes go in their own `app.css` section (`/* ---------- Insights v3 (M10) */`), so the two never edit the same lines. Unlike M2–M7, M10 is one feature end to end: contract, mock, UI, Rust, integration, in that order per tab.

**Known gaps owned here:** G-45 (skip-rate denominator), G-46 (comparisons mix coverage). Remove each row when fixed.

## Steps
1. **Contract (WI 1).** API.md §4 first:
   - `get_period_report(period)`, `get_trends(measure)`, `get_discovery(period)`, `get_listening_habits(period)`, `get_records()`, `explore(query)`, `get_data_coverage()`; `period` = `PeriodQuery {grain, vs, at, from?, to?}` (the page header);
   - every payload is `Covered<T>` = `Sourced<T>` plus a `coverage {compared_sources, desktop_only_since, gaps[], no_compare_reason}` block;
   - `get_listening_insights` keeps `range` for Home and gains the same `coverage` block. Its `plays_prev` / `ms_prev` become like-for-like (design §4.4), and Home's period strip shows the short coverage note (G-46).

   Then `src/api.ts` types, the `src/mocks/` implementation (shaped from the mockup's data, "heavy" state = 180k plays), and the contract check in `check.py`.
   - **Done 24 Oct 2026:** API.md §4.1, `api.ts`, `mocks/insightsV3.ts` (+ `insightsV3.test.ts`). The seven commands sit in `check.py` `PENDING_RUST` until WI 5 registers them (the check then asks for their removal); Rust's `PeriodStats` gains `plays_cmp`/`ms_cmp` and `Settings` `export_reminder` in WI 4/5. Saved Explore views (stored like saved filters) get their contract in WI 8.
2. **Schema + logger (WI 2).** Migration `0008` (0006 and 0007 are taken):
   - `listening_event.duration_ms`, `listening_event.shuffle` (nullable);
   - `logger_run`;
   - rollups `listening_day`, `listening_month_artist`, `artist_first_play` with triggers (pattern: `0005`), rebuilt after import.

   Logger: keep SMTC `EndTime` (already read in `logger/session.rs`) and shuffle state; write `logger_run` on start/stop/pause/resume.
   - Tests: accumulator carries duration and shuffle; triggers keep the rollups equal to a full recount after insert, delete and reconcile; migration from a v7 DB with data.
   - **Done 24 Oct 2026:** `migrations/0008_listening_v3.sql` (`shuffle` already existed from 0003, so only `duration_ms` is added), `db::rebuild_rollups` after each import, `Logger::sync_run`/`end_run`. Tests: `closed_play_carries_duration_and_shuffle`, `plays_store_duration_and_shuffle`, `logger_run_records_running_intervals`, `rollups_equal_a_recount_after_every_write` (goes red with any 0008 trigger dropped), `v7_with_plays_upgrades_to_v8`. perf_listening import 32.5 s.
3. **Definitions (WI 3).** One Rust function per design §4.2 term in `insights/`, each with a fixture test against hand-computed values:
   - the base terms: play, streaks across gaps, stick rate, the skip rate fix (G-45), quick bail, started-by-you, device classes over the KB's seen `platform` values;
   - sessions, deep dives, eras, arcs, "took time from";
   - the fingerprint, music ratio, records and top tags.
   - **Done 24 Oct 2026:** `insights/terms.rs` + `insights/heuristics.rs` (one fn + constant per term, choices the design left open in TSD §4.14), each with hand-computed tests; G-45 fixed in `listening.rs` (bounded: flag filter; `all`: 0008 partial index `listening_event_skip_flag`, test `skip_rate_counts_only_flagged_events`); the synthetic export now holds every kb-seen `reason_start`/`reason_end` (`every_seen_reason_has_a_kind_and_is_in_the_fixture`). Proven on synthetic data only; thresholds get tuned on the real history in WI 10.
4. **Coverage engine (WI 4).** Per-source ranges, gaps, and the FR-048 rule as one function used by every comparison (G-46).
   - Tests: the current period crosses the export end → desktop-only on both sides; no logger run in the earlier period → delta hidden with a reason; a gap day doesn't break a streak.
5. **Commands + performance (WI 5).**
   - Assemble the WI 1 commands from WI 3–4.
   - Extend `perf_listening` (500k events): each command < 500 ms, `explore` < 200 ms, import 10 years still < 60 s.
   - Record the numbers in kb/perf-integrity.md.
6. **UI: Report + header (WI 6).** Page header (grain, stepper, compare), line tabs with deep links, and all Report cards per the design and mockup:
   - "the period in a few lines" (design §5.1 rules, in Rust, returned as structured sentences);
   - loading, empty, error and heavy states, CountUp, Motion recipes as M9.
7. **UI: Trends, Discovery, How you listen, Records (WI 7).** One tab per sub-step, each with its states, journeys and screenshots. The calendar links to Recent plays for a date; the brush sets a custom period.
8. **UI: Explore + Your data (WI 8).**
   - Explore: builder, ready-made questions, saved views (reuse saved filters), Copy as CSV.
   - Your data: coverage strip, import history, the reminder switch, and the private-session switch (moved from where it lives today).
9. **Genres & decades (WI 9, gated).** Only after **U-11 = yes**:
   - `spotify-researcher` resolves T6 (licence, rate limit, match rate on real names);
   - then `change-spec` adds the source to BRD §5.1, the allowed host to `check.py` and TSD §5;
   - then `enrich/` (names-only lookups, ≤ 1 request/s, separate tables, delete switch) and the tab.

   If U-11 = no, drop the step and record it.
10. **Integration + record (WI 10).**
    - Switch the commands from mock to `invoke`.
    - Installed-build run on the user's real history: hand-check 10 numbers against SQL.
    - Journeys, check.py, spec review, then SOURCE_MAP rows to `built`, KB facts, DESIGN §6 SC-09 states, then `close-milestone M10`.

## Parallelism
1 first (both sides depend on it). Then the two streams run side by side:
- **Backend:** 2 → 3 → 4 → 5 (Rust);
- **UI:** 6 → 7 → 8 (on the mock, React).

10 needs both streams. 9 runs any time after U-11 and T6, independent of the rest.

## Rules
- Sources U, S, E only. `check.py` "insights no cache" stays green. No autofill-cache durations, years or genres anywhere in `insights/`.
- Every heuristic is labelled in the UI as an estimate (design §4.2). Every comparison goes through the WI 4 coverage function, never a raw delta.
- Fingerprint scores are never exported, copied or shared. "Copy as CSV" in Explore copies plays and hours, not scores.
- No new colours or components outside DESIGN §3/§5. A new recipe goes in `app.css` `@layer components` with its reason in DESIGN §3.
- Fixtures carry structure only; never commit real export values.

## Risks
| Risk | Response |
|---|---|
| Lifetime queries miss NFR-04 on 500k events | Rollups in WI 2; measure in WI 5 before any UI depends on timing; precompute eras and arcs after import if needed |
| Heuristics (eras, arcs) read badly on real data | Tune thresholds once on the installed build (WI 10); each threshold is one constant |
| Coverage banner shows too often (exports are rare) | The monthly export reminder (off by default) and clear wording; desktop-only deltas still shown |
| U-11 never answered | WI 9 stays open; M10 closes without it, with a G-n row owned by a later milestone |

## Done when
MILESTONES M10 acceptance passes; `python scripts/check.py` and `node scripts/journeys.mjs` exit 0; the installed-build hand-check is recorded.
