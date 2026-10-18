# Plan: M2 UI build (mock data)

Specs: [MILESTONES M2](../MILESTONES.md#m2--ui-build-mock-data) · [DESIGN.md](../DESIGN.md) (tokens §3, components §5, screens §6, compliance §7) · [API.md](../API.md) (every type and command) · TSD §1, §4.3 · BRD §8 journeys A–H, NFR-12.
Reference: prototype `docs/design/prototype/` (`#/system` = design-system page) and `docs/design/screens/`.
Agent: `react-ui` for everything; `spotify-researcher` for T4 before step 6.

**First implementation milestone** ([plans README](README.md)): builds every screen of MVP, R2 and R3 so the backend milestones only fill in the contract.

**Entry:** M1 accepted and **U-9** approved. Do not start screen work (step 5+) before U-9; steps 1–4 may start once DESIGN.md tokens are frozen.

## Steps
1. **Tooling (WI 1).** Add `react-router-dom` (hash router, matches prototype routes), `@phosphor-icons/react`, `recharts`. Self-host Schibsted Grotesk + JetBrains Mono woff2 in `src/assets/fonts/` with `@font-face` in `src/styles/fonts.css`. Write `src/styles/tokens.css` from DESIGN §3 verbatim (colours, elevation, radius, spacing, motion, reduced-motion override).
   - Proof: Vitest `tokens.test.ts` parses `tokens.css` and asserts every DESIGN §3 text/background pair ≥ 4.5:1 (NFR-12). Becomes a `check.py` gate via Vitest.
2. **Contract + mocks (WI 3).** `src/api.ts`: every type and command in API.md §1–6 as `camelCase` functions; one `call<T>(cmd, args)` that routes to `invoke` or to `src/mocks/` by a single build flag `VITE_MOCK` (default on in M2). `src/mocks/`: loads `mock-data.json` (copied from the prototype by a Vite import, not duplicated), keeps edits in memory, applies the dev state switcher (`loading|empty|offline|error`) and 150 ms delay. Mock link parsing reuses the TS port of TSD §4.1 only inside mocks.
   - Proof: `tsc` green; Vitest for mock `create_item` duplicate → `duplicate` error with `existing_id`; `parse_link` table (≥ 10 cases, same table M3 reuses).
3. **Shell (WI 1).** Rail (nav + mono counts + freshness box, collapses < 1100 px), top bar with the combined search/add field, routes for every DESIGN §6 path, dev state switcher (dev builds only, `import.meta.env.DEV`). Usage: `recordPageVisit` on route change.
4. **Components (WI 2)** in DESIGN §5 order, each with all five interaction states; logic-bearing ones get Vitest:
   - Rating control (`role="slider"`, arrows ±0.5, halves) — test.
   - Combined field link detection on paste (switches button to Add) — test.
   - Freshness note text (“Desktop: live. All devices: up to …”) — test.
   - Status control, tag chip/add tag, stat tile, rank bar row, record card, play row, toast with Undo, skeleton, empty state, name-needed badge, source tag, live dot, attribution, embed slot (placeholder only here).
   - Build `#/system` design-system page in-app mirroring the prototype one; it is the side-by-side review target.
5. **Screens (WI 4)**, MVP first (it carries the first release), then R2, then R3 — all three before M2 closes:
   1. MVP: SC-02 Home MVP + first run, SC-06 Quick add (all modes incl. duplicate/unsupported/offline/naming), SC-03 Backlog (reorder, multi-select bar), SC-05 Item detail (album/track/artist, edit, history), SC-04 Library + search, SC-07 Rediscover, SC-13 Settings (data, tags, backup/restore, Shortcut steps, delete library).
   2. R2: SC-01 Home full, SC-08 Recent plays, SC-09 Insights (Recharts; source tag + freshness on every card), SC-10 Import (3 steps), SC-11 Playlists check-up.
   3. R3: SC-11 Studio (draft, preview, apply confirm, undo banner), SC-12 From Spotify, SC-06 autofill mode, SC-07 Make a Spotify playlist, SC-13c Connect.
   Each screen: loading, empty, offline and error states driven by the switcher; compare with `docs/design/screens/` and `screens/states/`.
6. **Embeds (WI 5).** Run `verify-assumption` for **T4** first. Main window `incognito(true)` in `tauri.conf.json`; CSP per TSD §4.3. Embed slot derives `https://open.spotify.com/embed/{type}/{id}`, mounts only when `getOnline()` is true and the slot is visible (IntersectionObserver), artwork mode pages of 20 ending with attribution + “Open Spotify”. Official Spotify logo file from brand resources in `src/assets/spotify/` at minimum sizes (DESIGN §7).
   - Proof: T4 recorded in `docs/kb/windows-tauri.md`; `EBWebView` holds no HTTP cache after browsing 10 embeds.
7. **Keyboard (WI 6).** `Ctrl+N` add, `Ctrl+K` search, `1–5` status on focused item, `Esc` closes dialog, arrows on rating. One `useShortcuts` hook; Vitest for key → action mapping.
8. **Accessibility (WI 7).** Keyboard-only walkthrough of journeys A–H; every icon-only button has `aria-label`; focus ring never removed; `prefers-reduced-motion` honoured.
8b. **Tailwind migration** (user decision, 18 Oct 2026). After the screens land: add Tailwind v4 (`@tailwindcss/vite`), map DESIGN §3 tokens into `@theme` in `src/styles/tokens.css`, move component styles to utilities (keep `@layer components` only for multi-state recipes: elevation, buttons, segmented, rating, skeleton, Now-playing tint). No visual change: re-screenshot every screen against its board. Update TSD §1/§9 and DESIGN §3 (tokens live in `@theme`).
9. **Review.** Side-by-side of every SC-n vs approved board; `spec-reviewer` compliance pass (attribution, ≤ 20 embeds, no source A in insights, no Spotify look-alike); fix findings.
10. **Record.** SOURCE_MAP rows for `src/api.ts`, `src/mocks/`, `src/styles/`, `src/pages/`, `src/components/`, `src/hooks/`, `src/assets/` → `built`. Then `close-milestone M2`.

## Parallelism
Steps 1–2 together; 3 needs 1; 4 needs 1+3; 5 needs 2+4 (screens of one release can be split across two `react-ui` agents); 6 independent after 3; 7–8 after 5.

## Rules
- Pages call `api.ts` only. No `fetch`, no SQLite, no `@tauri-apps/plugin-*` imports in `src/` (check: grep in `check.py`, added in step 2).
- Anything visual not in DESIGN.md → add it to DESIGN.md first (`change-spec`).
- Mock data is the generated file; never hand-edit counts. Need new data → change `scripts/gen_mock_data.py` and regenerate.
- No real Spotify IDs beyond the random mock ones.

## Risks
| Risk | Response |
|---|---|
| T4 fails (InPrivate breaks embeds) | Fall back to normal profile + clear cache on exit; record in KB; `change-spec` TSD §1 |
| Embed iframe cost in artwork mode | Lazy mount + 20 cap; measure scroll on 20 slots |
| Contract gaps found while building screens | Fix API.md first, then mock (§7 of API.md) |

## Done when
MILESTONES M2 acceptance passes; `python scripts/check.py` exits 0.
