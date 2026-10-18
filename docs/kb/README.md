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

## Open questions (single index)
| ID | Question | Resolve in | Owner | Status |
|---|---|---|---|---|
| T1 / O2 | Real export file + field names; delivery time | M4 | U-1, U-2 then agent | open |
| T2 | NSIS update-mode detection for cleanup hook | M6 | agent | open |
| T3 | Redirect-URI rules, `/me/library` scope | M5 | agent | open |
| T4 | Incognito WebView2 works with embeds and keeps no cache | M2 | agent | **resolved** 2026-10-18 (wry probe; [windows-tauri.md](windows-tauri.md#webview2)); in-app re-check in M2 |
| O1 | Student Premium qualifies for Development Mode? | M5 | U-6 then agent | open |
| O4 / O5 | Snapshot retention 30 d / cache expiry 7 d | M5 | agent | proposed |
| O7 | Spotify support on export-data analysis (optional) | before M5 | U-8 | optional |

## Glossary
| Term | Meaning |
|---|---|
| **U / S / E / A** | Data sources: User-authored · desktop logger (SMTC) · Spotify data Export · Spotify plAtform (embed/Web API) |
| **Promotion** | A Spotify-provided value becomes user data only when the user confirms it; stored with provenance `promoted` (TSD §4.2) |
| **Play** | `ms_played ≥ 30000`, any source (D25) |
| **Reconcile** | Linking a logger event to its export twin so it isn't counted twice (TSD §4.7) |
| **Hard stop / U-n** | Step only the user can do ([USER_ACTIONS](../USER_ACTIONS.md)) |
| **CAP / FR / NFR / D / S / O / T** | Capability · functional req · non-functional req · decision · Spotify constraint · BRD open item · TSD open item |
