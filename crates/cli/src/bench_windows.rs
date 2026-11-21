//! The real [`BenchDriver`]: the pinned PresentMon, launchers, the keyboard and the clock.

use std::io::BufRead;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pco_core::bench::presentmon;
use pco_core::guard;
use pco_core::hw::GameId;

use crate::bench::BenchDriver;

pub struct WindowsBench {
    exe_dir: Option<PathBuf>,
    presentmon: Option<PathBuf>,
}

impl WindowsBench {
    pub fn new() -> Self {
        Self {
            exe_dir: std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(PathBuf::from)),
            presentmon: None,
        }
    }
}

impl Default for WindowsBench {
    fn default() -> Self {
        Self::new()
    }
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
        pco_core::hw::windows::launch_game(game)
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
        let never = std::sync::atomic::AtomicBool::new(false);
        presentmon::capture(&exe, process, seconds, &never)
    }

    fn now_ms(&mut self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }
}
