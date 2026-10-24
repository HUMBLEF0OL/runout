---
name: build-work-item
description: Implement one Runout milestone work item from docs/MILESTONES.md end to end (spec → test → code → proof). Use when starting or continuing any numbered work item, feature, or FR from the BRD/TSD.
---

# Build a work item

## Process
1. **Locate.** Read the **Status** block and **Known gaps** in `docs/MILESTONES.md` (any G-n this item touches is part of the job), then the milestone's plan in `docs/plans/`. Find the item in `docs/MILESTONES.md`. Read every TSD section and BRD FR/NFR it names, its row(s) in `docs/SOURCE_MAP.md`, and the relevant `docs/kb/` file. Note the milestone's acceptance lines this item feeds.
2. **Check for blockers.** Item needs a `U-n` from `docs/USER_ACTIONS.md` that isn't done → stop, point the user to it, work on a different item meanwhile. Item rests on an open `⚑ verify`/T-n → run `verify-assumption` first.
3. **Test first** for any rule or algorithm (parser, play rule, matching, dedupe, rotation, promotion): write the failing `cargo test` / Vitest case from the spec's examples. UI-only glue needs no test.
4. **Implement** the minimum that passes, in the file TSD §9 assigns. Logic and I/O in Rust; UI via typed wrappers in `src/api.ts`.
5. **Schema change** → new numbered file in `src-tauri/migrations/`; never edit a shipped migration.
6. **Prove** (below). Fix until green. Then get a `spec-reviewer` pass (`compliance-review`) and fix its findings.
7. **Record.** New file → its path is in `docs/SOURCE_MAP.md`; first working version → status `built`. Platform surprise → dated entry in `docs/kb/` Gotchas. Spec was wrong → update TSD (`change-spec` if scope moves). Fixed a G-n → delete its Known gaps row and name it in the commit; deferred something (incl. anything proven only on synthetic/stub data) → add a G-n row (gap, where, why, fix in); `check.py` "deferrals tracked" fails an open acceptance line with no G-n/U-n. Tick nothing in MILESTONES; ticking is `close-milestone`'s job.
8. **Report** in ≤3 lines: what's done, proof result, anything deferred.

## Toolbox
- Specs: `docs/MILESTONES.md`, `docs/TSD.md`, `BRD.md` (FR/NFR ids).
- Code map: `docs/SOURCE_MAP.md`. Facts and gotchas: `docs/kb/`.
- Agents: `rust-core` (src-tauri), `react-ui` (src), `spec-reviewer` (review), `spotify-researcher` (external facts).
- Layout: TSD §9. Test plan: TSD §8. Error behaviour: TSD §6. Allowed hosts: TSD §7.
- Fixtures: `src-tauri/tests/fixtures/` — synthetic or structure-only, never real values.
- Proof runner: `python scripts/check.py`; UI journeys: `node scripts/journeys.mjs` (Vite on :1420).
- Git: commit on `dev`, never push (CLAUDE.md "Start here").

## Proof (all required)
- [ ] `python scripts/check.py` exits 0.
- [ ] New rule/algorithm has a test that fails if the rule breaks (comment it out mentally — would a test go red?).
- [ ] Every TSD/BRD behaviour the item names is either implemented or explicitly deferred in the report.
- [ ] `spec-reviewer` reports no unresolved findings.
- [ ] SOURCE_MAP rows for touched paths are correct (coverage is checked by `check.py`; status is on you).
- [ ] Feature is user-facing → `node scripts/journeys.mjs` passes, and run the app (`npm run tauri dev`) and exercise it once; say what you saw.
- [ ] MILESTONES Known gaps updated (fixed rows removed, new deferrals added).

## Gotchas
<!-- Lessons from corrections go here, newest first. Promote to scripts/check.py when automatable. -->
- **UI redesign "looks static and flat" (24 Oct 2026, M9 Home).** Motion on arrival alone isn't enough. Before calling a UI item done, look at your own screenshots at 900 / 1280 / 1440 and check that: every card or row responds to the pointer (hover fill, spotlight, press); the focused data point is shown by the one accent; numbers have a visual next to them (bars, ring), not text alone; hairlines run the full width of their container; no embed is narrower than Spotify's layout (compact player ≥ 250 px). Reduced motion is enforced by `check.py` "reduced motion (JS)".
