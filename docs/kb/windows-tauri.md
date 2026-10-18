# Windows, Tauri 2, SQLite — knowledge base

Format: fact — **status**, date, source. Design decisions live in [TSD](../TSD.md); this file holds the platform facts and gotchas found while building.

## Toolchain on the dev PC
- Rust 1.98 + MSVC, Node 26, WebView2 runtime, Python 3.10 installed; no new installs needed for Tauri 2 — **Verified**, 2026-10-18, TSD §1.

## SMTC (`Windows.Media.Control`)
- Entry point `GlobalSystemMediaTransportControlsSessionManager::RequestAsync()`; per-session `MediaPropertiesChanged`, `PlaybackInfoChanged`, `TimelinePropertiesChanged` events — **Documented**, https://learn.microsoft.com/en-us/uwp/api/windows.media.control
- Not every app raises every event → 5 s timeline poll fallback — **Interpretation**, TSD §4.5; confirm in the M4 SMTC spike.

## Tauri 2
- App data paths must come from the path API (`app_data_dir`, `app_local_data_dir`) so the uninstaller deletes the same folders — **Documented**, TSD §2. Docs: https://v2.tauri.app/
- Capabilities restrict which commands/plugins the WebView may call; Runout grants only its own commands (no shell/fs plugin) — **Documented**, TSD §7. https://v2.tauri.app/security/capabilities/
- NSIS uninstaller "delete app data" checkbox is off by default and skipped in update mode; `NSIS_HOOK_PREUNINSTALL` available — **Documented**, TSD §4.11; update-mode detection **Unverified (T2)**.

## WebView2
- Cross-origin iframes don't report load failure reliably → Rust `net::online()` decides whether to mount embeds — **Interpretation**, TSD §4.3.
- Whether embeds keep a Spotify login inside WebView2 — **Unverified (T4)**, check in M2.

## Credentials
- Tokens go to Windows Credential Manager via the `keyring` crate, target `Runout/spotify` — **Documented**, TSD §2.

## SQLite
- `VACUUM INTO` gives a consistent compacted snapshot for backups — **Documented**, https://www.sqlite.org/lang_vacuum.html
- FTS5 ships in `rusqlite` `bundled` builds — **Documented**; confirm feature flag in M3.

## Gotchas found while building
<!-- newest first: date — what bit us — fix / playbook updated -->
