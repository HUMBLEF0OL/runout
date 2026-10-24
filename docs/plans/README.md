# Implementation plans — Runout

One plan per milestone, written before it starts (MILESTONES spec-driven rule). Order is three strict phases (BRD D30, confirmed by the user 18 Oct 2026):
1. **Frontend** — M2 builds the whole UI for all releases on mock data.
2. **Backend** — M3, M4, M5 implement the whole contract in Rust. Starts only when M2 is done.
3. **Integration** — M6, M7 switch the UI to the real commands. Starts only when M3, M4 and M5 are all done (or a backend milestone is formally stopped, e.g. R3 after a failed O1 gate).

Work is agentic (BRD D31): each milestone is hours of parallel agent build and external review, gated by the user actions and real-use periods it names. No phase overlaps the next. Inside the backend phase, M4 and M5 may run in parallel once M3 is done.

| # | Plan | Builds | Starts when | Unblocks |
|---|---|---|---|---|
| 1 | [M1-design.md](M1-design.md) | Designs (done: prototype + screens) | — | M2 after compliance pass, contrast check, **U-9** |
| 2 | [M2-ui-build.md](M2-ui-build.md) | **Frontend phase:** entire UI, all releases (MVP, R2, R3), on mock data | U-9 approved | backend phase |
| 3 | [M3-backend-mvp.md](M3-backend-mvp.md) | Backend phase: Rust core for API.md §3 | M2 done | M4, M5 |
| 4 | [M4-listening-insights.md](M4-listening-insights.md) | Backend phase: Rust for API.md §4 | M3 done (WI 4 needs U-2) | integration phase |
| 5 | [M5-spotify-connected.md](M5-spotify-connected.md) | Backend phase: Rust for API.md §5 | M3 done + O1 gate | integration phase |
| 6 | [M6-integration-mvp.md](M6-integration-mvp.md) | Integration phase: MVP screens on real commands, installer | M3, M4, M5 all done | M7 (U-5 in parallel, D32; U-3/U-4 optional, D33) |
| 7 | [M7-integration-r2-r3.md](M7-integration-r2-r3.md) | Integration phase: R2/R3 screens on real commands **+ Runout Inbox** (phone capture, D33) | M6 done (apart from U-5) | M8 |
| 8 | [M8-stabilization-ui.md](M8-stabilization-ui.md) | Stabilization & UI Polish: end-to-end functionality hardening, gap resolution, UI streamline via taste skill | M7 done | M9 |
| 9 | [M9-motion-ui.md](M9-motion-ui.md) | Interactive UI refinement: taste skill at motion 7, screen by screen (Settings first) | M8 UI audit done | — |

```
 FRONTEND            BACKEND                      INTEGRATION
 M1 ─U-9─► M2 ─────► M3 ─┬─► M4 ─┐               
 (all screens,           └─► M5 ─┴─(all done)──► M6 ───────► M7 (+ Runout Inbox, D33) ──► M8 (Stabilize & Taste UI)
  mock data)              (M4 ∥ M5)
```

## Why the UI can go first safely
- [API.md](../API.md) fixes every command, type and event **before** M2. M2 mocks it; M3–M5 implement the same names; M6–M7 flip `api.ts` from mock to `invoke` per command. No page code changes at switch-over.
- The mock data (`docs/design/prototype/mock-data.json`) is shaped like TSD §3, so mocks already behave like the real data.

## Rules for backend milestones (M3–M5)
- **The UI is done; the backend serves it.** Implement API.md exactly. Do not change pages or components except to fix a bug the real data exposes.
- A contract change is a last resort: change API.md first, then `api.ts` types + the mock + Rust in one change, and say why in the plan's record step.
- Every backend work item ends with its commands still passing the UI's Vitest suite in mock mode (nothing in `src/` breaks).
