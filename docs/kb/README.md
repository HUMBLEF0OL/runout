# Knowledge Base — Runout

Facts the specs rely on, and facts learned while building. The BRD says *what*, the TSD says *how*; the KB says **what is true out there** (Spotify, Windows, Tauri) and **what we found out**.

## Rules
- One fact per bullet: the fact, **status**, date checked, source.
  Status: **Verified** (tested or official doc) · **Documented** (official doc, not tested) · **Interpretation** · **Unverified**.
- Re-checking a fact updates its date; a fact that turned out wrong is corrected in place, with the old value noted.
- Resolving a TSD `⚑ verify`, a TSD §10 `T-n` or a BRD §18 `O-n` → record it here *and* update the spec (`verify-assumption` skill).
- Never paste real personal values (export rows, tokens, IPs). Structure only.

## Files
| File | Covers |
|---|---|
| [spotify.md](spotify.md) | Links, embeds/oEmbed, Web API limits, data export, local client data |
| [windows-tauri.md](windows-tauri.md) | SMTC, Tauri 2, WebView2, NSIS, Credential Manager, SQLite |
| [perf-integrity.md](perf-integrity.md) | Measured NFR-04 timings, NFR-05 kill tests, NFR-11 backup size |

## Open questions (single index)
| ID | Question | Resolve in | Owner | Status |
|---|---|---|---|---|
| T1 / O2 | Real export file + field names; delivery time and request frequency | M4 | U-1, U-2 then agent | T1 **resolved** 2026-10-19 ([spotify.md](spotify.md#extended-streaming-history-format-t1--verified-2026-10-19)); O2 **partly resolved**: Extended streaming history took ~1 day (BRD S14); Account data part (`Playlist*.json`) not received and allowed request frequency unknown, still open |
| T2 | NSIS update-mode detection for cleanup hook | M6 | agent | **resolved** 2026-10-20 (template: `$UpdateMode` set before `NSIS_HOOK_PREUNINSTALL`) and **verified on the installed build** 2026-10-21: unticked, ticked and `/UPDATE` cases all as specified ([windows-tauri.md](windows-tauri.md#gotchas-found-while-building), MILESTONES M6) |
| T3 | Redirect-URI rules, `/me/library` scope | M5 | agent | **resolved**: redirect 2026-10-19 (live); `/me/library` request shape + scopes documented 2026-10-21 ([spotify.md](spotify.md#web-api-release-3)) |
| T4 | Incognito WebView2 works with embeds and keeps no cache | M2 | agent | **resolved** 2026-10-18 (wry probe; [windows-tauri.md](windows-tauri.md#webview2)); in-app re-check in M2 |
| O1 | Student Premium qualifies for Development Mode? | M5 | U-6 then agent | resolved 19 Oct 2026: yes ([spotify.md](spotify.md)) |
| O4 / O5 | Snapshot retention 30 d / cache expiry 7 d | M5 | agent | resolved 20 Oct 2026: built as proposed (TSD §5.2) |
| O7 | Spotify support on export-data analysis (optional) | before M5 | U-8 | optional |
| T5 | Runout Inbox: playlist item removal shape, followed-artists endpoint + `user-follow-read` in Dev Mode, album-batch rule (TSD §5.3, BRD D33) | M7 | agent, then U-10 live | **partly resolved** 2026-10-21: shapes and Dev Mode availability documented ([spotify.md](spotify.md#web-api-release-3)); live check and album-batch rule open (U-10) |

## Glossary
| Term | Meaning |
|---|---|
| **U / S / E / A** | Data sources: User-authored · desktop logger (SMTC) · Spotify data Export · Spotify plAtform (embed/Web API) |
| **Promotion** | A Spotify-provided value becomes user data only when the user confirms it; stored with provenance `promoted` (TSD §4.2) |
| **Play** | `ms_played ≥ 30000`, any source (D25) |
| **Reconcile** | Linking a logger event to its export twin so it isn't counted twice (TSD §4.7) |
| **Hard stop / U-n** | Step only the user can do ([USER_ACTIONS](../USER_ACTIONS.md)) |
| **CAP / FR / NFR / D / S / O / T** | Capability · functional req · non-functional req · decision · Spotify constraint · BRD open item · TSD open item |
