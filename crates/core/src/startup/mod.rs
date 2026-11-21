//! Programs that start with Windows (Stage 3, 3.12), the classic kinds Task Manager lists:
//! the `Run` registry keys and the Startup folders. Store apps are changed in Windows
//! Settings (owner decision, 2026-11-22).
//!
//! Turning a program off uses the same switch as Task Manager: a value under
//! `...\Explorer\StartupApproved\<place>` named like the program, whose first byte is even
//! when it may start and odd when it may not. The program's own entry is never touched, so
//! turning it on again is always possible. Each change goes through the journal as a
//! one-action recipe with the entry's ID, so "Undo all" and the uninstaller restore it.

#[cfg(windows)]
pub mod windows;

use std::path::PathBuf;

use serde::Serialize;

use crate::backend::{BackendError, RegData, RegRead, RegistryBackend, REG_SZ};
use crate::recipe::{
    Action, Category, FormFactor, Hive, I18n, Platform, Recipe, RegValue, RegistryAction,
    RegistryOp, RequiresRestart, Risk,
};

/// Where the switches live.
pub const APPROVED_ROOT: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";

/// Where a program is set to start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Location {
    /// `HKCU\...\Run`: this user.
    UserRun,
    /// `HKLM\...\Run`: every user (64-bit programs).
    MachineRun,
    /// `HKLM\...\WOW6432Node\...\Run`: every user (32-bit programs).
    MachineRun32,
    /// This user's Startup folder.
    UserFolder,
    /// The Startup folder of every user.
    CommonFolder,
}

pub const LOCATIONS: [Location; 5] = [
    Location::UserRun,
    Location::MachineRun,
    Location::MachineRun32,
    Location::UserFolder,
    Location::CommonFolder,
];

impl Location {
    pub fn slug(self) -> &'static str {
        match self {
            Location::UserRun => "user-run",
            Location::MachineRun => "machine-run",
            Location::MachineRun32 => "machine-run32",
            Location::UserFolder => "user-folder",
            Location::CommonFolder => "common-folder",
        }
    }

    /// The key holding this place's on/off switches.
    pub fn approved(self) -> (Hive, String) {
        let (hive, place) = match self {
            Location::UserRun => (Hive::CurrentUser, "Run"),
            Location::MachineRun => (Hive::LocalMachine, "Run"),
            Location::MachineRun32 => (Hive::LocalMachine, "Run32"),
            Location::UserFolder => (Hive::CurrentUser, "StartupFolder"),
            Location::CommonFolder => (Hive::LocalMachine, "StartupFolder"),
        };
        (hive, format!(r"{APPROVED_ROOT}\{place}"))
    }

    fn run_key(self) -> Option<(Hive, &'static str)> {
        match self {
            Location::UserRun => Some((Hive::CurrentUser, RUN)),
            Location::MachineRun => Some((Hive::LocalMachine, RUN)),
            Location::MachineRun32 => Some((Hive::LocalMachine, RUN32)),
            Location::UserFolder | Location::CommonFolder => None,
        }
    }

    /// Turning it on or off writes under HKLM.
    pub fn needs_admin(self) -> bool {
        self.approved().0 == Hive::LocalMachine
    }
}

/// How much a program is likely to slow down startup and the background. Windows does not
/// expose Task Manager's measured impact, so this is an estimate from a list of known
/// programs (owner decision, 2026-11-22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Impact {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Advice {
    /// Not needed at startup for gaming; start it when you use it.
    Off,
    /// Drivers, security, devices: leave it on.
    Keep,
}

/// A program this app knows: what it is (a translation key), impact, advice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Known {
    pub key: &'static str,
    pub impact: Impact,
    pub advice: Advice,
}

/// Known programs by executable name (lowercase).
const KNOWN: &[(&str, Known)] = &[
    ("onedrive.exe", known("onedrive", Impact::High, Advice::Off)),
    ("steam.exe", known("steam", Impact::High, Advice::Off)),
    (
        "epicgameslauncher.exe",
        known("epic", Impact::High, Advice::Off),
    ),
    ("eadesktop.exe", known("ea", Impact::High, Advice::Off)),
    (
        "battle.net.exe",
        known("battlenet", Impact::Medium, Advice::Off),
    ),
    (
        "riotclientservices.exe",
        known("riot", Impact::Medium, Advice::Off),
    ),
    ("discord.exe", known("discord", Impact::Medium, Advice::Off)),
    ("update.exe", known("discord", Impact::Medium, Advice::Off)),
    ("spotify.exe", known("spotify", Impact::Medium, Advice::Off)),
    ("ms-teams.exe", known("teams", Impact::High, Advice::Off)),
    ("teams.exe", known("teams", Impact::High, Advice::Off)),
    ("msedge.exe", known("edge", Impact::Medium, Advice::Off)),
    ("skype.exe", known("skype", Impact::Medium, Advice::Off)),
    (
        "securityhealthsystray.exe",
        known("security", Impact::Low, Advice::Keep),
    ),
    (
        "rtkauduservice64.exe",
        known("realtek", Impact::Low, Advice::Keep),
    ),
    ("igfxem.exe", known("intel_gpu", Impact::Low, Advice::Keep)),
    (
        "lghub.exe",
        known("peripherals", Impact::Medium, Advice::Keep),
    ),
    (
        "icue.exe",
        known("peripherals", Impact::Medium, Advice::Keep),
    ),
    (
        "razer synapse 3.exe",
        known("peripherals", Impact::Medium, Advice::Keep),
    ),
];

const fn known(key: &'static str, impact: Impact, advice: Advice) -> Known {
    Known {
        key,
        impact,
        advice,
    }
}

/// One program that starts with Windows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// Stable ID, also the journal's recipe ID: `startup.<place>.<hash of the name>`.
    pub id: String,
    pub location: Location,
    /// The value name (Run) or the file name (Startup folder).
    pub name: String,
    pub command: String,
    pub enabled: bool,
    pub known: Option<Known>,
}

/// `startup.user-run.1a2b3c4d`: the place plus a hash of the lowercase name (FNV-1a), so it
/// is a valid recipe ID whatever characters the name has.
pub fn entry_id(location: Location, name: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for b in name.to_lowercase().bytes() {
        hash ^= u32::from(b);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("startup.{}.{hash:08x}", location.slug())
}

/// The first byte of a StartupApproved value is odd when the program may not start.
pub fn enabled(approved: &RegRead) -> bool {
    match approved {
        RegRead::Present(d) => d.bytes.first().is_none_or(|b| b & 1 == 0),
        RegRead::KeyMissing | RegRead::ValueMissing => true,
    }
}

/// The switch Task Manager writes: 12 bytes, the first 2 (on) or 3 (off). The time stamp
/// Task Manager adds is left at zero, so the value is the same whenever it is written.
pub fn approved_value(enable: bool) -> RegValue {
    let mut bytes = vec![0u8; 12];
    bytes[0] = if enable { 2 } else { 3 };
    RegValue::Binary(bytes)
}

/// Text of a `REG_SZ` or `REG_EXPAND_SZ` value.
fn text(data: &RegData) -> Option<String> {
    if data.reg_type != REG_SZ && data.reg_type != 2 {
        return None;
    }
    let (pairs, _) = data.bytes.as_chunks::<2>();
    let mut units: Vec<u16> = pairs.iter().map(|p| u16::from_le_bytes(*p)).collect();
    while units.last() == Some(&0) {
        units.pop();
    }
    String::from_utf16(&units).ok()
}

/// The executable a command starts: `"C:\a b\x.exe" --flag` → `C:\a b\x.exe`.
pub fn exe_path(command: &str) -> Option<String> {
    let command = command.trim();
    let path = if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next()?.to_owned()
    } else {
        let lower = command.to_ascii_lowercase();
        match lower.find(".exe") {
            Some(end) => command[..end + 4].to_owned(),
            None => command.split_whitespace().next()?.to_owned(),
        }
    };
    (!path.is_empty()).then_some(path)
}

fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .to_lowercase()
}

pub fn known_program(command: &str) -> Option<Known> {
    let exe = file_name(&exe_path(command)?);
    KNOWN.iter().find(|(name, _)| *name == exe).map(|(_, k)| *k)
}

/// Size of the program started, with `%VARIABLES%` expanded. Reads only.
pub fn exe_size(command: &str) -> Option<u64> {
    let mut path = exe_path(command)?;
    while let Some(start) = path.find('%') {
        let end = start + 1 + path[start + 1..].find('%')?;
        let value = std::env::var(&path[start + 1..end]).ok()?;
        path.replace_range(start..=end, &value);
    }
    std::fs::metadata(PathBuf::from(path))
        .ok()
        .filter(std::fs::Metadata::is_file)
        .map(|m| m.len())
}

/// Every startup program: the Run keys from the registry, the Startup folders from
/// `folders` (place → full file paths), each with its switch. Reads only.
pub fn list(
    reg: &dyn RegistryBackend,
    folders: &[(Location, Vec<String>)],
) -> Result<Vec<Entry>, BackendError> {
    let mut out = Vec::new();
    let mut add = |location: Location, name: String, command: String| -> Result<(), BackendError> {
        let (hive, key) = location.approved();
        let switch = reg.read(hive, &key, &name)?;
        out.push(Entry {
            id: entry_id(location, &name),
            known: known_program(&command),
            location,
            name,
            command,
            enabled: enabled(&switch),
        });
        Ok(())
    };
    for location in LOCATIONS {
        if let Some((hive, key)) = location.run_key() {
            for name in reg.values(hive, key)? {
                if let RegRead::Present(data) = reg.read(hive, key, &name)? {
                    if let Some(command) = text(&data) {
                        add(location, name, command)?;
                    }
                }
            }
        }
    }
    for (location, files) in folders {
        for path in files {
            let name = file_name(path);
            if name == "desktop.ini" {
                continue;
            }
            let shown = path.rsplit(['\\', '/']).next().unwrap_or(path).to_owned();
            add(*location, shown, path.clone())?;
        }
    }
    Ok(out)
}

/// The one-action recipe that sets an entry's switch, as the journal records it.
pub fn recipe_for(entry: &Entry, enable: bool) -> Recipe {
    let (hive, key) = entry.location.approved();
    let text = |s: &str| I18n {
        ro: s.to_owned(),
        en: s.to_owned(),
    };
    Recipe {
        id: entry.id.clone(),
        version: 1,
        name: text(&entry.name),
        description: text(&entry.command),
        category: Category::Windows,
        risk: Risk::Safe,
        profiles: Vec::new(),
        impact: text("-"),
        why: text("-"),
        breaks: text("-"),
        requires_restart: RequiresRestart::None,
        platform: Platform {
            min_build: 22000,
            max_build: None,
            editions: Vec::new(),
            form_factor: FormFactor::Any,
        },
        sources: vec![
            "https://support.microsoft.com/en-us/windows/configure-startup-applications-in-windows-115a420a-0bff-4a6f-90e0-1934c844e473".into(),
        ],
        verified: false,
        actions: vec![Action::Registry(RegistryAction {
            hive,
            key,
            value: entry.name.clone(),
            op: RegistryOp::Set(approved_value(enable)),
            only_if: None,
        })],
        source_file: PathBuf::from("startup"),
    }
}

/// The journal-trust rule for startup entries: a switch value in one of the StartupApproved
/// keys, creating at most those keys or their parents.
pub fn is_approved_target(hive: Hive, key: &str, missing_keys: &[String]) -> bool {
    let place = LOCATIONS
        .iter()
        .map(|l| l.approved())
        .find(|(h, k)| *h == hive && k.eq_ignore_ascii_case(key));
    let Some((_, approved)) = place else {
        return false;
    };
    let approved = approved.to_ascii_lowercase();
    missing_keys.iter().all(|m| {
        let m = m.to_ascii_lowercase();
        !m.is_empty() && (m == approved || approved.starts_with(&format!("{m}\\")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::memory::MemoryRegistry;

    fn sz(s: &str) -> RegData {
        RegData::from_value(&RegValue::String(s.into()))
    }

    #[test]
    fn ids_are_stable_valid_recipe_ids() {
        let a = entry_id(Location::UserRun, "OneDrive");
        assert_eq!(a, entry_id(Location::UserRun, "onedrive"));
        assert_ne!(a, entry_id(Location::MachineRun, "OneDrive"));
        assert!(a.starts_with("startup.user-run."));
        assert_eq!(a.len(), "startup.user-run.".len() + 8);
    }

    #[test]
    fn commands_and_known_programs() {
        assert_eq!(
            exe_path(r#""C:\Program Files\Steam\steam.exe" -silent"#).as_deref(),
            Some(r"C:\Program Files\Steam\steam.exe")
        );
        assert_eq!(
            exe_path(r"C:\Windows\system32\SecurityHealthSystray.exe").as_deref(),
            Some(r"C:\Windows\system32\SecurityHealthSystray.exe")
        );
        let steam = known_program(r#""C:\Program Files (x86)\Steam\steam.exe" -silent"#).unwrap();
        assert_eq!((steam.impact, steam.advice), (Impact::High, Advice::Off));
        let sec = known_program(r"%windir%\system32\SecurityHealthSystray.exe").unwrap();
        assert_eq!(sec.advice, Advice::Keep);
        assert!(known_program(r"C:\Tools\mytool.exe").is_none());
    }

    #[test]
    fn lists_run_keys_and_folders_with_their_switches() {
        let (hive, approved) = Location::UserRun.approved();
        let reg = MemoryRegistry::new()
            .with_value(
                Hive::CurrentUser,
                RUN,
                "OneDrive",
                sz(r#""C:\OD\OneDrive.exe" /background"#),
            )
            .with_value(Hive::CurrentUser, RUN, "Tool", sz(r"C:\Tools\tool.exe"))
            .with_value(
                hive,
                &approved,
                "Tool",
                RegData::from_value(&approved_value(false)),
            );
        let folders = vec![(
            Location::UserFolder,
            vec![
                r"C:\Users\u\Startup\Notes.lnk".to_owned(),
                r"C:\Users\u\Startup\desktop.ini".to_owned(),
            ],
        )];
        let entries = list(&reg, &folders).unwrap();
        assert_eq!(entries.len(), 3);
        let od = entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case("OneDrive"))
            .unwrap();
        assert!(od.enabled);
        assert_eq!(od.known.unwrap().key, "onedrive");
        let tool = entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case("Tool"))
            .unwrap();
        assert!(!tool.enabled);
        let notes = entries
            .iter()
            .find(|e| e.location == Location::UserFolder)
            .unwrap();
        assert_eq!(notes.name, "Notes.lnk");
    }

    #[test]
    fn switches_and_trust() {
        assert!(enabled(&RegRead::ValueMissing));
        assert!(enabled(&RegRead::Present(RegData::from_value(
            &approved_value(true)
        ))));
        assert!(!enabled(&RegRead::Present(RegData::from_value(
            &approved_value(false)
        ))));
        assert!(enabled(&RegRead::Present(RegData {
            reg_type: 3,
            bytes: vec![6, 0, 0, 0]
        })));

        let (hive, key) = Location::MachineRun32.approved();
        assert!(is_approved_target(hive, &key, &[]));
        assert!(is_approved_target(hive, &key, std::slice::from_ref(&key)));
        assert!(!is_approved_target(
            hive,
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            &[]
        ));
        assert!(!is_approved_target(
            Hive::CurrentUser,
            &key.replace("Run32", "Other"),
            &[]
        ));
        assert!(!is_approved_target(hive, &key, &["Software\\Other".into()]));
    }

    #[test]
    fn recipe_for_writes_only_the_switch() {
        let entry = Entry {
            id: entry_id(Location::MachineRun, "X"),
            location: Location::MachineRun,
            name: "X".into(),
            command: "x.exe".into(),
            enabled: true,
            known: None,
        };
        let r = recipe_for(&entry, false);
        assert!(r.needs_admin());
        match &r.actions[..] {
            [Action::Registry(a)] => {
                assert_eq!(a.hive, Hive::LocalMachine);
                assert!(a.key.ends_with(r"StartupApproved\Run"));
                assert_eq!(a.op, RegistryOp::Set(approved_value(false)));
            }
            other => panic!("{other:?}"),
        }
    }
}
