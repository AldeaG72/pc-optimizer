//! Starting an installed game through its own launcher (for a benchmark). Uses only the
//! launchers' public ways to start a game; never touches the game's files or process.

use std::path::PathBuf;
use std::process::Command;

use crate::hw::games::STEAM_GAMES;
use crate::hw::{GameId, Launcher};

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

/// Starts `game` through the launcher it is installed with. The EA app cannot be started
/// this way: the user starts the game.
pub fn launch_game(game: GameId) -> Result<(), String> {
    let installed = super::installed_games();
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
