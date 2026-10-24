# Plan: M9 — Interactive UI refinement (taste skill, motion)

Specs: [MILESTONES M9](../MILESTONES.md#m9--interactive-ui-refinement-taste-skill-motion) · [DESIGN.md §2–3](../DESIGN.md#2-design-read-and-dials-taste-skill).
Agents: `react-ui` (screens), `spec-reviewer` (design + a11y review). UI only: no contract, Rust or scope change.

**Entry:** M8 UI audit ticked (23 Oct 2026). Started 24 Oct 2026 by the user: "refine the UI using taste and modern UI skills showcasing highly interactive elements with animation. Start with settings."

## Principles
1. Dials move to `DESIGN_VARIANCE 5 · MOTION_INTENSITY 7 · VISUAL_DENSITY 6` (DESIGN §2). Still product UI: no marketing patterns, no glassmorphism, one accent.
2. Every animation states its reason: feedback, state change, orientation, or arrival of data. No decoration loops; the Now-playing pulse stays the only loop.
3. Native platform first: CSS transitions, `::details-content` + `interpolate-size`, IntersectionObserver, the clipboard API. No animation library (Motion/GSAP not added).
4. Animate `transform`/`opacity` (plus the details height, which the platform interpolates). Pointer-driven values go to CSS custom properties, never React state.
5. `prefers-reduced-motion` collapses everything to instant (global rule in `tokens.css`); JS motion checks it too (smooth scroll, count-up).
6. Each screen's recipes are scoped (`.st-*` for Settings) so a screen's refinement can't leak into another; shared controls (switch, checkbox, segment) change app-wide on purpose. Journeys screenshots are compared before/after.

## Steps
1. **Settings (WI 1)** — done 24 Oct 2026:
   - Toggle switches (`input.toggle`, role `switch`): rounded-rect track (shape lock, no pills), spring thumb, stretch on press; flip at once and roll back if the write fails (`settings.test.tsx`).
   - **Tabs, one section at a time** (user, 24 Oct 2026: "content should be displayed corresponding to tabs only"): `Seg line` tablist (arrow keys, roving focus) with a spring underline; the panel slides in from the picked side; `?tab=` deep-links (Connect Spotify → `spotify`, Import → `import`, Tags → `tags`). Replaced a first try with a sticky jump bar over a two-column card grid. Journeys pick tabs by keyboard (`pickTab`).
   - Pointer spotlight on the card hairline (`--mx/--my`).
   - Metric numbers count up once when scrolled into view; screen readers get the final text only.
   - Wi-Fi capture disclosure opens with a height transition and a rotating caret; pairing code gets a copy button with a check-mark pop.
2. **App-wide controls (WI 2)** — done 24 Oct 2026 (user: "make the toggle application wide, and think in the similar direction for other elements"): every on/off checkbox is a switch (Item: Saved in Spotify, Revisit later, Favourite; Rediscover: Private playlist); selection checkboxes (Backlog rows, Quick-add tracks, Item track list) get the rounded-rect box with a spring check mark; all segment indicators use the spring. `check.py` "hover keeps selected" guards the review finding that a hover repainted a checked thumb (also seen in M8 on segments).
3. **Next screens (WI 3+)** — one per step, same rules: Home / Now playing, Backlog, Insights (bars grow in, rank bars), Playlist Studio (keep/cut), Item. Each gets its own scoped recipes and a before/after journeys screenshot diff.
4. **Review** — `spec-reviewer` against DESIGN §2–3 and accessibility; findings fixed.

## Done when
- Every screen refined under the rules above; DESIGN §3 motion table lists each new motion.
- `python scripts/check.py` and `node scripts/journeys.mjs` pass (M1 reduced-motion, M2 names, M3 focus rings).
- Screenshot diff of unrelated screens shows timing noise only.
