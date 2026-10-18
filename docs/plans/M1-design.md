# Plan: M1 Design

Specs: [DESIGN.md](../DESIGN.md) (tokens, components, SC-01–13) · [MILESTONES M1](../MILESTONES.md#m1--design) · BRD §6 modules, §8 journeys, §11 constraints.
Designs: [docs/design/](../design/README.md) (the canvas was archived there on 18 Oct 2026 and the online canvas deleted).

## Steps
1. **Keep the explorations.** Boards A–C stay as history; D · Pulse (`Home.dc.html`) becomes SC-01.
2. **MVP screens (row 2):** SC-02 Home MVP state, SC-06 Quick add + name confirm, SC-03 Backlog, SC-05 Item detail, SC-04 Library, SC-07 Rediscover. These carry the first daily-use release, so they come first.
3. **R2 screens (row 3):** SC-08 Recent plays + unmatched, SC-09 Insights, SC-10 Import.
4. **R3 + settings (row 4):** SC-11 Playlists (check-up and Studio), SC-12 From Spotify, SC-13 Settings and data.
5. **States:** every board shows its empty, offline or error state in a side panel or a second state on the same board.
6. **Walkthrough:** one step-by-step route per journey A–H in `docs/design/README.md`, followable in the prototype.
7. **Compliance pass** (`compliance-review`) over all boards; fix findings.
8. **U-9:** walk the user through; apply changes; record approval in MILESTONES.

## Rules carried from the spec
- Same shell on every screen: rail (nav + counts + freshness box) and top bar (search/add field).
- Tokens and components only from DESIGN.md §3 and §5; anything new is added to DESIGN.md first.
- Sample data stays consistent across boards (same albums, artists, counts).
- Spotify content: unaltered embed slot, attribution, ≤ 20 artwork per set, never inside insight charts.

## Done when
MILESTONES M1 acceptance passes.
