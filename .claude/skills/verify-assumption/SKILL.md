---
name: verify-assumption
description: Resolve a Runout assumption — a TSD "⚑ verify", a TSD §10 T-n, a BRD §18 O-n, or any external fact the code relies on — by testing or official docs, then record it in docs/kb and the spec. Use when one is due in the current milestone or before coding against an external API.
---

# Verify an assumption

## Process
1. State the question in one line and where it comes from (TSD §/T-n/O-n/S-id).
2. Check `docs/kb/` — already answered and recent? Done; cite it.
3. Needs the user (login, device, legal) → it's a hard stop: point to the `U-n` in `docs/USER_ACTIONS.md` and stop.
4. **Test** if possible (curl, tiny probe, run the app). Else read the **official** doc. Third-party sources only as last resort, marked Unverified.
5. Record in the right `docs/kb/*.md` file: fact — status — date — source. Update the open-questions table in `docs/kb/README.md`.
6. Update the spec: remove the `⚑ verify` marker / close the T-n, adjusting the TSD text to the real answer. BRD changes → `change-spec` skill.
7. Answer changes scope or compliance → hard stop; report to the user.

## Toolbox
- `docs/kb/README.md` (rules, open questions), `docs/kb/spotify.md`, `docs/kb/windows-tauri.md`.
- Official docs: developer.spotify.com, v2.tauri.app, learn.microsoft.com, sqlite.org.

## Proof
- [ ] KB entry has status, date and source URL (or the test command + result).
- [ ] Open-questions row updated.
- [ ] No `⚑ verify` left for this item in TSD (`grep -n "⚑" docs/TSD.md`).
- [ ] No real personal data pasted.

## Gotchas
<!-- Lessons from corrections go here, newest first. -->
