//! The app's own choices, in `settings.json` next to the journal. Nothing here changes
//! Windows; a broken or missing file just means the defaults.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Bumped when the first-start disclaimer changes, so it is shown and accepted again.
pub const TOUR_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// "ro" or "en"; `None` follows Windows.
    pub lang: Option<String>,
    /// "system", "light" or "dark".
    pub theme: String,
    pub sidebar_collapsed: bool,
    /// The tour version whose disclaimer the user accepted ("Sunt de acord").
    pub tour_accepted: Option<u32>,
    /// Create a Windows restore point before applying (optional, SPEC 3).
    pub restore_point: bool,
    /// Daily copy of the changed settings (optional, SPEC 3).
    pub daily_backup: bool,
    /// Show the Advanced category (asked for confirmation when turned on).
    pub show_advanced: bool,
    /// The monitor the user plays on, when there are several (device name).
    pub game_monitor: Option<String>,
    /// Game mode (Stage 4): what happens while a game runs.
    pub game_mode: pco_core::gamemode::GameModeSettings,
    /// Start hidden in the tray when Windows starts (off by default).
    pub start_with_windows: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang: None,
            theme: "system".into(),
            sidebar_collapsed: false,
            tour_accepted: None,
            restore_point: false,
            daily_backup: false,
            show_advanced: false,
            game_monitor: None,
            game_mode: pco_core::gamemode::GameModeSettings::default(),
            start_with_windows: false,
        }
    }
}

impl Settings {
    /// Unknown values fall back to the defaults instead of breaking the app.
    fn cleaned(mut self) -> Self {
        if !matches!(self.lang.as_deref(), None | Some("ro" | "en")) {
            self.lang = None;
        }
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            self.theme = "system".into();
        }
        if self.game_monitor.as_ref().is_some_and(|m| m.len() > 200) {
            self.game_monitor = None;
        }
        // Executable names only (letters, digits, spaces and ._-()+, ending in .exe).
        let exe = |n: &String| pco_core::bench::target::is_exe_name(n);
        self.game_mode.close_apps.retain(exe);
        self.game_mode.extra_games.retain(exe);
        self.game_mode.close_apps.truncate(50);
        self.game_mode.extra_games.truncate(50);
        self
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .unwrap_or_default()
            .cleaned()
    }

    /// Written to a temporary file first, then renamed, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text =
            serde_json::to_string_pretty(&self.clone().cleaned()).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pco-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn missing_or_broken_file_gives_defaults() {
        let path = temp("broken");
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn round_trip_and_cleaning() {
        let path = temp("trip");
        let s = Settings {
            lang: Some("ro".into()),
            theme: "dark".into(),
            sidebar_collapsed: true,
            tour_accepted: Some(TOUR_VERSION),
            restore_point: true,
            daily_backup: false,
            show_advanced: false,
            game_monitor: Some(r"\\.\DISPLAY1".into()),
            ..Settings::default()
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);

        std::fs::write(
            &path,
            r#"{"lang":"fr","theme":"neon","sidebarCollapsed":true}"#,
        )
        .unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.lang, None);
        assert_eq!(loaded.theme, "system");
        assert!(loaded.sidebar_collapsed);
        assert_eq!(loaded.tour_accepted, None);
    }
}
