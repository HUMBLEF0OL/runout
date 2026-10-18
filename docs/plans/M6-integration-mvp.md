# Plan: M6 Integration — MVP release

Specs: [MILESTONES M6](../MILESTONES.md#m6--integration-mvp-release) · [API.md §3, §7](../API.md) · TSD §4.11, §5.1, §7, §8 (network, performance) · BRD §13.1, §15, FR-018, FR-024, NFR-04, 07 · [USER_ACTIONS](../USER_ACTIONS.md) U-3, U-4, U-5.
Agents: `react-ui` (switch-over), `rust-core` (installer, cleanup hook), `spotify-researcher` (T2), `spec-reviewer` (final compliance).

**Entry:** integration phase ([plans README](README.md)): M2 done and the whole backend phase done — M3, M4 and M5 (or M5 formally stopped after a failed O1 gate).

## Steps
1. **Switch MVP calls (WI 1).** In `api.ts`, route every API.md §3 command to `invoke` (per-command table `REAL = new Set([...])` so R2/R3 stay mocked). Wire §6 MVP events. Remove MVP-only data from `src/mocks/`. Run journeys A–E and H in `npm run tauri dev` on an empty library, then on the 10k fixture.
   - Proof: Vitest asserts no §3 command resolves to a mock when `VITE_MOCK` is off.
2. **Phone capture (WI 2).** Settings shows LAN IP, port, code from `get_capture`; Shortcut steps screen matches TSD §5.1. U-4 firewall prompt on first enable; **U-3** user builds the iOS Shortcut and shares one song → backlog shows it as “name needed” → naming flow.
3. **Installer (WI 3, T2).** `verify-assumption` T2: NSIS update-mode detection inside `NSIS_HOOK_PREUNINSTALL`. Add `src-tauri/windows/hooks.nsh` running `runout.exe --uninstall-cleanup` unless update mode; relabel checkbox “Also delete my music library” via custom language string. Per-user NSIS bundle, unsigned.
   - Proof: the four uninstall cases in M6 acceptance, each run on a VM/fresh user profile, results recorded in KB.
4. **Success metrics (WI 4).** Settings → success-metrics view on `get_success_metrics`; matches BRD §15 targets.
5. **Release checks (WI 5)** on the installed build:
   - Offline: unplug → every MVP feature works; embeds show placeholder; short link queued then resolved.
   - Network capture (Resource Monitor or `pktmon`): idle = no outbound; normal use = allow-listed hosts + embed traffic only (NFR-07).
   - Grep app-data folders for `"html":` and `thumbnail_url` → none.
   - NFR-04: start < 3 s, navigation < 500 ms on 10k items.
   - 50 real items (5 short links, 2 offline), each opens in the Spotify app; duplicate → open existing; episode rejected; two versions linked.
6. **Compliance.** `spec-reviewer` over the running app: attribution on every Spotify-sourced screen, embeds unaltered, ≤ 20 per set.
7. **Daily use (U-5).** User uses the installed app for 2 weeks; R2/R3 stay on mocks meanwhile (no new backend work in this phase). At the end: review success metrics vs BRD §15 with the user → go/no-go for M7 recorded in MILESTONES.
8. **Record.** SOURCE_MAP (`src-tauri/windows/`), KB (T2, network capture), `close-milestone M6`.

## Parallelism
1 and 3 in parallel; 2 after 1; 4 after 1; 5–6 after 1–4; 7 last.

## Risks
| Risk | Response |
|---|---|
| NSIS update mode not detectable in hook | Fall back to `/UPDATE` flag check or registry marker written by the installer; record in KB, `change-spec` TSD §4.11 |
| Firewall blocks LAN listener on public network profile | Settings shows the profile and a hint; U-4 covers private profile |
| Success metrics miss targets | Not a release blocker for M7 work; findings go to the M7 go/no-go conversation |

## Done when
MILESTONES M6 acceptance passes, including U-3, U-4 and U-5 with the go/no-go recorded.
