//! Reads the launchers' files and registry to find the launch games, then the per-game GPU
//! preference from Settings › Display › Graphics.

use std::path::{Path, PathBuf};

use super::reg_string;
use crate::backend::windows::WindowsRegistry;
use crate::hw::games::{
    epic_game, exe_relative, riot_install_path, steam_games, steam_libraries, Found, RIOT_GAMES,
};
use crate::hw::{Game, GameId, GpuPreference, HwInfo, Launcher};
use crate::recipe::Hive;

const GPU_PREFS_KEY: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn program_data() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}

pub(super) fn games(reg: &WindowsRegistry, info: &mut HwInfo, _errors: &mut Vec<String>) {
    let mut found: Vec<Found> = Vec::new();

    // Steam.
    if let Some(steam) = reg_string(reg, Hive::CurrentUser, r"Software\Valve\Steam", "SteamPath") {
        let steam = PathBuf::from(steam.replace('/', "\\"));
        if let Some(text) = read(&steam.join("steamapps").join("libraryfolders.vdf")) {
            found.extend(steam_games(&steam_libraries(&text), &read));
        }
    }

    // Epic Games Launcher.
    let manifests = program_data().join(r"Epic\EpicGamesLauncher\Data\Manifests");
    if let Ok(entries) = std::fs::read_dir(&manifests) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("item"))
            {
                found.extend(read(&path).as_deref().and_then(epic_game));
            }
        }
    }

    // Riot Client.
    for (product, id) in RIOT_GAMES {
        let file = program_data()
            .join(r"Riot Games\Metadata")
            .join(product)
            .join(format!("{product}.product_settings.yaml"));
        if let Some(dir) = read(&file).as_deref().and_then(riot_install_path) {
            found.push(Found {
                id: *id,
                launcher: Launcher::Riot,
                install_dir: dir,
            });
        }
    }

    // EA app (Apex Legends outside Steam).
    for key in [
        r"SOFTWARE\Respawn\Apex",
        r"SOFTWARE\WOW6432Node\Respawn\Apex",
    ] {
        if let Some(dir) = reg_string(reg, Hive::LocalMachine, key, "Install Dir") {
            found.push(Found {
                id: GameId::Apex,
                launcher: Launcher::Ea,
                install_dir: PathBuf::from(dir.trim_end_matches('\\')),
            });
            break;
        }
    }

    for f in found {
        let candidates: Vec<PathBuf> = exe_relative(f.id)
            .iter()
            .map(|r| f.install_dir.join(r))
            .collect();
        let existing = candidates.iter().find(|p| p.exists());
        let exe = existing.or(candidates.first()).cloned().unwrap_or_default();
        let exe_text = exe.to_string_lossy().into_owned();
        if info
            .games
            .iter()
            .any(|g| g.id == f.id && g.exe.eq_ignore_ascii_case(&exe_text))
        {
            continue;
        }
        let gpu_preference = reg_string(reg, Hive::CurrentUser, GPU_PREFS_KEY, &exe_text)
            .map(|d| GpuPreference::parse(&d))
            .unwrap_or(GpuPreference::Default);
        let volume = exe_text
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic() && exe_text.get(1..2) == Some(":"))
            .map(|c| c.to_ascii_uppercase());
        info.games.push(Game {
            id: f.id,
            launcher: f.launcher,
            install_dir: f.install_dir.to_string_lossy().into_owned(),
            exe: exe_text,
            exe_exists: existing.is_some(),
            gpu_preference,
            volume,
        });
    }
}
