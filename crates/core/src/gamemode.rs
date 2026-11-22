//! Game mode (SPEC 4.5, Stage 4: Part A): while a game runs, the actions the user ticked;
//! when it ends, everything goes back.
//!
//! These are temporary changes made by the app itself (no administrator rights): the
//! active power plan, and closing apps the user chose (reopened afterwards). The same rule as
//! the journal applies (CLAUDE.md rules 2 and 3): the original state is saved to a state file
//! **before** anything changes, and if the app stops while a game runs (crash, power cut),
//! the next start restores it from that file. The game's own process is never touched.

use serde::{Deserialize, Serialize};

/// What the user chose (Settings › Game mode).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameModeSettings {
    pub enabled: bool,
    /// Executable names to close while playing, e.g. `onedrive.exe`; reopened after.
    pub close_apps: Vec<String>,
    /// Switch to the "High performance" plan while playing.
    pub high_performance: bool,
    /// Extra games, as executable names (the supported games are always detected).
    pub extra_games: Vec<String>,
}

/// High performance plan.
pub const HIGH_PERFORMANCE: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";

/// Apps suggested in the list to close (the user ticks them; none by default).
pub const SUGGESTED: &[&str] = &[
    "onedrive.exe",
    "ms-teams.exe",
    "spotify.exe",
    "msedge.exe",
    "epicgameslauncher.exe",
    "eadesktop.exe",
];

/// Never closed, whatever the settings say: Windows itself, security, the app.
const NEVER_CLOSE: &[&str] = &[
    "explorer.exe",
    "dwm.exe",
    "csrss.exe",
    "winlogon.exe",
    "svchost.exe",
    "lsass.exe",
    "services.exe",
    "msmpeng.exe",
    "securityhealthsystray.exe",
    "pc-optimizer.exe",
    "pco-helper.exe",
];

/// What game mode changed, saved before changing it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveState {
    pub game: String,
    /// The plan that was active before, if game mode switched it.
    pub plan_before: Option<String>,
    /// Full paths of the apps closed, to start them again.
    pub closed: Vec<String>,
}

/// A running process: its executable name and full path (when Windows tells it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub name: String,
    pub path: Option<String>,
}

/// What game mode needs from Windows. The real one is in the app; tests use a fake.
pub trait GameModeSystem {
    fn processes(&self) -> Result<Vec<Process>, String>;
    fn active_plan(&self) -> Result<String, String>;
    fn plan_exists(&self, plan: &str) -> Result<bool, String>;
    fn set_plan(&mut self, plan: &str) -> Result<(), String>;
    /// Asks the app to close (like closing its window), never forced.
    fn close(&mut self, name: &str) -> Result<(), String>;
    fn start(&mut self, path: &str) -> Result<(), String>;
    /// Writes the state file (before any change) or removes it (`None`, after restoring).
    fn save_state(&mut self, state: Option<&ActiveState>) -> Result<(), String>;
}

/// The process list, or `None` if Windows would not give it (try again next time).
pub fn processes_for(sys: &dyn GameModeSystem) -> Option<Vec<Process>> {
    sys.processes().ok()
}

/// The running game, if any: a supported game's process or one the user added.
pub fn running_game(processes: &[Process], settings: &GameModeSettings) -> Option<String> {
    let known: Vec<String> = crate::bench::target::GAMES
        .iter()
        .flat_map(|g| crate::bench::target::game_processes(*g).iter())
        .map(|p| p.to_lowercase())
        .chain(settings.extra_games.iter().map(|g| g.to_lowercase()))
        .collect();
    processes
        .iter()
        .find(|p| known.contains(&p.name.to_lowercase()))
        .map(|p| p.name.clone())
}

fn closable(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".exe")
        && !NEVER_CLOSE.contains(&lower.as_str())
        && crate::bench::target::GAMES
            .iter()
            .flat_map(|g| crate::bench::target::game_processes(*g).iter())
            .all(|g| !g.eq_ignore_ascii_case(&lower))
}

/// Turns game mode on for `game`. The state file is written before each change, so a
/// crash at any point can be restored by [`leave`].
pub fn enter(
    game: &str,
    settings: &GameModeSettings,
    sys: &mut dyn GameModeSystem,
) -> Result<ActiveState, String> {
    let mut state = ActiveState {
        game: game.to_owned(),
        ..ActiveState::default()
    };
    let mut errors = Vec::new();

    if settings.high_performance && sys.plan_exists(HIGH_PERFORMANCE)? {
        let before = sys.active_plan()?;
        if !before.eq_ignore_ascii_case(HIGH_PERFORMANCE) {
            state.plan_before = Some(before);
            sys.save_state(Some(&state))?;
            if let Err(e) = sys.set_plan(HIGH_PERFORMANCE) {
                errors.push(e);
            }
        }
    }

    let processes = sys.processes()?;
    for name in settings.close_apps.iter().filter(|n| closable(n)) {
        let running: Vec<&Process> = processes
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case(name))
            .collect();
        let Some(path) = running.iter().find_map(|p| p.path.clone()) else {
            continue;
        };
        state.closed.push(path);
        sys.save_state(Some(&state))?;
        if let Err(e) = sys.close(name) {
            errors.push(e);
        }
    }
    sys.save_state(Some(&state))?;
    if errors.is_empty() {
        Ok(state)
    } else {
        // Whatever was changed is in the state file and is restored on leaving.
        Err(errors.join("; "))
    }
}

/// Puts everything back (also after a crash, from the saved state), then removes the state
/// file. Restoring something that never changed is harmless.
pub fn leave(state: &ActiveState, sys: &mut dyn GameModeSystem) -> Result<(), String> {
    let mut errors = Vec::new();
    if let Some(plan) = &state.plan_before {
        if let Err(e) = sys.set_plan(plan) {
            errors.push(e);
        }
    }
    let running = sys.processes().unwrap_or_default();
    for path in &state.closed {
        let already = running.iter().any(|p| {
            p.path
                .as_deref()
                .is_some_and(|q| q.eq_ignore_ascii_case(path))
        });
        if !already {
            if let Err(e) = sys.start(path) {
                errors.push(e);
            }
        }
    }
    if errors.is_empty() {
        sys.save_state(None)
    } else {
        Err(errors.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";

    #[derive(Default)]
    struct Fake {
        plan: String,
        plans: Vec<String>,
        running: Vec<Process>,
        saved: Vec<Option<ActiveState>>,
        log: Vec<String>,
    }

    impl GameModeSystem for Fake {
        fn processes(&self) -> Result<Vec<Process>, String> {
            Ok(self.running.clone())
        }
        fn active_plan(&self) -> Result<String, String> {
            Ok(self.plan.clone())
        }
        fn plan_exists(&self, plan: &str) -> Result<bool, String> {
            Ok(self.plans.iter().any(|p| p == plan))
        }
        fn set_plan(&mut self, plan: &str) -> Result<(), String> {
            self.plan = plan.into();
            Ok(())
        }
        fn close(&mut self, name: &str) -> Result<(), String> {
            self.log.push(format!("close {name}"));
            self.running.retain(|p| !p.name.eq_ignore_ascii_case(name));
            Ok(())
        }
        fn start(&mut self, path: &str) -> Result<(), String> {
            self.log.push(format!("start {path}"));
            Ok(())
        }
        fn save_state(&mut self, state: Option<&ActiveState>) -> Result<(), String> {
            self.saved.push(state.cloned());
            Ok(())
        }
    }

    fn proc(name: &str, path: &str) -> Process {
        Process {
            name: name.into(),
            path: Some(path.into()),
        }
    }

    fn world() -> Fake {
        Fake {
            plan: BALANCED.into(),
            plans: vec![BALANCED.into(), HIGH_PERFORMANCE.into()],
            running: vec![
                proc("cs2.exe", r"C:\Games\cs2.exe"),
                proc("OneDrive.exe", r"C:\OD\OneDrive.exe"),
                proc("explorer.exe", r"C:\Windows\explorer.exe"),
            ],
            ..Fake::default()
        }
    }

    fn settings() -> GameModeSettings {
        GameModeSettings {
            enabled: true,
            close_apps: vec![
                "onedrive.exe".into(),
                "explorer.exe".into(),
                "cs2.exe".into(),
            ],
            high_performance: true,
            extra_games: vec!["mygame.exe".into()],
        }
    }

    #[test]
    fn detects_supported_and_added_games() {
        let w = world();
        assert_eq!(
            running_game(&w.running, &settings()).as_deref(),
            Some("cs2.exe")
        );
        let other = vec![proc("MyGame.exe", r"C:\x\MyGame.exe")];
        assert!(running_game(&other, &settings()).is_some());
        assert!(running_game(&other, &GameModeSettings::default()).is_none());
    }

    #[test]
    fn enter_then_leave_puts_everything_back() {
        let mut w = world();
        let state = enter("cs2.exe", &settings(), &mut w).unwrap();
        assert_eq!(w.plan, HIGH_PERFORMANCE);
        // Never Windows itself or the game, even if listed.
        assert_eq!(w.log, ["close onedrive.exe"]);
        assert_eq!(state.plan_before.as_deref(), Some(BALANCED));
        // Saved before the first change.
        assert_eq!(
            w.saved[0].as_ref().unwrap().plan_before.as_deref(),
            Some(BALANCED)
        );

        leave(&state, &mut w).unwrap();
        assert_eq!(w.plan, BALANCED);
        assert_eq!(w.log.last().unwrap(), r"start C:\OD\OneDrive.exe");
        assert_eq!(w.saved.last().unwrap(), &None);
    }

    #[test]
    fn a_saved_state_after_a_crash_is_enough_to_restore() {
        let mut w = world();
        let state = enter("cs2.exe", &settings(), &mut w).unwrap();
        // The app crashes: only the state file survives.
        let saved = w.saved.last().unwrap().clone().unwrap();
        assert_eq!(saved, state);
        leave(&saved, &mut w).unwrap();
        assert_eq!(w.plan, BALANCED);
    }

    #[test]
    fn nothing_to_do_when_already_high_performance_or_app_not_running() {
        let mut w = world();
        w.plan = HIGH_PERFORMANCE.into();
        w.running.retain(|p| p.name != "OneDrive.exe");
        let state = enter("cs2.exe", &settings(), &mut w).unwrap();
        assert_eq!(state.plan_before, None);
        assert!(state.closed.is_empty());
        assert!(w.log.is_empty());
        leave(&state, &mut w).unwrap();
        assert_eq!(w.plan, HIGH_PERFORMANCE);
    }
}
