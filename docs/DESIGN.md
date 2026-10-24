# Design Specification — Runout

| | |
|---|---|
| Version | 1.1 (uplift: depth, interaction states, motion, design system board) |
| Date | 18 Oct 2026 |
| Direction | **D · Pulse** (BRD D29): Last.fm-inspired listening story + your opinions; Spotify-compatible dark |
| Designs | [docs/design/](design/README.md): clickable prototype of every screen and state, screenshots, journey walkthroughs |
| Related | [BRD](../BRD.md) · [TSD](TSD.md) · [Capabilities](CAPABILITIES.md) · [Milestones](MILESTONES.md) |

The BRD says what, the TSD says how it works; this file says **how it looks and behaves**. M1 produces every screen below in the design prototype; M2 builds them in React from this spec and the prototype.

## 1. Principles
1. **Every play is a prompt for an opinion.** Listening data is never shown alone: it always sits next to your rating, status or a one-click action (Rate, Add a note, Save to backlog).
2. **Your data and Spotify’s data look different.** Source-A content (embeds, Top items, recent plays from the API) carries Spotify attribution and an “Open Spotify” link; it is never mixed into Insights charts (BRD 5.1, D27, S9, S10).
3. **Honest freshness.** Any screen built from listening data says how current it is (“Desktop: live. All devices: up to <export date>”).
4. **Spotify-compatible, never Spotify-like.** Dark graphite so embeds sit naturally; no Spotify green, no Spotify layout or logo-like marks (Developer Policy III.11, Embeds Terms VIII.5). The only green on screen is inside Spotify’s own embed.
5. **Quiet by default.** One accent, used for “needs your attention” (unrated plays, high priority, new). Motion only for feedback and state changes.

## 2. Design read and dials (taste skill)
Personal desktop product UI for one deliberate listener; data-rich like Last.fm, calm like a notebook, **tactile rather than flat** (v1.1 uplift).
`DESIGN_VARIANCE 5 · MOTION_INTENSITY 7 · VISUAL_DENSITY 6` (motion raised from 5 in M9, 24 Oct 2026). Product UI, so no marketing-page patterns (heroes, logo walls, marquees).

What “not flat” means here, and nothing more:
1. **Three depth layers** (canvas, raised, overlay) built from a 1px light hairline, an inner top highlight and a soft shadow tinted to the background. No glassmorphism, no neon glow.
2. **One ambient moment:** the Now-playing card picks up a faint radial tint of the playing album’s dominant color (8% opacity, behind the card only). Everywhere else stays graphite.
3. **Every interactive element has five drawn states:** default, hover, pressed, focus, disabled (plus loading where it applies).
4. **Motion with a reason:** feedback (press, switch thumb), state change (segment indicator slides, row enters, disclosure opens), orientation (tab underline springs to the pick, the panel slides in from that side), arrival of data (numbers count up once), live status (the Now-playing pulse, the only looping animation).
5. **Typographic contrast:** big mono numbers against small muted labels; 800-weight section titles with tight tracking.

## 3. Tokens
Tailwind v4 `@theme` in `src/styles/tokens.css` (M2): colors are `--color-<token>` (utilities `bg-surface`, `text-muted`, `border-line-strong`), type scale `text-xs`…`text-3xl` = 12…32, radii `rounded-bar/thumb/control/card` = 3/6/10/16, spacing `--spacing: 4px` (html is 14 px). Tailwind's default palette is removed, so off-token colors cannot be used. Dark is the only theme in v1 (BRD D29); light theme is a future candidate.

| Token | Value | Use |
|---|---|---|
| `--bg` | `#101113` | App background |
| `--rail` | `#0B0C0E` | Sidebar |
| `--surface` | `#17191C` | Cards, inputs, list containers |
| `--surface-2` | `#1F2226` | Nested items, selected nav, hover |
| `--line` | `#23262B` | Dividers, rail border |
| `--line-2` | `#2E3238` | Input, switch-track and dashed add-chip borders (M9: quiet at rest) |
| `--line-strong` | `#3A3F46` | Input hover border, selection-checkbox border |
| `--line-hover` | `#4A5059` | Hover ring on icon buttons, chips and switches |
| `--danger-line` | `#452C2C` | Danger frames (danger card, error banner): 20% `--danger` over `--surface` |
| `--text` | `#ECEDEF` | Primary text |
| `--text-2` | `#C3C7CE` | Secondary text |
| `--muted` | `#B6BBC3` | Labels, metadata (≥ 7:1 on every surface incl. hover fills; was `#A9AEB7` until M8) |
| `--faint` | `#9A9FA8` | Ranks, timestamps (≥ 6:1 on `--bg`) |
| `--accent` | `#F5B544` | Attention, primary button fill, active bar |
| `--accent-ink` | `#101113` | Text on accent |
| `--link` | `#F5C46B` | Links; hover `#FFDCA0` |
| `--bar` | `#4A5059` | Chart bars (no background track) |
| `--danger` | `#FF7A6B` | Destructive actions, errors |

**Type:** Schibsted Grotesk (UI) and JetBrains Mono (all numbers, times, counts). Both SIL OFL; **self-hosted** woff2 in the app bundle (no runtime font requests, TSD §7).
Scale: 13 / 14 (body) / 15 / 17 / 20 (section) / 26 (stat, page title) / 32 (detail title). Weights 400, 600, 700, 800 (headings).

**Elevation** (dark-mode depth comes from light edges, not big shadows):
| Token | Recipe | Use |
|---|---|---|
| `--elev-0` | `--bg`, no border | Page canvas |
| `--elev-1` | surface `linear-gradient(180deg, #1A1C20, #16181B)`, border `1px rgba(255,255,255,0.045)`, `inset 0 1px 0 rgba(255,255,255,0.04)`, shadow `0 8px 24px rgba(0,0,0,0.28)` | Cards, list containers |
| `--elev-1-hover` | border `rgba(255,255,255,0.10)`, `translateY(-1px)`, shadow `0 12px 28px rgba(0,0,0,0.34)` | Hovered cards, rows, tiles |
| `--elev-2` | `#1C1F23`, border `rgba(255,255,255,0.08)`, shadow `0 24px 64px rgba(0,0,0,0.5)` | Dialogs, menus, toasts, selection bar |
| `--accent-fill` | `linear-gradient(180deg, #F8C566, #F0A92E)`, `inset 0 1px 0 rgba(255,255,255,0.35)`, shadow `0 2px 8px rgba(245,181,68,0.18)` | Primary button |
| `--indicator` | `inset 3px 0 0 var(--accent)` | Selected nav item |

**Shape:** radius 10 for controls and list rows, 16 for cards, 6 for thumbnails and small or nested controls (small buttons, segment buttons and indicator, badges, menu items, switch, checkbox), 3 for bars. Pills are not used. Exceptions: the Spotify embed keeps Spotify's 12; the live dot and the Home share ring are circles. `check.py` “control consistency” enforces the scale.
**Spacing:** 4 · 8 · 12 · 16 · 20 · 24 · 32. Page padding 32 desktop.
**Focus:** 2px `--accent` outline, 2px offset, on every interactive element.
**Icons:** Phosphor (`@phosphor-icons/react`), regular weight, 20px; icon-only buttons have `aria-label`.
**Motion:** curve `cubic-bezier(0.16, 1, 0.3, 1)`. Hover 150 ms; press `scale(0.98)` 100 ms; list insert/remove 200 ms (fade + 4px rise); dialog 180 ms (fade + scale from 0.98); toast slides up 220 ms. The Now-playing live dot pulses (2 s, opacity only) and is the only loop. Skeletons shimmer once per 1.6 s. M9 additions: `--ease-spring` `cubic-bezier(0.34, 1.56, 0.64, 1)` for things that land (switch thumb 280 ms, tab underline 320 ms, caret, check-mark pop 260 ms); switch thumb stretches 4 px on press; disclosure height 300 ms; metric numbers count up 900 ms once on first view; card hairline spotlight follows the pointer (white 32%, 240 px). Page-level tabs (Settings) are `Seg line`: an accent underline on the hairline that springs to the picked tab, and the panel slides in 320 ms (fade + 18px from the picked side). Segment indicators everywhere use the spring (280 ms). Borders are quiet at rest and strengthen on hover (24 Oct 2026, user: "reduce the border of elements"): hairline 4.5% white, secondary buttons 8% white over a 2.5% fill (14% on hover), inputs `--line-2` (`--line-strong` on hover), dialogs 8%, danger frame 20% and danger button 45% of `--danger` (full on hover). **Controls:** every on/off setting is a switch (`input.toggle`, role `switch`; rounded-rect radius 6, not a pill; thumb springs across, stretches on press); selection checkboxes (rows, tracks) are an 18px rounded-rect box whose check mark springs in. A hover never repaints a selected/checked state (`check.py` “hover keeps selected”). **Motion for React** (M9 WI 3, user 24 Oct 2026) drives data that arrives or changes, CSS keeps the states: Home sections rise in once on arrival (fade + 14 px, 500 ms, 70 ms apart); the 12 month bars and rank bars grow on a spring (bounce 0.15, 450 ms, staggered 30 to 35 ms), as do the now/before bars and the #1 share ring (900 ms); the month value tip springs between bars; Home cards carry the Settings pointer spotlight plus a faint surface glow at the pointer; tab panels and the period heading swap through `AnimatePresence` (panel 320 ms fade + 18 px from the picked side, heading 180 ms ±8 px); a new Now-playing track slides in 280 ms. Everything collapses to instant under `prefers-reduced-motion` (CSS rule in `tokens.css`; `MotionConfig reducedMotion="user"` stops transforms only, so `useReducedMotion` also skips the clip-path and bars, makes `AnimatePresence` swaps `duration: 0`, and `jump`s springs; `check.py` "reduced motion (JS)" enforces it). Home month bars are `--bar`; only the month in focus is accent (the "active bar").

**Interaction states** (every interactive component draws all that apply):
| State | Treatment |
|---|---|
| Default | As specified per component |
| Hover | Surface one step lighter (`--surface` → `--surface-2`) or `--elev-1-hover`; text unchanged |
| Pressed | `scale(0.98)`; primary button gradient flips darker (`#F0A92E` → `#E39A1F`) |
| Focus (keyboard) | 2px `--accent` outline, 2px offset; never removed |
| Selected | Light fill (`#ECEDEF` on dark text) for value segments; nav and current list row: bright bold label + accent icon + 2×20 px accent marker, no fill; accent underline for page-level tabs (`Seg line`); accent fill + ink check for checkboxes; accent track + ink thumb for switches |
| Disabled | 40% opacity, no hover, `cursor: not-allowed`, still announced |
| Loading | Label replaced by a 3-dot progress (no spinner); width kept |
| Error | `--danger` border + message below the control |

## 4. Layout
- **Shell:** left rail 216 px + main content (max 1240 px, fluid). Rail (M9 redesign, 24 Oct 2026): app icon (the `src-tauri/icons/icon.svg` master; turns a little on hover) + wordmark and a collapse toggle; a full-width **Quick add** row with its shortcut (Ctrl+N made visible); nav grouped by what you're doing: Home, then *Listening* (Recent plays, Insights), *Collection* (Backlog, Library, Rediscover with mono counts), *Spotify* (Playlists, From Spotify), group names in 12 px `--faint`, never uppercase; icons always shown; the current page has no fill (user, 24 Oct 2026: the grey pill + accent edge read as generic): its label goes bright and bold, its icon takes the accent, and a short 2×20 px accent marker sitting on the rail's divider springs to it (360 ms); list rows use the same marker; **Settings** sits apart at the foot; one status card at the bottom: **Now playing** (live dot, track, artist; on every screen, opens Home) above data freshness. **Collapsible:** the toggle or Ctrl+B collapses the rail to 72 px icons (width eases 240 ms; the choice is remembered on this PC); below 1100 px it is always collapsed and the toggle hides. Collapsed: icons with tooltips, groups become hairline dividers, the status card hides.
- **Top bar** in main: one combined field “Search your library, or paste a Spotify link to add it” + Add, and the page’s range or filter control.
- Window minimum 900 × 600 (TSD tauri.conf).

## 5. Components (M2 builds these first)
| Component | Notes |
|---|---|
| Nav item | label + mono count; selected = `--surface-2` + bold |
| Combined search/add field | detects a Spotify link on paste and switches the button to “Add” |
| Stat tile | label, mono value, delta line (accent only when it asks for action) |
| Rank bar row | rank, name (+ artist for albums/tracks), bar without track, mono hours over your status and stars; unsaved rows: accent bar + a Save icon button. Home (M9) shows #1 as a lead (hours, plays, share of your time, your opinion + action) and 2 to 10 in two columns |
| Record card (default) | text-first: type, title, artist, your status in mono (accent if actionable), Revisit badge. No artwork |
| Artwork tile (artwork mode) | a compact **Spotify embed slot** in place of art (the embed is the only artwork source in the MVP, BRD 5.3, FR-017); at most 20 per set; each set ends with the Spotify logo and “Open Spotify”. Off by default, toggled per list with “Show artwork” |
| Play row | thumb, track, artist, your stars or a Rate button, mono relative time |
| Status control | Planned · In progress · Explored · Not for me · Abandoned (segmented); Revisit is a separate switch |
| Rating | 5 stars with halves; keyboard: arrows ±0.5 |
| Tag chip / add tag | radius 10 |
| Embed slot | Spotify iframe when online; neutral placeholder when offline (TSD §4.3); never cropped or overlaid |
| Attribution | Spotify logo + “Open Spotify” wherever Spotify-sourced content appears outside the embed |
| Freshness note | sources + dates, on every listening-data screen |
| Name-needed badge | for phone captures (Runout Inbox or Wi-Fi Shortcut) without a confirmed name |
| Empty state | one sentence + one action |
| Toast | transient confirmations only, with Undo where the action can be undone; errors stay inline |
| Skeleton | shapes of the final layout, one shimmer pass; no spinners |
| Live dot | 8px accent dot that pulses; only on Now playing (semantic: logger is receiving) |
| Embed slot actions | quick actions appear **below** the slot, never on top of it (artwork must not be overlaid) |
| Source tag | “From: Your records + Desktop log + Spotify export” on every card built from listening data (FR-032) |
| Rating control | `role="slider"`, 0.5 steps, arrow keys; visible focus |
| Skipped play | text in `--faint`, never dimmed with opacity (contrast) |
| Buttons (M9 rule) | `btn-primary`: the single main action of a page or dialog; `btn-light`: a card's or section's main action when the page has its primary (or none); `btn-secondary`: every other action; `btn-ghost`: inline text action beside a value; `btn-danger`: destructive. `.s` only in rows, lists, banners, the selection bar or beside a value. Same intent, same variant, size and label everywhere |
| Switch (M9) | `Switch` (`input.toggle`, role `switch`): every on/off setting (Settings, Item, Artwork, Revisit only, Private playlist). Label left, switch right in a `.switch-row` with a hover fill; flips at once, rolls back if the save fails |
| Selection checkbox (M9) | 18px rounded-rect box, accent fill + ink check that springs in; for choosing rows or tracks, never for a setting |
| Page tabs (M9) | `Seg line` + a keyed `.tab-panel` that slides in from the picked side (`useSlide`): Settings, Insights groups, Playlists mode. List filters stay filled segments |
| Disclosure (M9) | `.st-details`: caret rotates, content height animates (`::details-content`) |
| Copy button (M9) | `CopyButton`: icon button beside a value the user types elsewhere (pairing code, capture URL, address, paths); check-mark pop + status message |
| Count-up number (M9) | `CountUp`: stat and metric numbers count up once on first view (keeps “6,519” grouping); screen readers get the final value |

The **design system page** (`#/system`; in the prototype for M1–M2, in the app since) shows every token and component above in all its states. From M8 the app's `src/styles` is the reference; the prototype stays as the M1 record and is not updated.

**Non-text contrast (decision, 24 Oct 2026):** at the user's request borders are quiet at rest (`--line-2` inputs ≈ 1.5:1, checkbox ring ≈ 1.8:1 on `--bg`), below WCAG 1.4.11's 3:1. Each control is still identified by its label, placeholder or fill, and hover/focus raise the edge (focus ring 2px accent). Revisit if accessibility needs change.

## 6. Screen inventory (M1 designs all of these)
Source of truth: the clickable prototype in [design/prototype/](design/README.md), driven by one mock dataset (`scripts/gen_mock_data.py`). Screenshots in `design/screens/`.

| ID | Screen | Release | Capabilities | Prototype route |
|---|---|---|---|---|
| SC-01 | Home, full (listening + opinions) | R2 | CAP-12, 18, 23, 25, 26 | `#/home` (release R2/R3) |
| SC-02 | Home, MVP state and first run | MVP | CAP-01, 09, 11, 12, 13 | `#/home` (release MVP), `#/first-run` |
| SC-03 | Backlog (incl. phone captures waiting for a name) | MVP | CAP-06, 07, 09, 13, 36 | `#/backlog` |
| SC-04 | Library and search results | MVP | CAP-02, 10, 17 | `#/library/{album,track,artist}`, `#/search/<q>` |
| SC-05 | Item detail: album, track, artist; edit mode | MVP | CAP-02, 04, 05, 06, 07, 08, 34 | `#/item/<id>`, `?edit=1` |
| SC-06 | Quick add: link, manual, naming, duplicate, unsupported, offline, R3 autofill | MVP | CAP-01, 03, 13, 31 | dialog (`Ctrl+N`) |
| SC-07 | Rediscover, with filters and Make a Spotify playlist (R3) | MVP | CAP-11, 30 | `#/rediscover` |
| SC-08 | Recent plays, unmatched plays, logger | R2 | CAP-18, 20, 21 | `#/plays/{all,unrated,missing}` |
| SC-09 | Insights: Listening, Collection, Your take, Year in review | R2 | CAP-23–27 | `#/insights/{listening,collection,take,year}` |
| SC-10 | Import Spotify export (3 steps) | R2 | CAP-19 | `#/import/{1,2,3}` |
| SC-11 | Playlists: check-up (R2) and Studio with apply and undo (R3) | R2/R3 | CAP-22, 29, 30 | `#/playlists/{checkup,studio}` |
| SC-12 | From Spotify: Top items and recent plays | R3 | CAP-32, 33, 34 | `#/spotify/{tracks,artists}` |
| SC-13 | Settings and data, tags, connect, restore, delete, **Runout Inbox (R3, M7)**, Shortcut steps (optional fallback) | MVP–R3 | CAP-13, 14, 15, 16, 28, 35, 36 | `#/settings`, `#/settings/tags` |
| — | Tray menu, uninstall dialog, design system | R2 / MVP | CAP-16, 18 | `#/tray`, `#/uninstall`, `#/system` |

Every screen renders its **loading**, **empty**, **offline** and **error** states from the prototype controls.

## 7. Compliance rules for every screen
- Spotify embed shown unaltered; ≤ 20 artwork items per set, each set ending with “Open Spotify” (S10).
- Source-A screens (SC-12, Playlist Studio, autofill) carry attribution and never show your listening stats.
- Insights never show Spotify Top items or API recent plays (FR-031, FR-041).
- No Spotify green, logo look-alikes or Spotify layout patterns.
- Dashed “Spotify logo” and “Spotify embed” slots in the prototype are placeholders: M2 uses the official logo file from Spotify’s brand resources, unaltered, at the minimum sizes in the Design Guidelines (logo ≥ 70 px, icon ≥ 21 px).
- Text contrast ≥ 4.5:1 (NFR-12); touch targets ≥ 32 px desktop.

## 8. Decisions
| # | Decision |
|---|---|
| D29 | Direction D · Pulse; dark only in v1; amber accent; Schibsted Grotesk + JetBrains Mono, self-hosted |
| D30 | Delivery order: design (M1) → UI on mock data (M2) → backend (M3–M5) → integration (M6–M7) |
| D33 | Phone capture = Runout Inbox (BRD D33). SC-13 gets an **Inbox** section in the Spotify area, designed and built in M7 with existing components only: state line (“Runout Inbox · ready” / “missing” / “not created yet” / “connect Spotify first”), last check + count imported (mono), **Check now** (secondary button), **Create Inbox** (only when missing or not created yet while connected, e.g. creation failed at sign-in; API.md `create_inbox`; never automatic; a Check now that finds the Inbox deleted says so inline and offers it), two toggles (import saved albums; import followed artists — notes the one-time re-approval), one sentence on how to use it from the phone. The Wi-Fi Shortcut steps move under a collapsed “Optional: instant capture on home Wi-Fi”. Imported items reuse the name-needed badge; no new tokens. |
