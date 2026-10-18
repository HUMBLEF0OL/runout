# Runout — agent guide

Local-first Windows music notebook for Spotify. Tauri 2 (Rust core) + React/TS/Vite + Tailwind v4 UI + SQLite (rusqlite, FTS5). Releases: MVP · R2 listening log + insights · R3 Spotify Web API. Delivery order (BRD D30): M1 design → M2 UI on mock data → M3–M5 backend → M6–M7 integration. Each milestone starts from current specs and a plan in `docs/plans/`.

## Start here (every session, before any development)
1. [docs/MILESTONES.md](docs/MILESTONES.md) → **Status** block (where the project is, what's next) and **Known gaps** (G-n: deferred items with their owner milestone). Pick up any G-n owned by the milestone you're in.
2. The current milestone's plan in [docs/plans/](docs/plans/README.md) — its steps, parallelism and rules.
3. [docs/USER_ACTIONS.md](docs/USER_ACTIONS.md) — which hard stops are open; don't start an item that waits on one.
4. [docs/SOURCE_MAP.md](docs/SOURCE_MAP.md) for where code lives, then the TSD/API.md sections the item names.
5. Git: work on `dev`; commit only (no remote, never push); merge `dev` → `master` only when the user asks.

## Source of truth
- [BRD.md](BRD.md) — what and why. Generated from `BRD.docx`; never edit the .md (`change-spec` skill).
- [docs/DESIGN.md](docs/DESIGN.md) — look and behaviour (direction D · Pulse) · [docs/TSD.md](docs/TSD.md) — how · [docs/API.md](docs/API.md) — UI ↔ Rust command contract · [docs/MILESTONES.md](docs/MILESTONES.md) — work items + acceptance · [docs/CAPABILITIES.md](docs/CAPABILITIES.md) — CAP ↔ FR.
- [docs/SOURCE_MAP.md](docs/SOURCE_MAP.md) — where each capability lives in code, its spec and tests.
- [docs/kb/](docs/kb/README.md) — verified external facts, open questions, glossary, gotchas.
- [docs/USER_ACTIONS.md](docs/USER_ACTIONS.md) — the only hard stops.

## How work is done: one playbook per job
Each recurring job has a playbook (`.claude/skills/<job>/SKILL.md`): **process** · **toolbox** · **proof** · gotchas.

| Job | Playbook | Usually run by |
|---|---|---|
| Implement one milestone work item | `build-work-item` | `rust-core` / `react-ui` agent |
| Review a change against spec + Spotify/privacy rules | `compliance-review` | `spec-reviewer` agent (read-only) |
| Resolve a ⚑ verify / T-n / O-n / external fact | `verify-assumption` | `spotify-researcher` agent |
| Change requirements or design across the spec set | `change-spec` | main session |
| Verify and close a milestone | `close-milestone` | main session |
| Turn a correction into a playbook change | `capture-lesson` | main session |

Agents live in `.claude/agents/`; each preloads its playbook.

## Development flow
1. **Pick** the next unchecked work item in MILESTONES (current milestone first). Due `⚑ verify`/T-n for it → `spotify-researcher` first.
2. **Build** → delegate to `rust-core` and/or `react-ui` (parallel when independent; Rust command first if UI needs it).
3. **Prove** → `python scripts/check.py` (also enforced by the Stop hook).
4. **Review** → `spec-reviewer`. Fix findings; mechanically detectable ones become checks (`capture-lesson`).
5. **Record** → SOURCE_MAP status, KB facts/gotchas, TSD if reality differed. Update the MILESTONES **Status** block; remove fixed **Known gaps** rows (name the G-n in the commit) and add any new deferral as a G-n row.
6. All items done → `close-milestone`. Hit a `U-n` → stop, point the user to it, continue other items.

## Proof
`python scripts/check.py` must exit 0: Rust fmt/clippy/tests, tsc/Vitest, allowed hosts, no secrets in logs, UI purity (no network/SQLite in `src/`), contract (every `api.ts` field is in API.md), Spotify attribution, source map coverage.
UI-facing changes also run `node scripts/journeys.mjs` (needs Vite on :1420): keyboard-only journeys A–H, focus rings, accessible names; screenshots in `docs/design/screens/m2-journeys/`. A Stop hook in `.claude/settings.json` runs it after every turn and keeps Claude working until it passes.

## The loop: fix the playbook, not the chat
When the user corrects you, or a failure will recur: fix the work, then run `capture-lesson` — put the rule in its strongest home (automated check > proof item > process step > gotcha). Say in one line which file changed. Project lessons go in versioned files, not auto memory; auto memory is for facts about the user.

## Working rules
- Decide autonomously; stop only at USER_ACTIONS hard stops, money, or scope/compliance changes.
- All data access and network in Rust; UI calls Tauri commands only (TSD §1).
- Contract changes: API.md first, then `src/api.ts`, `src/mocks/` and Rust in the same change (API.md §7).
- UI styling is Tailwind v4 with only the DESIGN §3 tokens (`src/styles/tokens.css` `@theme`, default palette removed); multi-state recipes live in `src/styles/app.css` `@layer components`. No static inline styles; no `]${` in class strings.
- Free text is sanitized once in `src/api.ts` (`src/lib/sanitize.ts`); Spotify URLs only via `safeUri`. Rust validates again.
- Never commit real Spotify export values, tokens, or oEmbed responses. Never read Spotify client files (BRD S11).
