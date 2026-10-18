---
name: change-spec
description: Propagate a requirement or design change across Runout's spec set (BRD.docx → BRD.md, TSD, CAPABILITIES, MILESTONES, USER_ACTIONS, SOURCE_MAP, KB) so they stay consistent. Use whenever a decision, scope item, FR, tech choice or milestone changes.
---

# Change the spec

## Process
1. Classify: **scope/compliance** (BRD promise changes) → hard stop, get the user's OK first. **Design** (how) → decide and proceed.
2. BRD change: edit `BRD.docx` (python-docx), add a Change History line and a Decisions Log `D-n` if it's a decision, then `python scripts/brd_to_md.py`. Never hand-edit `BRD.md`.
3. Walk the chain and update every affected file: TSD (§ and §10 open items) → CAPABILITIES (CAP rows, FR refs) → MILESTONES (work items, acceptance) → USER_ACTIONS (hard stops) → SOURCE_MAP (paths, specs) → KB (facts, open questions).
4. Grep for the old term/ID across `BRD.md docs/ .claude/ CLAUDE.md` to catch stragglers.
5. Report in one line per file changed.

## Toolbox
- `scripts/brd_to_md.py`; `python-docx` (installed).
- ID schemes: CAP-n, FR-n, NFR-n, D-n, S-n, O-n, T-n, U-n, M-n.

## Proof
- [ ] `BRD.md` regenerated after any `.docx` edit (its header says generated).
- [ ] Grep for the old wording/ID returns only intended hits.
- [ ] Every CAP's FRs exist in BRD §7; every milestone's CAPs exist in CAPABILITIES.
- [ ] `python scripts/check.py` exits 0.

## Gotchas
<!-- Lessons from corrections go here, newest first. -->
