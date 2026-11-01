// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // NSIS pre-uninstall hook (TSD §4.11): clean up and exit without opening a window.
    if std::env::args().any(|a| a == "--uninstall-cleanup") {
        std::process::exit(runout_lib::uninstall_cleanup());
    }
    runout_lib::run()
}
