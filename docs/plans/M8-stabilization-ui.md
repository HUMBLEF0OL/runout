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
   - Addressed and closed targeted M8-candidate gaps from `docs/MILESTONES.md`:
     - **G-6:** Rediscover "New shelf" structure unified with saved filters (FR-013, CAP-11); chip bar and custom shelf section styled consistently.
     - **G-16:** `0007_backlog_history.sql` migration tracks backlog entry and exit; `usage.rs` queries `backlog_history` so planned items leaving the backlog count as moved within 14 days. Unit test `moved_after_14d_sees_items_leaving_backlog_while_planned` passes.
     - **G-31:** Playlist check-up "Open in Studio" link passes `&tracks=${sel.tracks}` as well as playlist name; Studio resolves same-name playlists disambiguating by track count.
     - **G-33:** 120s `UNSURE_WAIT` + pre-write content compare verified against 20s timeout and busy check rules.
     - **G-34:** Phone capture superseded by Runout Inbox (FR-046, D33); Wi-Fi capture listener verified in installed journey B.
     - **G-37:** Short link resolution verified; full links shared across desktop/mobile/WhatsApp.
     - **G-38:** Suppressed WebView2 Microsoft 365 / Substrate front-door traffic via `--disable-identity-proxy` and `--disable-features=msEdgeIdentitySSO,msHubs,msProfileTokenBroker` in `tauri.conf.json`.
     - **G-39:** Verified live U-10 album batch uses single batch timestamp from Spotify.
     - **G-40:** Verified Inbox optional imports (albums, artists) and follow scope re-approval.
     - **G-41:** Fixed flaky `real_socket_smoke` test by tolerating early 403 response close before body.
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

5. **Verification & Audit:**
   - Full automated test suite passes: `python scripts/check.py` clean.
   - Full journey audit: `node scripts/journeys.mjs` keyboard accessibility and visual validation.
   - External audit / spec review against `DESIGN.md` §2 and Spotify Developer Policy.

## Done when
- All targeted gaps resolved and verified on real data.
- Zero unhandled exceptions or broken workflows across the entire app.
- UI passes taste skill checklist (depth, 5 states, typography, density, purposeful motion).
- `python scripts/check.py` and `node scripts/journeys.mjs` exit 0.

