//! PC Optimizer, the Windows app. Runs as a normal user: every command here only reads
//! (the registry through a read-only wrapper); every change goes through `pco-helper.exe`,
//! started with UAC (Stage 3, 3.2 and 3.4).

// No console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod home;
mod settings;
mod views;

use std::path::PathBuf;

use pco_core::backend::windows::{WindowsRegistry, WindowsServices};
use pco_core::backend::ReadOnlyRegistry;
use pco_core::bench::Series;
use pco_core::guard::{self, SystemInfo};
use pco_core::journal::Journal;
use pco_core::recipe;
use pco_helper::protocol::Event;
use tauri::ipc::Channel;

use settings::Settings;
use views::HelperRequest;

/// `%LOCALAPPDATA%\PcOptimizer`, or `PCO_DATA_DIR` (tests), like `pco` and the helper.
fn data_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("PCO_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("PcOptimizer"))
        .ok_or_else(|| "LOCALAPPDATA is not set".to_owned())
}

fn journal() -> Result<Journal, String> {
    let dir = data_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Journal::open(&dir.join("journal.db")).map_err(|e| e.to_string())
}

/// `pco-helper.exe` next to the app; in a development build, the one `cargo build` made.
fn helper_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let next_to_app = exe.with_file_name("pco-helper.exe");
    if next_to_app.exists() {
        return Ok(next_to_app);
    }
    if cfg!(debug_assertions) {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("debug")
            .join("pco-helper.exe");
        if dev.exists() {
            return Ok(dev);
        }
    }
    Err("pco-helper.exe is missing next to the app".into())
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
}

/// "ro" or "en": the Windows display language.
#[tauri::command]
fn system_lang() -> &'static str {
    match pco_core::i18n::system_lang() {
        recipe::Lang::Ro => "ro",
        recipe::Lang::En => "en",
    }
}

/// The app's saved choices (defaults if there are none yet).
#[tauri::command]
fn get_settings() -> Result<Settings, String> {
    Ok(Settings::load(&data_dir()?.join("settings.json")))
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<(), String> {
    let dir = data_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    settings.save(&dir.join("settings.json"))
}

/// The tour version the user must accept (bumped when the disclaimer changes).
#[tauri::command]
fn tour_version() -> u32 {
    settings::TOUR_VERSION
}

/// Every built-in recipe with its state on this PC. Reads only.
#[tauri::command]
async fn recipes(lang: String) -> Result<views::Catalog, String> {
    blocking(move || {
        let recipes =
            recipe::load_embedded(pco_core::embedded::RECIPES).map_err(|e| e.to_string())?;
        let host = guard::windows::host().map_err(|e| e.to_string())?;
        let mut journal = journal()?;
        let mut registry = ReadOnlyRegistry(WindowsRegistry::system());
        let mut services = WindowsServices::read_only();
        Ok(views::Catalog {
            recipes: views::recipe_views(
                &recipes,
                views::lang_from(&lang),
                &mut registry,
                &mut services,
                &mut journal,
                SystemInfo::from(&host),
            ),
            laptop: host.form_factor == Some(pco_core::recipe::FormFactor::Laptop),
        })
    })
    .await
}

/// The Home page: hardware and health findings in `lang` (about a second). Reads only.
#[tauri::command]
async fn home(lang: String) -> Result<home::HomeView, String> {
    blocking(move || {
        let texts = pco_core::i18n::Texts::load(views::lang_from(&lang))?;
        let recipes =
            recipe::load_embedded(pco_core::embedded::RECIPES).map_err(|e| e.to_string())?;
        let info = pco_core::hw::windows::detect();
        let today = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_secs() / 86_400).unwrap_or(0))
            .unwrap_or(0);
        let findings = pco_core::health::run(&info, today);
        Ok(home::home_view(&texts, &info, &findings, &recipes))
    })
    .await
}

/// Benchmark history, newest first.
#[tauri::command]
async fn bench_history() -> Result<Vec<Series>, String> {
    blocking(|| journal()?.benchmark_series(None).map_err(|e| e.to_string())).await
}

/// Starts the helper (UAC prompt) for one request and streams its events to the interface.
#[tauri::command]
async fn run_helper(request: HelperRequest, on_event: Channel<Event>) -> Result<(), String> {
    let request = request.to_request()?;
    blocking(move || {
        let helper = helper_path()?;
        pco_helper::pipe::run(&helper, &request, true, &mut |event| {
            let _ = on_event.send(event.clone());
        })
        .map_err(|e| match e {
            pco_helper::pipe::RunError::Cancelled => "cancelled".to_owned(),
            other => other.to_string(),
        })
    })
    .await
}

/// "Restart now" after applying: restarts Explorer, signs out or restarts Windows (5 s).
/// Only on the user's click; needs no administrator rights.
#[tauri::command]
fn restart(kind: String) -> Result<(), String> {
    let system32 = std::env::var_os("SystemRoot")
        .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
        .join("System32");
    let run = |exe: &str, args: &[&str]| {
        std::process::Command::new(system32.join(exe))
            .args(args)
            .status()
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    match kind.as_str() {
        "reboot" => run("shutdown.exe", &["/r", "/t", "5"]),
        "signout" => run("shutdown.exe", &["/l"]),
        "explorer" => {
            run("taskkill.exe", &["/f", "/im", "explorer.exe"])?;
            let windows = system32.parent().map(PathBuf::from).unwrap_or_default();
            std::process::Command::new(windows.join("explorer.exe"))
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        _ => Err(format!("unknown restart kind: {kind}")),
    }
}

fn main() {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            system_lang,
            get_settings,
            save_settings,
            tour_version,
            recipes,
            home,
            bench_history,
            run_helper,
            restart
        ])
        .run(tauri::generate_context!());
    if let Err(e) = app {
        eprintln!("PC Optimizer could not start: {e}");
        std::process::exit(1);
    }
}
