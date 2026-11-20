//! The real [`BenchDriver`]: the pinned PresentMon, launchers, the keyboard and the clock.

use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pco_core::bench::presentmon;
use pco_core::guard;
use pco_core::hw::games::STEAM_GAMES;
use pco_core::hw::{GameId, Launcher};

use crate::bench::BenchDriver;

pub struct WindowsBench {
    exe_dir: Option<PathBuf>,
    presentmon: Option<PathBuf>,
    captures: u32,
}

impl WindowsBench {
    pub fn new() -> Self {
        Self {
            exe_dir: std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(PathBuf::from)),
            presentmon: None,
            captures: 0,
        }
    }
}

impl Default for WindowsBench {
    fn default() -> Self {
        Self::new()
    }
}

/// Opens a launcher URI (`steam://…`) with its registered handler.
fn open_uri(uri: &str) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(uri)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn riot_client() -> Option<PathBuf> {
    let base = std::env::var_os("ProgramData")
        .map_or_else(|| PathBuf::from(r"C:\ProgramData"), PathBuf::from);
    let text = std::fs::read_to_string(base.join(r"Riot Games\RiotClientInstalls.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let path = value.get("rc_default")?.as_str()?;
    Some(PathBuf::from(path.replace('/', "\\")))
}

impl BenchDriver for WindowsBench {
    fn presentmon(&mut self) -> Result<(), String> {
        let path = presentmon::locate(self.exe_dir.as_deref()).map_err(|e| e.to_string())?;
        self.presentmon = Some(path);
        Ok(())
    }

    fn is_running(&mut self, process: &str) -> bool {
        guard::windows::running_processes()
            .map(|list| list.iter().any(|n| n.eq_ignore_ascii_case(process)))
            .unwrap_or(false)
    }

    fn launch(&mut self, game: GameId) -> Result<(), String> {
        let installed = pco_core::hw::windows::installed_games();
        let Some(found) = installed.iter().find(|g| g.id == game) else {
            return Err("not installed".into());
        };
        match found.launcher {
            Launcher::Steam => {
                let app = STEAM_GAMES
                    .iter()
                    .find(|(_, id)| *id == game)
                    .map(|(app, _)| *app)
                    .ok_or("no Steam app id")?;
                open_uri(&format!("steam://rungameid/{app}"))
            }
            Launcher::Epic => {
                open_uri("com.epicgames.launcher://apps/Fortnite?action=launch&silent=true")
            }
            Launcher::Riot => {
                let client = riot_client().ok_or("Riot Client not found")?;
                let product = match game {
                    GameId::LeagueOfLegends => "league_of_legends",
                    _ => "valorant",
                };
                Command::new(client)
                    .args([
                        format!("--launch-product={product}"),
                        "--launch-patchline=live".into(),
                    ])
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            Launcher::Ea => Err("the EA app cannot be started automatically".into()),
        }
    }

    fn wait_for_enter(&mut self) -> bool {
        let mut line = String::new();
        matches!(std::io::stdin().lock().read_line(&mut line), Ok(n) if n > 0)
    }

    fn sleep(&mut self, seconds: u32) {
        std::thread::sleep(Duration::from_secs(u64::from(seconds)));
    }

    fn capture(&mut self, process: &str, seconds: u32) -> Result<String, String> {
        let exe = self.presentmon.clone().ok_or("PresentMon not checked")?;
        // Checked again right before running it.
        presentmon::verify(&exe).map_err(|e| e.to_string())?;
        self.captures += 1;
        let csv = std::env::temp_dir().join(format!(
            "pco-bench-{}-{}.csv",
            std::process::id(),
            self.captures
        ));
        let _ = std::fs::remove_file(&csv);
        let mut child = Command::new(&exe)
            .args(presentmon::arguments(process, &csv, seconds))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start PresentMon: {e}"))?;
        // PresentMon stops by itself (--timed); give it a margin, then stop it.
        let deadline = Instant::now() + Duration::from_secs(u64::from(seconds) + 30);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(250))
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("PresentMon did not stop in time".into());
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        let text = std::fs::read_to_string(&csv)
            .map_err(|_| "no frames were captured (was the game running and in focus?)".to_owned());
        let _ = std::fs::remove_file(&csv);
        text
    }

    fn now_ms(&mut self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }
}
