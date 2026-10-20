//! Runout core: module tree (TSD §9) and the Tauri builder.
pub mod backup;
pub mod capture;
pub mod cleanup;
pub mod commands;
pub mod db;
pub mod error;
pub mod export;
pub mod importer;
pub mod insights;
pub mod library;
pub mod links;
pub mod logger;
pub mod net;
pub mod settings;
pub mod spotify;
pub mod state;
pub mod text;
pub mod usage;

use std::time::Duration;

use tauri::{Emitter, Manager};

/// Re-probe `net::online()` this often so `online-changed` fires when the network comes back.
const ONLINE_POLL: Duration = Duration::from_secs(30);
/// Spotify cache sweeper period (TSD §5.2: startup + daily).
const SWEEP_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// One embedded context (assets) shared by `run` and `uninstall_cleanup`.
fn context() -> tauri::Context {
    tauri::generate_context!()
}

/// `runout.exe --uninstall-cleanup` (NSIS pre-uninstall hook, TSD §4.11). Builds the app only to
/// resolve paths via Tauri's path API; windows are created on run, so none opens. Returns the exit code.
pub fn uninstall_cleanup() -> i32 {
    let dir = tauri::Builder::default()
        .build(context())
        .and_then(|app| app.path().app_local_data_dir());
    match dir.map_err(error::AppError::from) {
        Ok(d) if cleanup::uninstall_cleanup(&d).is_ok() => 0,
        _ => 1,
    }
}

/// Retry the offline queue and emit `capture-received` / `unresolved-resolved` (TSD §4.1).
fn retry_queued(h: &tauri::AppHandle) {
    let st = h.state::<state::AppState>();
    if let Ok((n, new)) = tauri::async_runtime::block_on(links::retry_unresolved(&st.db)) {
        commands::emit_resolved(h, &st, &new);
        if n > 0 {
            let _ = h.emit("unresolved-resolved", serde_json::json!({ "resolved": n }));
        }
    }
}

/// Blocking native error box; works before any window or event loop exists.
#[cfg(windows)]
fn fatal_box(text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    use windows::core::{HSTRING, w};
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            w!("Runout can't start"),
            MB_OK | MB_ICONERROR,
        )
    };
}

/// The box really shows: opened on a thread, found by its title, closed. `#[ignore]`: it flashes a
/// window on the desktop (`cargo test --lib -- --ignored fatal_box`).
#[cfg(all(test, windows))]
#[test]
#[ignore]
fn fatal_box_shows_a_window() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
    use windows::core::w;
    let t = std::thread::spawn(|| fatal_box("test: settings file is damaged"));
    let hwnd = (0..50)
        .find_map(|_| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            unsafe { FindWindowW(None, w!("Runout can't start")) }.ok()
        })
        .expect("error box appeared");
    unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }.unwrap();
    t.join().unwrap(); // returns once closed: no hang, no panic
}

#[cfg(not(windows))]
fn fatal_box(text: &str) {
    eprintln!("{text}");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Rust-only (set_logger); the UI gets no autostart permission (TSD §7).
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(commands::handler())
        .setup(|app| {
            let st = match state::AppState::init(app.handle()) {
                Ok(st) => st,
                Err(e) => {
                    // Release builds have no console: say why, then exit without a panic (TSD §6).
                    let file = app.path().app_config_dir().map(|d| d.join(settings::FILE));
                    let file =
                        file.map_or_else(|_| "settings.json".into(), |f| f.display().to_string());
                    fatal_box(&format!(
                        "{}

Settings file: {file}",
                        e.message
                    ));
                    std::process::exit(1);
                }
            };
            backup::cleanup_pre_restore(std::path::Path::new(
                &st.settings.lock().unwrap().library_path,
            ));
            usage::record(&st.db.lock().unwrap(), usage::Kind::AppOpen, None, None)?;
            let capture_file = st.paths.config_dir.join(capture::FILE);
            let sp = spotify::Spotify::new(&st.paths.local_data_dir);
            let h_sp = app.handle().clone();
            sp.on_status(move |s| {
                let _ = h_sp.emit("spotify-status", s);
            });
            app.manage(sp);
            app.manage(st);
            let (h_save, h_emit) = (app.handle().clone(), app.handle().clone());
            let listener = capture::Listener::new(
                &capture_file,
                capture::PORT,
                capture::Hooks {
                    save: Box::new(move |link, arrived| {
                        let st = h_save.state::<state::AppState>();
                        tauri::async_runtime::block_on(capture::save(&st.db, link, arrived))
                    }),
                    emit: Box::new(move |event, payload| {
                        let _ = h_emit.emit(event, payload);
                    }),
                },
            );
            let _ = listener.resume(); // port taken → stays off; Settings shows it off
            app.manage(listener);
            backup::spawn_scheduler(app.handle().clone());
            app.manage(std::sync::Mutex::new(logger::Logger::default()));
            logger::poll::spawn(app.handle().clone());
            // spotify_cache.db sweeper: at startup, then daily (TSD §5.2)
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    let _ = handle.state::<spotify::Spotify>().sweep();
                    std::thread::sleep(SWEEP_EVERY);
                }
            });
            let handle = app.handle().clone();
            net::on_online_change(move |on| {
                let _ = handle.emit("online-changed", on);
                if on {
                    let h = handle.clone();
                    std::thread::spawn(move || retry_queued(&h));
                }
            });
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    // Online but a spotify.link request was queued/stopped offline: retry now.
                    if tauri::async_runtime::block_on(net::online()) && links::take_retry_pending()
                    {
                        retry_queued(&handle);
                    }
                    std::thread::sleep(ONLINE_POLL);
                }
            });
            Ok(())
        })
        .run(context())
        .expect("error while running tauri application");
}
