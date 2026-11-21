//! What a benchmark measures: one of the supported games (by short name) or any process
//! given by its executable name. Shared by `pco bench`, the app and the elevated helper.

use crate::hw::GameId;

/// The six supported games, in the order the app lists them.
pub const GAMES: [GameId; 6] = [
    GameId::Cs2,
    GameId::Valorant,
    GameId::Fortnite,
    GameId::Apex,
    GameId::LeagueOfLegends,
    GameId::Dota2,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Stored in the history: the game key, or the process name in lowercase.
    pub key: String,
    pub game: Option<GameId>,
    /// Executables that count as the target (e.g. Apex has a DX11 and a DX12 build).
    pub processes: Vec<String>,
}

pub fn game_key(id: GameId) -> &'static str {
    match id {
        GameId::Cs2 => "cs2",
        GameId::Dota2 => "dota2",
        GameId::Apex => "apex",
        GameId::Fortnite => "fortnite",
        GameId::Valorant => "valorant",
        GameId::LeagueOfLegends => "lol",
    }
}

pub fn game_processes(id: GameId) -> &'static [&'static str] {
    match id {
        GameId::Cs2 => &["cs2.exe"],
        GameId::Dota2 => &["dota2.exe"],
        GameId::Apex => &["r5apex.exe", "r5apex_dx12.exe"],
        GameId::Fortnite => &["FortniteClient-Win64-Shipping.exe"],
        GameId::Valorant => &["VALORANT-Win64-Shipping.exe"],
        GameId::LeagueOfLegends => &["League of Legends.exe"],
    }
}

/// Games without a fixed test map (SPEC 4.4): measured during normal play, "less precise".
pub fn precise(id: GameId) -> bool {
    !matches!(id, GameId::Valorant | GameId::LeagueOfLegends)
}

/// A plain executable name: letters, digits, spaces and `._-()+`, ending in `.exe`, no
/// path. Anything else is refused (the name is only ever a PresentMon process filter).
pub fn is_exe_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.len() > 4
        && lower.len() <= 80
        && lower.ends_with(".exe")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " ._-()+".contains(c))
}

/// `cs2`, `valorant`, … or an executable name such as `msedge.exe`.
pub fn resolve(target: &str) -> Option<Target> {
    let trimmed = target.trim();
    let lower = trimmed.to_lowercase();
    let game = match lower.as_str() {
        "cs2" | "counter-strike" => Some(GameId::Cs2),
        "dota2" | "dota" => Some(GameId::Dota2),
        "apex" => Some(GameId::Apex),
        "fortnite" => Some(GameId::Fortnite),
        "valorant" => Some(GameId::Valorant),
        "lol" | "league" => Some(GameId::LeagueOfLegends),
        _ => None,
    };
    if let Some(id) = game {
        return Some(Target {
            key: game_key(id).to_owned(),
            game: Some(id),
            processes: game_processes(id).iter().map(|p| (*p).to_owned()).collect(),
        });
    }
    is_exe_name(trimmed).then(|| Target {
        key: lower.clone(),
        game: None,
        processes: vec![trimmed.to_owned()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn games_and_executables() {
        let cs2 = resolve("CS2").unwrap_or_else(|| unreachable!());
        assert_eq!(cs2.key, "cs2");
        assert_eq!(cs2.processes, ["cs2.exe"]);
        assert_eq!(resolve("apex").map(|t| t.processes.len()), Some(2));
        let edge = resolve(" msedge.exe ").unwrap_or_else(|| unreachable!());
        assert_eq!((edge.key.as_str(), edge.game), ("msedge.exe", None));
        assert_eq!(
            resolve("League of Legends.exe").map(|t| t.key),
            Some("league of legends.exe".to_owned())
        );
        for bad in [
            "",
            "notepad",
            ".exe",
            r"C:\x\evil.exe",
            "a/b.exe",
            "x\".exe",
            "a;b.exe",
        ] {
            assert_eq!(resolve(bad), None, "{bad}");
        }
        assert!(!precise(GameId::Valorant) && precise(GameId::Cs2));
    }
}
