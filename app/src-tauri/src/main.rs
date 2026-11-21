//! PC Optimizer, the Windows app. Runs as a normal user: everything here only reads; every
//! change goes through `pco-helper.exe`, started with UAC (Stage 3, 3.2 and 3.4).

// No console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(e) = tauri::Builder::default().run(tauri::generate_context!()) {
        eprintln!("PC Optimizer could not start: {e}");
        std::process::exit(1);
    }
}
