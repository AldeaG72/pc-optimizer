//! PC Optimizer, the Windows app. Runs as a normal user: every command here only reads
//! (the registry through a read-only wrapper); every change goes through `pco-helper.exe`,
//! started with UAC (Stage 3, 3.2 and 3.4). The one exception is the cleanup of the user's
//! own files (temporary files, launcher caches, Recycle Bin): it runs here, without
//! administrator rights, so nothing elevated ever works in a folder the user controls.

// No console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod benchview;
mod cleanview;
mod gamemode;
mod home;
mod settings;
mod startupview;
mod views;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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

/// What the interface shows when the helper cannot run. "antivirus": the file is missing
/// (quarantined), Windows refused to start it (access denied, "virus detected", blocked by
/// policy), or it was stopped before it could connect.
fn helper_error(e: pco_helper::pipe::RunError) -> String {
    use pco_helper::pipe::RunError;
    const BLOCKED: [&str; 5] = [
        "(os error 2)",
        "(os error 5)",
        "(os error 225)",
        "(os error 226)",
        "(os error 1260)",
    ];
    match e {
        RunError::Cancelled => "cancelled".to_owned(),
        RunError::NoConnection => "antivirus".to_owned(),
        RunError::Launch(msg) if BLOCKED.iter().any(|c| msg.contains(c)) => "antivirus".to_owned(),
        other => other.to_string(),
    }
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
fn save_settings(
    settings: Settings,
    shared: tauri::State<'_, Arc<gamemode::Shared>>,
) -> Result<(), String> {
    let dir = data_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("settings.json");
    let before = Settings::load(&path);
    if before.start_with_windows != settings.start_with_windows {
        gamemode::set_autostart(settings.start_with_windows)?;
    }
    if let Ok(mut gm) = shared.settings.lock() {
        *gm = settings.game_mode.clone();
    }
    settings.save(&path)
}

/// Game mode right now (Home card).
#[tauri::command]
fn gamemode_status(shared: tauri::State<'_, Arc<gamemode::Shared>>) -> gamemode::Status {
    let mut status = shared.status.lock().map(|s| s.clone()).unwrap_or_default();
    status.enabled = shared.settings.lock().map(|s| s.enabled).unwrap_or(false);
    status
}

/// Apps suggested in the "close while playing" list.
#[tauri::command]
fn gamemode_suggested() -> Vec<&'static str> {
    pco_core::gamemode::SUGGESTED.to_vec()
}

/// Today, in days since 1970-01-01.
fn today_days() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    i64::try_from(secs / 86_400).unwrap_or(0)
}

/// The optional daily backup (Settings): today's `.reg` copy of the values the app changed,
/// once a day, last 7 kept. Reads the registry only.
#[tauri::command]
async fn daily_backup() -> Result<Option<String>, String> {
    blocking(|| {
        let dir = data_dir()?.join("backups");
        let journal = journal()?;
        let reg = ReadOnlyRegistry(WindowsRegistry::system());
        let today = pco_core::backup::date_from_days(today_days());
        pco_core::backup::run_daily(&dir, &today, &reg, &journal)
            .map(|p| p.map(|p| p.display().to_string()))
            .map_err(|e| e.to_string())
    })
    .await
}

/// The anonymized support log (3.13).
fn build_report(lang: &str) -> Result<String, String> {
    let recipes = recipe::load_embedded(pco_core::embedded::RECIPES).map_err(|e| e.to_string())?;
    let host = guard::windows::host().ok();
    let hw = pco_core::hw::windows::detect();
    let mut journal = journal()?;
    let mut registry = ReadOnlyRegistry(WindowsRegistry::system());
    let mut services = WindowsServices::read_only();
    let mut power = pco_core::backend::WindowsPower::read_only();
    let mut apps = pco_core::backend::WindowsAppx::read_only();
    let system = host.as_ref().map_or(
        SystemInfo {
            build: 0,
            edition: None,
            form_factor: None,
        },
        SystemInfo::from,
    );
    let statuses = views::recipe_views(
        &recipes,
        views::lang_from(lang),
        &mut registry,
        &mut services,
        &mut power,
        &mut apps,
        &mut journal,
        system,
    )
    .into_iter()
    .map(|v| {
        let status = serde_json::to_value(&v.status)
            .ok()
            .map_or_else(String::new, |s| s.to_string());
        (v.id, status)
    })
    .collect();
    let series = journal.benchmark_series(None).unwrap_or_default();
    let secrets = ["USERNAME", "COMPUTERNAME", "USERPROFILE", "USERDOMAIN"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .collect();
    let input = pco_core::report::ReportInput {
        app_version: env!("CARGO_PKG_VERSION"),
        host: host.as_ref(),
        hw: Some(&hw),
        statuses,
        series,
        secrets,
    };
    pco_core::report::build(&input, &journal).map_err(|e| e.to_string())
}

/// Settings › Export the log: builds the anonymized report, then asks where to save it.
/// `None` if the user closed the dialog.
#[tauri::command]
async fn export_log(app: tauri::AppHandle, lang: String) -> Result<Option<String>, String> {
    blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let text = build_report(&lang)?;
        let Some(path) = app
            .dialog()
            .file()
            .set_file_name("pc-optimizer-log.txt")
            .add_filter("Text", &["txt"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    })
    .await
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
        let mut power = pco_core::backend::WindowsPower::read_only();
        let mut apps = pco_core::backend::WindowsAppx::read_only();
        Ok(views::Catalog {
            recipes: views::recipe_views(
                &recipes,
                views::lang_from(&lang),
                &mut registry,
                &mut services,
                &mut power,
                &mut apps,
                &mut journal,
                SystemInfo::from(&host),
            ),
            laptop: host.form_factor == Some(pco_core::recipe::FormFactor::Laptop),
        })
    })
    .await
}

/// How much each cleanup would free (system folders: unknown without admin). Reads only.
#[tauri::command]
async fn cleanup_sizes() -> Result<Vec<cleanview::CleanSize>, String> {
    blocking(move || {
        let recipes =
            recipe::load_embedded(pco_core::embedded::RECIPES).map_err(|e| e.to_string())?;
        Ok(cleanview::sizes(
            &recipes,
            &pco_core::cleanup::windows::WindowsCleanup::user(),
        ))
    })
    .await
}

/// Cleans the user's own places of these cleanup recipes (after the interface asked).
#[tauri::command]
async fn clean_user(ids: Vec<String>) -> Result<Vec<cleanview::CleanResult>, String> {
    blocking(move || {
        let recipes =
            recipe::load_embedded(pco_core::embedded::RECIPES).map_err(|e| e.to_string())?;
        let mut user = pco_core::cleanup::windows::WindowsCleanup::user();
        cleanview::clean_user(&recipes, &ids, &mut user)
    })
    .await
}

/// Programs that start with Windows, with their switches. Reads only.
#[tauri::command]
async fn startup_list() -> Result<Vec<startupview::StartupView>, String> {
    blocking(move || {
        let reg = ReadOnlyRegistry(WindowsRegistry::system());
        startupview::views(&reg, &pco_core::startup::windows::folders()).map_err(|e| e.to_string())
    })
    .await
}

/// Troubleshooting: size of the shader caches. Reads only.
#[tauri::command]
async fn shader_cache_size() -> Result<Option<u64>, String> {
    use pco_core::cleanup::CleanupBackend;
    blocking(|| {
        pco_core::cleanup::windows::WindowsCleanup::user()
            .measure(pco_core::recipe::CleanupTarget::ShaderCache)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Troubleshooting: empties the shader caches (the user's own folders, no admin), after the
/// interface asked. Returns the bytes freed.
#[tauri::command]
async fn clean_shader_cache() -> Result<u64, String> {
    use pco_core::cleanup::CleanupBackend;
    blocking(|| {
        pco_core::cleanup::windows::WindowsCleanup::user()
            .clean(pco_core::recipe::CleanupTarget::ShaderCache)
            .map(|r| r.freed_bytes)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Troubleshooting: opens a Windows control panel (fixed list). Changes nothing itself.
#[tauri::command]
fn open_panel(kind: String) -> Result<(), String> {
    let applet = match kind.as_str() {
        "sound" => "mmsys.cpl",
        "power" => "powercfg.cpl",
        "network" => "ncpa.cpl",
        _ => return Err(format!("unknown panel: {kind}")),
    };
    let windows =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    std::process::Command::new(windows.join(r"System32\control.exe"))
        .arg(applet)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
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

/// The running helper's stop switch (a benchmark can be stopped from the interface).
#[derive(Default)]
struct HelperState {
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

/// Starts the helper (UAC prompt) for one request and streams its events to the interface.
#[tauri::command]
async fn run_helper(
    request: HelperRequest,
    on_event: Channel<Event>,
    state: tauri::State<'_, HelperState>,
) -> Result<(), String> {
    let request = request.to_request()?;
    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut slot) = state.cancel.lock() {
        *slot = Some(Arc::clone(&cancel));
    }
    blocking(move || {
        // Missing next to the app: most likely quarantined by an antivirus.
        let helper = helper_path().map_err(|_| "antivirus".to_owned())?;
        pco_helper::pipe::run(&helper, &request, true, &cancel, &mut |event| {
            let _ = on_event.send(event.clone());
        })
        .map_err(helper_error)
    })
    .await
}

/// Settings › Benchmark history › Delete: the app's own results, nothing in Windows.
#[tauri::command]
async fn bench_forget() -> Result<usize, String> {
    blocking(|| {
        journal()?
            .forget_benchmarks(None)
            .map_err(|e| e.to_string())
    })
    .await
}

/// The app's version, for Settings › About.
#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// "Stop" on a running benchmark: the runs measured so far are kept.
#[tauri::command]
fn cancel_helper(state: tauri::State<'_, HelperState>) {
    if let Ok(slot) = state.cancel.lock() {
        if let Some(flag) = slot.as_ref() {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

/// The six games for the benchmark card, with whether each is installed.
#[tauri::command]
async fn bench_games() -> Result<Vec<benchview::BenchGame>, String> {
    blocking(|| Ok(benchview::games(&pco_core::hw::windows::installed_games()))).await
}

/// Starts a game through its launcher (no administrator rights; nothing in the game).
#[tauri::command]
async fn launch_game(key: String) -> Result<(), String> {
    blocking(move || {
        let game = pco_core::bench::target::resolve(&key)
            .and_then(|t| t.game)
            .ok_or("not a supported game")?;
        pco_core::hw::windows::launch_game(game)
    })
    .await
}

/// The latest before → after verdict for `target`, and the newest "before" still waiting
/// for its "after".
#[tauri::command]
async fn bench_status(target: Option<String>) -> Result<serde_json::Value, String> {
    blocking(move || {
        let journal = journal()?;
        let verdict = match target {
            Some(t) => benchview::verdict(&journal, &t)?,
            None => None,
        };
        let open = benchview::open_before(&journal)?;
        Ok(serde_json::json!({ "verdict": verdict, "openBefore": open }))
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

/// Tray icon: open the window, or quit (game mode is turned off first).
fn tray(app: &tauri::App, shared: Arc<gamemode::Shared>) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;
    let open = MenuItem::with_id(app, "open", "PC Optimizer", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit / Ieșire", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut builder = TrayIconBuilder::new()
        .tooltip("PC Optimizer")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => {
                if let Ok(dir) = data_dir() {
                    gamemode::stop(&shared, &dir);
                }
                app.exit(0);
            }
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn show_main(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn main() {
    let shared = Arc::new(gamemode::Shared::default());
    let watch_shared = Arc::clone(&shared);
    let tray_shared = Arc::clone(&shared);
    let close_shared = Arc::clone(&shared);
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(HelperState::default())
        .manage(shared)
        .setup(move |app| {
            use tauri::Manager;
            if let Ok(dir) = data_dir() {
                // A game mode left on by a crash or power cut is undone first.
                let _ = gamemode::recover(&dir);
                let settings = Settings::load(&dir.join("settings.json"));
                if let Ok(mut gm) = watch_shared.settings.lock() {
                    *gm = settings.game_mode;
                }
                gamemode::watch(app.handle().clone(), Arc::clone(&watch_shared), dir);
            }
            tray(app, tray_shared)?;
            // Started with Windows: stay in the tray.
            if std::env::args().any(|a| a == "--tray") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            Ok(())
        })
        .on_window_event(move |window, event| {
            // With game mode on, closing the window keeps the app in the tray.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let enabled = close_shared
                    .settings
                    .lock()
                    .map(|s| s.enabled)
                    .unwrap_or(false);
                if enabled {
                    api.prevent_close();
                    let _ = window.hide();
                } else if let Ok(dir) = data_dir() {
                    gamemode::stop(&close_shared, &dir);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            system_lang,
            get_settings,
            save_settings,
            gamemode_status,
            gamemode_suggested,
            tour_version,
            recipes,
            home,
            cleanup_sizes,
            clean_user,
            daily_backup,
            export_log,
            shader_cache_size,
            clean_shader_cache,
            open_panel,
            startup_list,
            bench_history,
            run_helper,
            cancel_helper,
            bench_forget,
            app_version,
            bench_games,
            launch_game,
            bench_status,
            restart
        ])
        .run(tauri::generate_context!());
    if let Err(e) = app {
        eprintln!("PC Optimizer could not start: {e}");
        std::process::exit(1);
    }
}
