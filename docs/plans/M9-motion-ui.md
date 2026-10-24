# Plan: M9 — Interactive UI refinement (taste skill, motion)

Specs: [MILESTONES M9](../MILESTONES.md#m9--interactive-ui-refinement-taste-skill-motion) · [DESIGN.md §2–3](../DESIGN.md#2-design-read-and-dials-taste-skill).
Agents: `react-ui` (screens), `spec-reviewer` (design + a11y review). UI only: no contract, Rust or scope change.

**Entry:** M8 UI audit ticked (23 Oct 2026). Started 24 Oct 2026 by the user: "refine the UI using taste and modern UI skills showcasing highly interactive elements with animation. Start with settings."

## Principles
1. Dials move to `DESIGN_VARIANCE 5 · MOTION_INTENSITY 7 · VISUAL_DENSITY 6` (DESIGN §2). Still product UI: no marketing patterns, no glassmorphism, one accent.
2. Every animation states its reason: feedback, state change, orientation, or arrival of data. No decoration loops; the Now-playing pulse stays the only loop.
3. Native platform first for states: CSS transitions, `::details-content` + `interpolate-size`, IntersectionObserver, the clipboard API. **Motion for React** (`motion` 13.5.1, user 24 Oct 2026: "use motion for animations") from WI 3 on, for data arriving and changing: springs that can be interrupted, `AnimatePresence` for swapped content. It loads as `LazyMotion` + `domAnimation` with `m.*` components (`strict`), under one `MotionConfig reducedMotion="user"` (`src/main.tsx`). GSAP stays out.
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
3. **Home (WI 3)**, done 24 Oct 2026 (user: "redesign Home for users with a long Spotify history; state of the art, animated, interactive; test responsiveness"; "don't compromise performance or scalability"):
   - **Your last 12 months:** an area chart of `monthly` beside Now playing. Point at it or use the arrow keys (`role="slider"`, Home/End) and a spring-driven marker moves between months while the readout updates. It shows the 12-month total, the busiest month, the average line, and the current month marked "so far". The line is revealed with a clip-path once.
   - **Period strip:** the heading names the range and swaps with a short slide. Deltas carry an arrow. All time drops the meaningless delta and says "About N days of music" and "N artists in your history". Long totals stay short (`hmBig`: "9,843 h"; `hrs`: "1,284 h").
   - **Most played:** one card with Artists, Albums and Tracks tabs (`top_tracks` was never shown before) and all 10 rows. #1 is a lead (hours, plays, share of your time, your opinion and its action); 2–10 flow down two columns, their bars spring in staggered, unsaved rows get an accent bar and a Save button. The panel slides from the picked tab's side (`AnimatePresence`). Artwork mode stays on the Albums tab.
   - **Second pass (user: "still static and flat with UI issues"):** the 12-month line became 12 bars that grow in, the month in focus in accent with a value tip springing above it; sections rise in on arrival; the Settings spotlight (now `spot` in ui.tsx) plus a pointer glow on every Home card; #1 lead gets a share-of-time ring; Plays and Listening time get now/before bars; rank rows are whole-row targets with hover fill and press; fixed: the embed cropped to 152 px (Spotify's compact player needs ~250 px), the tab hairline stopping under the tabs, the lead card not filling its row.
   - Your take labels each row (Loved, then quiet; Played, not rated; Waiting, never played). Now playing swaps the track with a slide. A single 4+1 album grid and five repeated source tags are gone.
   - **Scale:** Rust aggregates in SQL, so Home renders a fixed DOM (top 10 ×1 panel, 12 months, 6 plays) whatever the history size. Motion moves transform/opacity/clip-path only. Scrubbing re-renders the chart card only, at most once per month crossed. Dev state **heavy** (`src/mocks`) scales the listening mocks ~37× (180k-play range, long names) for checking this.
   - Responsiveness: screenshots at 900, 1100, 1280, 1440, 1600 and 1920 (normal and heavy) show no horizontal overflow, and the wrap points are deliberate (chart under Now playing below ~1080, lead above the ranks, ranks one column below ~620 of card width).
4. **Recent plays (WI 4)**, done 24 Oct 2026 (user: "refactor the recent play feature using taste + motion"):
   - **Scale:** the old page asked for 500 plays of 30 days, then 12 months, so a long history never reached earlier days (and "Show earlier days" claimed totals it couldn't show). Now one query pages newest first, 150 at a time, as you scroll (IntersectionObserver 600 px ahead, plus a "Load earlier plays" button for the keyboard), back to the first play; each day is a `content-visibility: auto` section, and a long day shows 12 plays with "Show N more" (opens on a Motion height animation). Heavy dev state: 26,400 plays; 600 loaded = 133 rows in the DOM.
   - **Each day:** a sticky head with the day, plays and listening time, and a 24-hour strip of bars (ticks 00/06/12/18). Pointing at a play lights its hour in accent (a DOM class, no React state).
   - **Rows:** whole-row link to the record, hover fill and press, device as an icon (this PC / phone), duration, and your stars or Rate/Save; rows arrive with a short Motion fade-rise.
   - **Side:** the logger card shows its state (Logging with the live dot / Paused / Off) and what's playing, and stays in view while you scroll.
5. **Next screens (WI 5+)**, one per step, same rules: Backlog, Insights (bars grow in, rank bars), Playlist Studio (keep/cut), Item. Each gets its own scoped recipes and a before/after journeys screenshot diff.
6. **Review** — `spec-reviewer` against DESIGN §2–3 and accessibility; findings fixed.

## Done when
- Every screen refined under the rules above; DESIGN §3 motion table lists each new motion.
- `python scripts/check.py` and `node scripts/journeys.mjs` pass (M1 reduced-motion, M2 names, M3 focus rings).
- Screenshot diff of unrelated screens shows timing noise only.
