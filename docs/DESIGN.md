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
`DESIGN_VARIANCE 5 · MOTION_INTENSITY 5 · VISUAL_DENSITY 6`. Product UI, so no marketing-page patterns (heroes, logo walls, marquees).

What “not flat” means here, and nothing more:
1. **Three depth layers** (canvas, raised, overlay) built from a 1px light hairline, an inner top highlight and a soft shadow tinted to the background. No glassmorphism, no neon glow.
2. **One ambient moment:** the Now-playing card picks up a faint radial tint of the playing album’s dominant color (8% opacity, behind the card only). Everywhere else stays graphite.
3. **Every interactive element has five drawn states:** default, hover, pressed, focus, disabled (plus loading where it applies).
4. **Motion with a reason:** feedback (press), state change (segment indicator slides, row enters), live status (the Now-playing pulse, the only looping animation).
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
| `--line-strong` | `#3A3F46` | Secondary button and chip borders |
| `--text` | `#ECEDEF` | Primary text |
| `--text-2` | `#C3C7CE` | Secondary text |
| `--muted` | `#A9AEB7` | Labels, metadata (≥ 7:1 on `--bg`) |
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
| `--elev-1` | surface `linear-gradient(180deg, #1A1C20, #16181B)`, border `1px rgba(255,255,255,0.06)`, `inset 0 1px 0 rgba(255,255,255,0.04)`, shadow `0 8px 24px rgba(0,0,0,0.28)` | Cards, list containers |
| `--elev-1-hover` | border `rgba(255,255,255,0.12)`, `translateY(-1px)`, shadow `0 12px 28px rgba(0,0,0,0.34)` | Hovered cards, rows, tiles |
| `--elev-2` | `#1C1F23`, border `rgba(255,255,255,0.10)`, shadow `0 24px 64px rgba(0,0,0,0.5)` | Dialogs, menus, toasts, selection bar |
| `--accent-fill` | `linear-gradient(180deg, #F8C566, #F0A92E)`, `inset 0 1px 0 rgba(255,255,255,0.35)`, shadow `0 2px 8px rgba(245,181,68,0.18)` | Primary button |
| `--indicator` | `inset 3px 0 0 var(--accent)` | Selected nav item |

**Shape:** radius 10 for controls and list rows, 16 for cards, 6 for thumbnails, 3 for bars. Pills are not used.
**Spacing:** 4 · 8 · 12 · 16 · 20 · 24 · 32. Page padding 32 desktop.
**Focus:** 2px `--accent` outline, 2px offset, on every interactive element.
**Icons:** Phosphor (`@phosphor-icons/react`), regular weight, 20px; icon-only buttons have `aria-label`.
**Motion:** curve `cubic-bezier(0.16, 1, 0.3, 1)`. Hover 150 ms; press `scale(0.98)` 100 ms; segmented-control indicator slides 220 ms; list insert/remove 200 ms (fade + 4px rise); dialog 180 ms (fade + scale from 0.98); toast slides up 220 ms. The Now-playing live dot pulses (2 s, opacity only) and is the only loop. Skeletons shimmer once per 1.6 s. Everything collapses to instant under `prefers-reduced-motion`.

**Interaction states** (every interactive component draws all that apply):
| State | Treatment |
|---|---|
| Default | As specified per component |
| Hover | Surface one step lighter (`--surface` → `--surface-2`) or `--elev-1-hover`; text unchanged |
| Pressed | `scale(0.98)`; primary button gradient flips darker (`#F0A92E` → `#E39A1F`) |
| Focus (keyboard) | 2px `--accent` outline, 2px offset; never removed |
| Selected | Light fill (`#ECEDEF` on dark text) for segments; `--indicator` for nav |
| Disabled | 40% opacity, no hover, `cursor: not-allowed`, still announced |
| Loading | Label replaced by a 3-dot progress (no spinner); width kept |
| Error | `--danger` border + message below the control |

## 4. Layout
- **Shell:** left rail 208–220 px (logo, nav with counts, data-freshness box) + main content (max 1200 px, fluid). Rail collapses to icons below 1100 px window width.
- **Top bar** in main: one combined field “Search your library, or paste a Spotify link to add it” + Add, and the page’s range or filter control.
- Window minimum 900 × 600 (TSD tauri.conf).

## 5. Components (M2 builds these first)
| Component | Notes |
|---|---|
| Nav item | label + mono count; selected = `--surface-2` + bold |
| Combined search/add field | detects a Spotify link on paste and switches the button to “Add” |
| Stat tile | label, mono value, delta line (accent only when it asks for action) |
| Rank bar row | rank, name + your-status suffix, bar without track, mono count |
| Record card (default) | text-first: type, title, artist, your status in mono (accent if actionable), Revisit badge. No artwork |
| Artwork tile (artwork mode) | a compact **Spotify embed slot** in place of art (the embed is the only artwork source in the MVP, BRD 5.3, FR-017); at most 20 per set; each set ends with the Spotify logo and “Open Spotify”. Off by default, toggled per list with “Show artwork” |
| Play row | thumb, track, artist, your stars or a Rate button, mono relative time |
| Status control | Planned · In progress · Explored · Not for me · Abandoned (segmented); Revisit is a separate toggle |
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

The **design system page** (`#/system` in the prototype) shows every token and component above in all its states; it is the visual reference for M2.

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
| D33 | Phone capture = Runout Inbox (BRD D33). SC-13 gets an **Inbox** section in the Spotify area, designed and built in M7 with existing components only: state line (“Runout Inbox · ready” / “missing” / “connect Spotify first”), last check + count imported (mono), **Check now** (secondary button), **Create Inbox** (only when missing), two toggles (import saved albums; import followed artists — notes the one-time re-approval), one sentence on how to use it from the phone. The Wi-Fi Shortcut steps move under a collapsed “Optional: instant capture on home Wi-Fi”. Imported items reuse the name-needed badge; no new tokens. |
