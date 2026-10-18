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
- **T4 — InPrivate renders embeds and keeps no Spotify data on disk** — **Verified** (tested), 2026-10-18. Probe: `wry 0.57.0` + `tao 0.37.1` (the versions in `src-tauri/Cargo.lock`; Tauri's `incognito` maps to wry `with_incognito`), WebView2 runtime on this PC, page CSP = ours (`frame-src https://open.spotify.com`), track + album + artist iframes. Results: all three fired `load` and rendered (window-only capture); no `securitypolicyviolation` in the host page. After exit, the InPrivate UDF had an empty `Default/Cache/Cache_Data`, no `Network/Cookies` entry and no file containing a Spotify host; the same probe **without** incognito left Spotify JS in `Default/Code Cache/js` (21 files) and Spotify cookies. The `EBWebView` folder itself is still created (~7 MB of browser scaffolding) — the uninstall cleanup in TSD §4.11 still applies. Real app window confirmed 2026-10-18: `runout.exe` (dev build, `incognito: true`) driven over WebView2 CDP (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port`); a real album link added by keyboard mounted `open.spotify.com/embed/album/<id>`, which rendered fully (title, track list, "Preview" badge = logged out). After exit `EBWebView/Default/Cache/Cache_Data` held 0 files and none of the 51 files written in the run mentioned a Spotify host; the only Spotify strings on disk are Edge's built-in Subresource Filter rule lists (browser component data, not cache).
- Embeds in InPrivate are logged out: each shows a "Preview" badge (30 s previews; "Save on Spotify" prompts login) and any login inside an embed is dropped when the app closes — **Verified**, 2026-10-18, same probe. Fits BRD FR-016/FR-017 (embed for artwork/preview, "Open Spotify" for the app) — **Interpretation**.
- Docs behind it: Tauri `incognito` ("Enable or disable incognito mode for the WebView", Android unsupported) https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindowBuilder.html#method.incognito ; WebView2 `IsInPrivateModeEnabled` https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controlleroptions ; UDF contents (DiskCache, Cookies, …) https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/user-data-folder — MS docs do not spell out InPrivate's disk behaviour, hence the test.

## Credentials
- Tokens go to Windows Credential Manager via the `keyring` crate, target `Runout/spotify` — **Documented**, TSD §2.
- `--uninstall-cleanup` deletes the generic credential with target name exactly `Runout/spotify` (`CredDeleteW`, `windows` crate, `cleanup.rs`). keyring's Windows backend names the target `{user}.{service}` by default, so M5 must create the entry with `Entry::new_with_target("Runout/spotify", …)` or the uninstaller won't find it — **Interpretation** (keyring docs), 2026-10-18; check in M5.

## SQLite
- `VACUUM INTO` gives a consistent compacted snapshot for backups — **Documented**, https://www.sqlite.org/lang_vacuum.html
- FTS5 ships in `rusqlite` `bundled` builds — **Verified** (tested), 2026-10-18: rusqlite 0.40.2 `features = ["bundled"]`, no extra flag; `db` tests create and query `item_fts`.

## Gotchas found while building
<!-- newest first: date — what bit us — fix / playbook updated -->
- 2026-10-18 — Once Rust code calls `tauri-plugin-dialog`, `cargo test` executables die with `STATUS_ENTRYPOINT_NOT_FOUND` (0xc0000139): rfd needs Common-Controls v6 (`TaskDialogIndirect`) and tauri-build embeds that manifest into bins only. Fix in `src-tauri/build.rs`: `WindowsAttributes::new_without_app_manifest()` + `/MANIFEST:EMBED /MANIFESTINPUT:windows-app-manifest.xml` as a link arg for every target (MSVC only).
- 2026-10-18 — Tauri 2 commands without `async` run on the main thread, so a DB call or a blocking picker there freezes the window. `commands.rs` marks every command `#[tauri::command(async)]` (sync fn on the async runtime) or `async fn`. Arg keys: the macro uses `ident.unraw()` then camelCase, so `r#type` is sent as `type` and `from_id` as `fromId` (proved by the mock-runtime test in `commands.rs`).
- 2026-10-18 — `db::open` (and `Connection::open`) on a missing path silently creates an empty library. Restore recovery must never reopen `library.db` after it was moved aside: move `pre-restore` back, else open `pre-restore` in place and write `library.db.restore-failed`, which stops `cleanup_pre_restore` (it also needs a read-only `integrity_check` pass on library.db). In WAL mode `VACUUM` alone leaves deleted pages in `-wal`; `delete_library` follows with `PRAGMA wal_checkpoint(TRUNCATE)`. Backup names are UTC and parsed strictly (`runout-backup-YYYYMMDD-HHMMSS[-N].zip`) — local-time names reorder across DST.
- 2026-10-18 — Tauri 2.12 `Builder::build(ctx)` creates no windows (config windows are made in `setup` on `RunEvent::Ready`), so `--uninstall-cleanup` builds the app just for `app.path()` and exits: verified exit 0 in ~1.5 s, no window, `EBWebView` removed. Call `generate_context!()` once (`lib.rs` `context()`), since each call embeds the frontend assets again.
- 2026-10-18 — Restore must close the live `rusqlite::Connection` before renaming `library.db` on Windows (open files can't be renamed): swap an in-memory connection into the `Mutex`, `close()` the old one, move any `-wal`/`-shm` along with the file, then reopen. `backup::restore_backup`.
- 2026-10-18 — Capture listener tests can't send a LAN request over loopback: 127/8 isn't one of TSD §5.1's private ranges, so the real-socket smoke test asserts the 403 and the rest is driven through the pure `Guard::screen` / `handle` functions.
- 2026-10-18 — reqwest 0.13 renamed its TLS feature: `rustls-tls` no longer exists, use `default-features = false, features = ["rustls"]`. Builds on MSVC with no extra tools.
- 2026-10-18 — FTS5 + foreign-key cascades: deleting an item cascades `item_link`/`item_tag` deletes, whose triggers re-insert the FTS row from `item_fts_src`; the item row is already gone by then, so nothing stale comes back (`db::tests::deleting_items_leaves_no_fts_rows`).
- 2026-10-18 — PowerShell `FindWindow($null, …)` passes `""`, not NULL, and finds nothing → use `[NullString]::Value`. Capture a probe window with `PrintWindow(h, dc, 2)`, never a full-screen grab (it records the user's other windows).
