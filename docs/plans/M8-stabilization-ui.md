# Plan: M8 — Stabilization, Complete Functionality & UI Polish (Taste Skill)

Specs: [MILESTONES M8](../MILESTONES.md#m8--stabilization-complete-functionality--ui-polish-taste-skill) · [DESIGN.md §2 (Taste skill)](../DESIGN.md#2-design-read-and-dials-taste-skill) · [API.md](../API.md) · [TSD.md](../TSD.md) · [BRD.md §4.4](../../BRD.md) · [Known gaps (MILESTONES.md)](../MILESTONES.md#known-gaps).  
Agents: `react-ui` (UI streamline & states), `rust-core` (gap resolution & backend hardening), `spec-reviewer` (design & compliance review).

**Entry:** M7 closed (or live acceptance verified). U-5 daily use active/reviewed.

## Focus & Principles
1. **Stabilization & Zero Friction:** Every feature across all releases (MVP, R2, R3) must work cleanly end-to-end on real data. No unhandled states, dropped inputs, or silent failures.
2. **Taste Skill UI Uplift (`DESIGN.md` §2):**
   - Personal desktop product UI for one deliberate listener; data-rich like Last.fm, calm like a notebook, tactile rather than flat.
   - Dials: `DESIGN_VARIANCE 5 · MOTION_INTENSITY 5 · VISUAL_DENSITY 6`.
   - 3 depth layers (`--elev-0`, `--elev-1`, `--elev-1-hover`): 1px subtle hairlines, top highlight, soft shadow tinted to the background.
   - 5 drawn states for all interactive controls (default, hover, pressed, focus, disabled).
   - High typographic contrast (JetBrains Mono for numbers/counts vs muted labels; 800-weight headings with tight tracking).
   - Purposeful motion only (tactile press feedback, slide state transitions, polite Now-playing pulse).

## Steps

1. **Systematic Gap & Defect Resolution (WI 1):**
   - Targeted M8-candidate gaps from `docs/MILESTONES.md` (re-checked 22 Oct 2026; closures without evidence reopened):
     - **G-6** closed by design: FR-013 names saved filters; "New shelf" saves one, no separate shelf entity.
     - **G-16** fixed: `0007_backlog_history.sql` records backlog entry/exit (no row for `create_item`'s transient `in_backlog = 0` insert); a planned item taken out of the backlog stays in the denominator, unmoved (BRD §15: moved = status other than Planned). Test `moved_after_14d_keeps_items_taken_out_while_planned`.
     - **G-31** fixed with a ceiling: "Open in Studio" passes `&tracks=`; Studio picks the same-name playlist with that track count, else the first (the export's count can drift from the live one).
     - **G-33** accepted: apply and undo re-read the items and compare content right before writing, and wait `UNSURE_WAIT` (2 min ≫ 20 s timeout); a write landing later still can't be seen. Revisit only if seen.
     - **G-34** closed: the Shortcut was dropped with U-3 (D33); the LAN listener took a real request in installed journey B.
     - **G-37** accepted: the user's Spotify apps share only full links; `resolve_short` is fixture-tested only. Paste a real `spotify.link` if one ever appears.
     - **G-38** open: candidate switches added to `additionalBrowserArgs`, not yet confirmed with `scripts/installed/netwatch.ps1` on an installed build.
     - **G-39** fixed: album batches group entries within 60 s (`BATCH_GAP_S`), not by clock minute, so a straddling add stays one album.
     - **G-40** open: needs one live run per option (U-5).
     - **G-41** fixed: the gate answers loopback before reading, so the test reads without sending first (sent bytes left unread made Windows reset the connection).
   - Any issues identified during U-5 daily use.

2. **End-to-End Functionality Hardening (WI 2):**
   - Exercise and stress-test all main workflows:
     - Backlog management, filtering, tagging, status changes, promotion flows.
     - Streaming history importer (large volume handling, duplicate handling, missing metadata).
     - Live desktop play logging (SMTC accumulator, repeat detection, sleep/resume, exit flush).
     - Insights aggregation, source tags, freshness labels, empty states.
     - Playlist Studio (deduplication, reordering, apply, undo, conflict recovery).
     - Runout Inbox automation (phone mobile data capture, single tracks, full albums, missing inbox state).
     - Spotify library synchronization (save/remove tracks and albums, error recovery).
   - Verify network disconnect/reconnect transitions gracefully degrade and recover without app restarts.

3. **UI Streamline & Aesthetic Uplift via Taste Skill (WI 3):**
   - **Elevation & Layers:** Audit cards, modals, sheets, and lists against `--elev-0`, `--elev-1`, and `--elev-1-hover`. Remove flat or inconsistent borders.
   - **Interactive States:** Audit every button, chip, tab, row, and input across all 13 screens for the 5 drawn states (default, hover, active/pressed, focus-visible outline, disabled).
   - **Typographic Polish:** Ensure all numerical figures, counts, timestamps, and durations use JetBrains Mono; tighten headings and metadata labels for a balanced notebook feel.
   - **Layout Streamlining:** Eliminate redundant chrome, balance visual density (`VISUAL_DENSITY 6`), polish spacing tokens, and enhance the Now-playing ambient accent.
   - **Motion Audit:** Validate that motion exists only for tactile feedback and state transitions (`MOTION_INTENSITY 5`), maintaining a quiet, distraction-free environment.

4. **Future Candidates Evaluation (WI 4):**
   - Review BRD §4.4 candidate items (e.g., deep listening analytics, markdown/CSV export) based on user preference and priority.
   - **Evaluated 23 Oct 2026** (building any of them is a scope change, so the user picks; none started):

     | Candidate | Value | Cost / risk | Recommendation |
     |---|---|---|---|
     | Deep listening analytics (trends, streaks, charts by period, clock, discovery rate) | High: 180k imported plays + live log already in SQLite; extends Insights | Medium: new `insights` queries + 1–2 screens; compliance-clean (sources U/S/E only, BRD §4.3) | **Do first** (M9 candidate) → became **M10** (D35, 24 Oct 2026) |
     | Markdown / CSV export | Medium: data portability, Obsidian notes | Low: one Rust command + file dialog; export only source U/S/E fields (promoted fields marked, BRD §5.2) | **Do second** |
     | Custom fields; per-track tracking in albums | Medium | Medium–high: schema + contract + FTS changes | Wait for U-5 daily-use feedback |
     | PWA companion over local Wi-Fi | Low since the Runout Inbox covers phone capture (D33) | High: new network surface, auth on LAN | Drop unless U-5 shows a need |
     | Cross-device sync via user-owned storage | Low (one Windows device) | High: SQLite file conflicts | Drop |
     | Links to other services | Low | Compliance: never in the same row/set as Spotify content (S10) | Drop |

5. **Verification & Audit:**
   - Full automated test suite passes: `python scripts/check.py` clean.
   - Full journey audit: `node scripts/journeys.mjs` keyboard accessibility and visual validation.
   - External audit / spec review against `DESIGN.md` §2 and Spotify Developer Policy.

## Done when
- All targeted gaps resolved and verified on real data.
- Zero unhandled exceptions or broken workflows across the entire app.
- UI passes taste skill checklist (depth, 5 states, typography, density, purposeful motion).
- `python scripts/check.py` and `node scripts/journeys.mjs` exit 0.

