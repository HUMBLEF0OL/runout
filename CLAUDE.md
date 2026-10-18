# Runout — agent guide

Local-first Windows music notebook for Spotify. Tauri 2 (Rust core) + React/TS/Vite + Tailwind v4 UI + SQLite (rusqlite, FTS5). Releases: MVP · R2 listening log + insights · R3 Spotify Web API. Delivery order (BRD D30): M1 design → M2 UI on mock data → M3–M5 backend → M6–M7 integration. Each milestone starts from current specs and a plan in `docs/plans/`.

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
5. **Record** → SOURCE_MAP status, KB facts/gotchas, TSD if reality differed.
6. All items done → `close-milestone`. Hit a `U-n` → stop, point the user to it, continue other items.

## Proof
`python scripts/check.py` must exit 0: Rust fmt/clippy/tests, tsc/Vitest, allowed hosts, no secrets in logs, source map coverage. A Stop hook in `.claude/settings.json` runs it after every turn and keeps Claude working until it passes.

## The loop: fix the playbook, not the chat
When the user corrects you, or a failure will recur: fix the work, then run `capture-lesson` — put the rule in its strongest home (automated check > proof item > process step > gotcha). Say in one line which file changed. Project lessons go in versioned files, not auto memory; auto memory is for facts about the user.

## Working rules
- Decide autonomously; stop only at USER_ACTIONS hard stops, money, or scope/compliance changes.
- All data access and network in Rust; UI calls Tauri commands only (TSD §1).
- Never commit real Spotify export values, tokens, or oEmbed responses. Never read Spotify client files (BRD S11).
