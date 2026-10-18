---
name: close-milestone
description: Verify every acceptance check of a Runout milestone (M0–M8) in docs/MILESTONES.md, tick what passed with evidence, and hand off to the next milestone or hard stop. Use when all work items of a milestone look done, or the user asks for milestone status.
---

# Close a milestone

## Process
1. List the milestone's work items and acceptance lines from `docs/MILESTONES.md`.
2. Any work item unfinished → run `build-work-item` on it first.
3. Run `python scripts/check.py`, then a `spec-reviewer` pass over the whole milestone's code.
4. For each acceptance line, produce **evidence**: a command + its output, a query result, or a description of what you observed in the running app. No evidence → not passed.
5. Lines that need the user (`U-n`, real-use judgement) → mark "waiting on U-n" and point to `docs/USER_ACTIONS.md`.
6. Tick passed lines in `docs/MILESTONES.md` (`- [x]`) and append a one-line evidence note after each.
7. Resolve TSD §10 / KB open questions scheduled for this milestone (`verify-assumption`). Confirm SOURCE_MAP statuses match reality.
8. Report: passed / waiting / failed, then the next milestone's first work item.

## Toolbox
- `docs/MILESTONES.md`, `docs/TSD.md` §8 and §10, `docs/USER_ACTIONS.md`, `BRD.md` §13 (release criteria), §15 (metrics).
- `python scripts/check.py`; perf: `cargo test --release -- --ignored perf` (from M3).

## Proof
- [ ] Every ticked line has evidence written next to it.
- [ ] No line ticked on reasoning alone ("should work").
- [ ] `python scripts/check.py` exits 0 at close time.

## Gotchas
<!-- Lessons from corrections go here, newest first. -->
