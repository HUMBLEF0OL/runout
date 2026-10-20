# Plan: M7 Integration — R2 and R3

Specs: [MILESTONES M7](../MILESTONES.md#m7--integration-r2-and-r3) · [API.md §4–6](../API.md) · TSD §4.9, §5.2, §6 · BRD §13.2, §13.3, journeys F and G, FR-031, FR-032, FR-041, FR-042 · DESIGN §7.
Agents: `react-ui` (switch-over), `rust-core` (fixes), `spec-reviewer` (compliance on the running app).

**Entry:** M6 done apart from U-5 (so M4 and M5 are already done — integration starts only after the whole backend). U-5 daily use runs in parallel (BRD D32, 20 Oct 2026). If M5 was stopped at the O1 gate, only the R2 half runs.

## Steps
1. **Switch R2 (WI 1).** Add API.md §4 commands to the real set in `api.ts`; wire `logger-status` and `play-logged`; remove R2 data from `src/mocks/`. Run journey F on the user’s real imported history: import → insights (all four tabs) → recent plays → rate/save a play → unmatched list → link.
   - Proof: every insight card renders `sources` + `freshness` (Vitest on the card component with a response lacking them → test fails); freshness box in the rail shows real dates.
2. **Switch R3 (WI 2).** Add API.md §5 commands; wire `spotify-status`. Run journey G on the **test playlist**: check-up → Studio → cut 3, dedupe 1 → preview → apply → verify in Spotify → undo. Then: playlist from a saved filter; autofill on paste with confirm; From Spotify top items + recent plays, “Add to backlog” → promoted record (FR-041); save then remove an album in the Spotify library (FR-042).
3. **Runout Inbox (WI 4, D33).** API.md §5 contract first (TSD §5.3), then `spotify/inbox.rs` (create on connect, check, import as “name needed”, album-batch collapse, remove imported), a check timer in `lib.rs`, the Settings section, mocks. Live proof on a real phone on mobile data: track → backlog; whole album → one album item; Inbox emptied.
4. **Degradation.** Disconnect Spotify → every MVP and R2 feature still works, R3 screens show the connect state. Unplug network → MVP and R2 work. Revoke app access in Spotify account → reconnect prompt.
5. **Compliance (WI 3).** `spec-reviewer` on the running app: attribution on SC-12, Studio and autofill; no Spotify Top items or API recent plays in any insight; no listening data in Studio; design guidelines (logo sizes) met.
6. **Mock cleanup.** `src/mocks/` holds nothing imported by the production build: `VITE_MOCK` off in release config; Vite build check fails if `src/mocks` appears in the bundle graph (add to `check.py`). Keep mocks for Vitest only, or delete them.
7. **Record.** SOURCE_MAP statuses final; KB gotchas; BRD §15 metrics re-reviewed with the user after M7; `close-milestone M7`. Then M8 candidates (BRD §4.4) get their own spec + plan.

## Parallelism
1, 2 and 3 in parallel (3 after 2's `connect` wiring); 4–6 after all.

## Risks
| Risk | Response |
|---|---|
| Real data exposes insight edge cases (zero plays, huge ranges) | Add the case to M4 fixtures first, fix in Rust, then re-run journey F |
| R3 stopped at O1 gate in M5 | Run R2 half only; R3 screens stay on the connect/unavailable state |
| Embed/attribution regressions on real data | Compliance pass (step 4) is mandatory before close |

## Done when
MILESTONES M7 acceptance passes; BRD §13.2 and §13.3 criteria pass; `python scripts/check.py` exits 0.
