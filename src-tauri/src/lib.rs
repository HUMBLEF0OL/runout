//! Runout core: module tree (TSD §9) and the Tauri builder.
pub mod backup;
pub mod capture;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(state::AppState::init(app.handle())?);
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
