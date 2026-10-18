---
name: capture-lesson
description: Turn a correction or failure into a permanent playbook change ("fix the playbook, not the chat"). Use right after the user corrects Claude, a review finding recurs, or a check fails for a reason that will happen again.
argument-hint: "[what went wrong]"
---

# Capture a lesson

## Process
1. Name the lesson in one line: what went wrong, and the rule that prevents it.
2. One-off (won't recur) → stop; nothing to capture.
3. Pick the strongest home that fits, in this order:
   1. **Automated check** in `scripts/check.py`, if a script can detect it.
   2. **Proof checklist** item in the owning skill.
   3. **Process step** in the owning skill.
   4. **Gotchas** entry (dated) in the owning skill, or `docs/kb/*` if it's an external platform fact.
   5. `CLAUDE.md` only if it applies to every job.
4. No playbook owns the job and it will recur → create `.claude/skills/<job>/SKILL.md` with process / toolbox / proof / gotchas, and add it to the CLAUDE.md table.
5. Say in one line which file changed.

## Toolbox
- Playbooks: `.claude/skills/*/SKILL.md`; agents: `.claude/agents/*.md`; proof: `scripts/check.py`; facts: `docs/kb/`.
- Facts about the user (preferences, not project rules) → auto memory instead.

## Proof
- [ ] The lesson is in a versioned project file, not only in chat.
- [ ] New check → `python scripts/check.py` still exits 0 on the current tree, and fails on a planted violation.

## Gotchas
<!-- Lessons from corrections go here, newest first. -->
