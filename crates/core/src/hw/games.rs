//! Finding the launch games (SPEC 1) in the launchers' own files. Pure parsing here, so it
//! is tested on sample files on any OS; `hw::windows` reads the real files.

use std::path::{Path, PathBuf};

use super::{GameId, Launcher};

/// Where a game's main executable is, relative to its install folder.
pub fn exe_relative(id: GameId) -> &'static [&'static str] {
    match id {
        GameId::Cs2 => &[r"game\bin\win64\cs2.exe"],
        GameId::Dota2 => &[r"game\bin\win64\dota2.exe"],
        // DirectX 11 and DirectX 12 builds; the first that exists is used.
        GameId::Apex => &["r5apex.exe", "r5apex_dx12.exe"],
        GameId::Fortnite => &[r"FortniteGame\Binaries\Win64\FortniteClient-Win64-Shipping.exe"],
        GameId::Valorant => &[r"ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe"],
        GameId::LeagueOfLegends => &[r"Game\League of Legends.exe"],
    }
}

/// Steam app ids of the launch games.
pub const STEAM_GAMES: &[(u32, GameId)] = &[
    (730, GameId::Cs2),
    (570, GameId::Dota2),
    (1_172_470, GameId::Apex),
];

/// Riot product folders under `%ProgramData%\Riot Games\Metadata`.
pub const RIOT_GAMES: &[(&str, GameId)] = &[
    ("valorant.live", GameId::Valorant),
    ("league_of_legends.live", GameId::LeagueOfLegends),
];

/// A game found in a launcher's files (before checking the disk).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub id: GameId,
    pub launcher: Launcher,
    pub install_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// Steam: KeyValues ("VDF") text files.
// ---------------------------------------------------------------------------

/// A parsed VDF node: either a string or a list of children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Vdf {
    Str(String),
    Map(Vec<(String, Vdf)>),
}

impl Vdf {
    /// Child by key, ignoring case (Steam is inconsistent).
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Map(items) => items
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Vdf::Str(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Vdf::Str(s) => Some(s),
            Vdf::Map(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Map(items) => items,
            Vdf::Str(_) => &[],
        }
    }
}

/// Parses VDF text into a root map. Lenient: stops at the first malformed token.
pub fn parse_vdf(text: &str) -> Vdf {
    let tokens = vdf_tokens(text);
    let mut pos = 0;
    Vdf::Map(vdf_map(&tokens, &mut pos))
}

#[derive(Debug, PartialEq)]
enum Token {
    Str(String),
    Open,
    Close,
}

fn vdf_tokens(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(other) => s.push(other),
                            None => break,
                        },
                        other => s.push(other),
                    }
                }
                tokens.push(Token::Str(s));
            }
            '{' => tokens.push(Token::Open),
            '}' => tokens.push(Token::Close),
            '/' if chars.peek() == Some(&'/') => {
                // Comment to end of line.
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            c => {
                // Unquoted token.
                let mut s = String::from(c);
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() || n == '{' || n == '}' || n == '"' {
                        break;
                    }
                    s.push(n);
                    chars.next();
                }
                tokens.push(Token::Str(s));
            }
        }
    }
    tokens
}

fn vdf_map(tokens: &[Token], pos: &mut usize) -> Vec<(String, Vdf)> {
    let mut items = Vec::new();
    while let Some(Token::Str(key)) = tokens.get(*pos) {
        *pos += 1;
        match tokens.get(*pos) {
            Some(Token::Str(value)) => {
                *pos += 1;
                items.push((key.clone(), Vdf::Str(value.clone())));
            }
            Some(Token::Open) => {
                *pos += 1;
                let child = vdf_map(tokens, pos);
                if tokens.get(*pos) == Some(&Token::Close) {
                    *pos += 1;
                }
                items.push((key.clone(), Vdf::Map(child)));
            }
            _ => break,
        }
    }
    items
}

/// Steam library folders and the app ids installed in each, from
/// `steamapps\libraryfolders.vdf`.
pub fn steam_libraries(libraryfolders_vdf: &str) -> Vec<(PathBuf, Vec<u32>)> {
    let root = parse_vdf(libraryfolders_vdf);
    let Some(folders) = root.get("libraryfolders") else {
        return Vec::new();
    };
    folders
        .entries()
        .iter()
        .filter_map(|(_, folder)| {
            let path = folder.get("path")?.as_str()?;
            let apps = folder
                .get("apps")
                .map(|apps| {
                    apps.entries()
                        .iter()
                        .filter_map(|(id, _)| id.parse().ok())
                        .collect()
                })
                .unwrap_or_default();
            Some((PathBuf::from(path), apps))
        })
        .collect()
}

/// The `installdir` from an `appmanifest_<id>.acf`.
pub fn steam_installdir(appmanifest_acf: &str) -> Option<String> {
    parse_vdf(appmanifest_acf)
        .get("AppState")?
        .get("installdir")?
        .as_str()
        .map(str::to_owned)
}

/// Launch games in a Steam library: `(library path, app ids)` → found games, given a way to
/// read `appmanifest_<id>.acf` files.
pub fn steam_games(
    libraries: &[(PathBuf, Vec<u32>)],
    read_manifest: &dyn Fn(&Path) -> Option<String>,
) -> Vec<Found> {
    let mut found = Vec::new();
    for (library, apps) in libraries {
        for (app_id, game) in STEAM_GAMES {
            if !apps.contains(app_id) {
                continue;
            }
            let steamapps = library.join("steamapps");
            let manifest = steamapps.join(format!("appmanifest_{app_id}.acf"));
            if let Some(dir) = read_manifest(&manifest).and_then(|m| steam_installdir(&m)) {
                found.push(Found {
                    id: *game,
                    launcher: Launcher::Steam,
                    install_dir: steamapps.join("common").join(dir),
                });
            }
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Epic Games Launcher: JSON `.item` manifests.
// ---------------------------------------------------------------------------

/// A launch game from one Epic manifest (`%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests\*.item`).
pub fn epic_game(manifest_json: &str) -> Option<Found> {
    let value: serde_json::Value = serde_json::from_str(manifest_json).ok()?;
    let app = value.get("AppName")?.as_str()?;
    let id = match app {
        "Fortnite" => GameId::Fortnite,
        _ => return None,
    };
    let dir = value.get("InstallLocation")?.as_str()?;
    Some(Found {
        id,
        launcher: Launcher::Epic,
        install_dir: PathBuf::from(dir),
    })
}

// ---------------------------------------------------------------------------
// Riot Client: `*.product_settings.yaml`.
// ---------------------------------------------------------------------------

/// `product_install_full_path` from a Riot product settings file.
pub fn riot_install_path(product_settings_yaml: &str) -> Option<PathBuf> {
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(product_settings_yaml).ok()?;
    let path = value.get("product_install_full_path")?.as_str()?;
    Some(PathBuf::from(path.replace('/', "\\")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBRARYFOLDERS: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"228980"		"325556352"
			"730"		"38291029312"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps"
		{
			"570"		"40000000000"
			"1172470"		"75000000000"
		}
	}
}
"#;

    const APPMANIFEST_730: &str = r#"
"AppState"
{
	"appid"		"730"
	"name"		"Counter-Strike 2"
	"installdir"		"Counter-Strike Global Offensive"
}
"#;

    #[test]
    fn steam_library_folders() {
        let libs = steam_libraries(LIBRARYFOLDERS);
        assert_eq!(
            libs,
            [
                (
                    PathBuf::from(r"C:\Program Files (x86)\Steam"),
                    vec![228_980, 730]
                ),
                (PathBuf::from(r"D:\SteamLibrary"), vec![570, 1_172_470]),
            ]
        );
        assert_eq!(
            steam_installdir(APPMANIFEST_730).as_deref(),
            Some("Counter-Strike Global Offensive")
        );
    }

    #[test]
    fn steam_games_found_through_their_manifests() {
        let libs = steam_libraries(LIBRARYFOLDERS);
        let read = |p: &Path| -> Option<String> {
            let name = p.file_name()?.to_str()?;
            match name {
                "appmanifest_730.acf" => Some(APPMANIFEST_730.to_owned()),
                "appmanifest_570.acf" => {
                    Some("\"AppState\" { \"installdir\" \"dota 2 beta\" }".to_owned())
                }
                // Apex manifest missing: not reported.
                _ => None,
            }
        };
        let found = steam_games(&libs, &read);
        assert_eq!(
            found,
            [
                Found {
                    id: GameId::Cs2,
                    launcher: Launcher::Steam,
                    install_dir: PathBuf::from(r"C:\Program Files (x86)\Steam")
                        .join("steamapps")
                        .join("common")
                        .join("Counter-Strike Global Offensive"),
                },
                Found {
                    id: GameId::Dota2,
                    launcher: Launcher::Steam,
                    install_dir: PathBuf::from(r"D:\SteamLibrary")
                        .join("steamapps")
                        .join("common")
                        .join("dota 2 beta"),
                },
            ]
        );
    }

    #[test]
    fn vdf_is_lenient() {
        assert_eq!(parse_vdf(""), Vdf::Map(vec![]));
        let v = parse_vdf("// comment\n\"a\" { \"b\" \"c\" \n unquoted value }");
        assert_eq!(
            v.get("A").and_then(|a| a.get("b")).and_then(Vdf::as_str),
            Some("c")
        );
        assert_eq!(
            v.get("a")
                .and_then(|a| a.get("unquoted"))
                .and_then(Vdf::as_str),
            Some("value")
        );
        // Broken input does not panic.
        let _ = parse_vdf("\"a\" { \"b\" { {{ \"");
        assert!(steam_libraries("garbage").is_empty());
    }

    #[test]
    fn epic_manifests() {
        let fortnite = r#"{"FormatVersion":0,"AppName":"Fortnite","DisplayName":"Fortnite",
            "InstallLocation":"C:\\Program Files\\Epic Games\\Fortnite",
            "LaunchExecutable":"FortniteGame/Binaries/Win64/FortniteLauncher.exe"}"#;
        assert_eq!(
            epic_game(fortnite),
            Some(Found {
                id: GameId::Fortnite,
                launcher: Launcher::Epic,
                install_dir: PathBuf::from(r"C:\Program Files\Epic Games\Fortnite"),
            })
        );
        assert_eq!(
            epic_game(r#"{"AppName":"Other","InstallLocation":"C:\\x"}"#),
            None
        );
        assert_eq!(epic_game("not json"), None);
    }

    #[test]
    fn riot_product_settings() {
        let yaml = "product_install_full_path: \"C:/Riot Games/VALORANT/live\"\nproduct_install_root: \"C:/Riot Games\"\n";
        assert_eq!(
            riot_install_path(yaml),
            Some(PathBuf::from(r"C:\Riot Games\VALORANT\live"))
        );
        assert_eq!(riot_install_path("other: 1"), None);
    }
}
