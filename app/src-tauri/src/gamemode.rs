//! Game mode in the app (Stage 4, Part A): a background watcher that turns game mode on
//! while a game runs and off afterwards, the tray icon, and "Start with Windows".
//!
//! These are the app's own, temporary changes, made without administrator rights: the active
//! power plan and closing apps the user chose (see `pco_core::gamemode`, which saves the
//! original state before changing anything and restores it after a crash).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pco_core::gamemode::{self, ActiveState, GameModeSettings};
use pco_core::gamemode_win::WindowsGameMode;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// What the interface shows on Home.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    /// The game being played, while game mode is on.
    pub game: Option<String>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Shared {
    pub settings: Mutex<GameModeSettings>,
    pub status: Mutex<Status>,
    active: Mutex<Option<ActiveState>>,
}

fn state_file(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("gamemode-state.json")
}

/// Restores a game mode left on by a crash or power cut. Called once at start.
pub fn recover(data_dir: &std::path::Path) -> Option<String> {
    let mut sys = WindowsGameMode::new(state_file(data_dir));
    let state = sys.pending_state()?;
    gamemode::leave(&state, &mut sys).err()
}

/// Checks every few seconds for a running game; emits `gamemode` to the interface when
/// something changes.
pub fn watch(app: AppHandle, shared: Arc<Shared>, data_dir: PathBuf) {
    std::thread::spawn(move || {
        let mut sys = WindowsGameMode::new(state_file(&data_dir));
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let settings = shared
                .settings
                .lock()
                .map(|s| s.clone())
                .unwrap_or_default();
            let game = if settings.enabled {
                gamemode::processes_for(&sys).and_then(|p| gamemode::running_game(&p, &settings))
            } else {
                None
            };
            let mut active = match shared.active.lock() {
                Ok(a) => a,
                Err(_) => continue,
            };
            let mut error = None;
            match (active.as_ref(), &game) {
                (None, Some(g)) => match gamemode::enter(g, &settings, &mut sys) {
                    Ok(state) => *active = Some(state),
                    Err(e) => {
                        // Partly on: keep the saved state so leaving restores it.
                        *active = sys_state(&sys).or(Some(ActiveState {
                            game: g.clone(),
                            ..ActiveState::default()
                        }));
                        error = Some(e);
                    }
                },
                (Some(state), None) => {
                    if let Err(e) = gamemode::leave(state, &mut sys) {
                        error = Some(e);
                    }
                    *active = None;
                }
                _ => continue,
            }
            let status = Status {
                enabled: settings.enabled,
                game: active.as_ref().map(|s| s.game.clone()),
                error,
            };
            drop(active);
            if let Ok(mut s) = shared.status.lock() {
                *s = status.clone();
            }
            let _ = app.emit("gamemode", status);
        }
    });
}

fn sys_state(sys: &WindowsGameMode) -> Option<ActiveState> {
    sys.pending_state()
}

/// Leaves game mode now (the app is quitting).
pub fn stop(shared: &Shared, data_dir: &std::path::Path) {
    if let Ok(mut active) = shared.active.lock() {
        if let Some(state) = active.take() {
            let mut sys = WindowsGameMode::new(state_file(data_dir));
            let _ = gamemode::leave(&state, &mut sys);
        }
    }
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "PC Optimizer";

/// "Start with Windows": the app's own entry in this user's Run key, started in the tray.
/// The uninstaller removes it.
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    use pco_core::backend::windows::WindowsRegistry;
    use pco_core::backend::{RegData, RegistryBackend};
    use pco_core::recipe::{Hive, RegValue};
    let mut reg = WindowsRegistry::system();
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = format!("\"{}\" --tray", exe.display());
        reg.write(
            Hive::CurrentUser,
            RUN_KEY,
            RUN_VALUE,
            &RegData::from_value(&RegValue::String(command)),
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    } else {
        reg.delete_value(Hive::CurrentUser, RUN_KEY, RUN_VALUE)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
