//! Runout core: module tree (TSD §9) and the Tauri builder.
pub mod backup;
pub mod capture;
pub mod cleanup;
pub mod commands;
pub mod db;
pub mod error;
pub mod export;
pub mod library;
pub mod links;
pub mod net;
pub mod settings;
pub mod state;
pub mod text;
pub mod usage;

use std::time::Duration;

use tauri::{Emitter, Manager};

/// Re-probe `net::online()` this often so `online-changed` fires when the network comes back.
const ONLINE_POLL: Duration = Duration::from_secs(30);

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let st = state::AppState::init(app.handle())?;
            backup::cleanup_pre_restore(std::path::Path::new(
                &st.settings.lock().unwrap().library_path,
            ));
            usage::record(&st.db.lock().unwrap(), usage::Kind::AppOpen, None, None)?;
            let capture_file = st.paths.config_dir.join(capture::FILE);
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
            let handle = app.handle().clone();
            net::on_online_change(move |on| {
                let _ = handle.emit("online-changed", on);
                if on {
                    let h = handle.clone();
                    std::thread::spawn(move || {
                        let st = h.state::<state::AppState>();
                        if let Ok(n) =
                            tauri::async_runtime::block_on(links::retry_unresolved(&st.db))
                            && n > 0
                        {
                            let _ =
                                h.emit("unresolved-resolved", serde_json::json!({ "resolved": n }));
                        }
                    });
                }
            });
            std::thread::spawn(|| {
                loop {
                    tauri::async_runtime::block_on(net::online());
                    std::thread::sleep(ONLINE_POLL);
                }
            });
            Ok(())
        })
        .run(context())
        .expect("error while running tauri application");
}
