//! Hardware and system detection (Stage 1). Read-only by design: nothing in this module
//! can change the system.
//!
//! [`HwInfo`] is a plain snapshot that serializes to JSON, so health checks are pure
//! functions tested on saved snapshots, on any OS. `None` always means "could not be read",
//! never "no" — checks report it as unknown instead of guessing.
//!
//! Privacy: no user, computer or network names are collected.

pub mod apps;
pub mod games;
#[cfg(windows)]
pub mod windows;

use serde::{Deserialize, Serialize};

use crate::recipe::{Edition, FormFactor};

/// Version of the JSON layout.
pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HwInfo {
    pub schema: u32,
    pub windows: WindowsInfo,
    pub cpu: Option<Cpu>,
    pub memory: Vec<MemoryModule>,
    pub gpus: Vec<Gpu>,
    pub displays: Vec<Display>,
    pub directx: DirectXSettings,
    pub disks: Vec<Disk>,
    pub volumes: Vec<Volume>,
    /// TRIM for NTFS (`fsutil behavior query DisableDeleteNotify`).
    pub trim_enabled: Option<bool>,
    pub power: Power,
    pub security: Security,
    pub software: Vec<InstalledApp>,
    pub games: Vec<Game>,
    /// Parts that could not be read, e.g. `"displays: access denied"`.
    pub probe_errors: Vec<String>,
}

impl Default for HwInfo {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            windows: WindowsInfo::default(),
            cpu: None,
            memory: Vec::new(),
            gpus: Vec::new(),
            displays: Vec::new(),
            directx: DirectXSettings::default(),
            disks: Vec::new(),
            volumes: Vec::new(),
            trim_enabled: None,
            power: Power::default(),
            security: Security::default(),
            software: Vec::new(),
            games: Vec::new(),
            probe_errors: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsInfo {
    pub build: u32,
    pub edition: Option<Edition>,
    /// e.g. `24H2`.
    pub display_version: Option<String>,
    pub domain_joined: Option<bool>,
    /// Enrolled in Intune or another MDM.
    pub mdm_enrolled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cpu {
    pub name: String,
    pub cores: u32,
    pub threads: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryModule {
    pub capacity_mb: u64,
    /// The module's rated speed (what XMP/EXPO would give).
    pub rated_mhz: Option<u32>,
    /// The speed it actually runs at.
    pub configured_mhz: Option<u32>,
    /// `DeviceLocator`, e.g. `DIMM_A1` or `ChannelA-DIMM0`.
    pub slot: Option<String>,
    /// `BankLabel`, e.g. `BANK 0` or `P0 CHANNEL A`.
    pub bank: Option<String>,
}

impl MemoryModule {
    /// Which memory channel the module is on, as a key: two modules on different keys run in
    /// dual channel. Includes the memory controller when the slot names it, because on recent
    /// Intel/AMD systems `Controller0-ChannelA` and `Controller1-ChannelA` are two different
    /// channels. `None` if the names do not say.
    pub fn channel_key(&self) -> Option<String> {
        let letter = self.channel_letter()?;
        let controller = [&self.slot, &self.bank]
            .into_iter()
            .flatten()
            .find_map(|text| {
                let upper = text.to_ascii_uppercase();
                let pos = upper.find("CONTROLLER")? + "CONTROLLER".len();
                let digits: String = upper[pos..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                (!digits.is_empty()).then_some(digits)
            })
            .unwrap_or_default();
        Some(format!("{controller}{letter}"))
    }

    fn channel_letter(&self) -> Option<char> {
        for text in [&self.slot, &self.bank].into_iter().flatten() {
            let upper = text.to_ascii_uppercase();
            if let Some(pos) = upper.find("CHANNEL") {
                let rest = upper[pos + "CHANNEL".len()..].trim_start_matches([' ', '_', '-']);
                if let Some(c) = rest.chars().next().filter(char::is_ascii_alphabetic) {
                    return Some(c);
                }
            }
            // `DIMM_A1`, `DIMM A2`: the letter after DIMM is the channel on most boards.
            if let Some(pos) = upper.find("DIMM") {
                let rest = upper[pos + "DIMM".len()..].trim_start_matches([' ', '_', '-']);
                let mut chars = rest.chars();
                if let (Some(c), Some(d)) = (chars.next(), chars.next()) {
                    if c.is_ascii_alphabetic() && d.is_ascii_digit() {
                        return Some(c);
                    }
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

impl GpuVendor {
    pub fn from_pci_id(id: u16) -> Self {
        match id {
            0x10DE => GpuVendor::Nvidia,
            0x1002 | 0x1022 => GpuVendor::Amd,
            0x8086 => GpuVendor::Intel,
            _ => GpuVendor::Other,
        }
    }

    /// The vendor's official driver page.
    pub fn driver_url(self) -> Option<&'static str> {
        match self {
            GpuVendor::Nvidia => Some("https://www.nvidia.com/Download/index.aspx"),
            GpuVendor::Amd => Some("https://www.amd.com/en/support/download/drivers.html"),
            GpuVendor::Intel => {
                Some("https://www.intel.com/content/www/us/en/download-center/home.html")
            }
            GpuVendor::Other => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gpu {
    pub name: String,
    pub vendor: GpuVendor,
    pub vendor_id: u16,
    pub device_id: u16,
    pub dedicated_mb: Option<u64>,
    /// Shares system memory (Direct3D 12 "UMA"): integrated graphics.
    pub integrated: Option<bool>,
    pub driver_version: Option<String>,
    /// `YYYY-MM-DD`.
    pub driver_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Display {
    /// Monitor name, e.g. `DELL S2721DGF`.
    pub name: String,
    pub primary: bool,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    /// Highest refresh rate available at the current resolution.
    pub max_refresh_hz: Option<u32>,
    /// The monitor's preferred (native) resolution.
    pub native_width: Option<u32>,
    pub native_height: Option<u32>,
    pub hdr_enabled: Option<bool>,
}

/// Settings › Display › Graphics › Advanced graphics settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectXSettings {
    pub auto_hdr: Option<bool>,
    /// "Optimizations for windowed games".
    pub windowed_optimizations: Option<bool>,
    /// "Variable refresh rate".
    pub variable_refresh_rate: Option<bool>,
}

impl DirectXSettings {
    /// Parses `DirectXUserGlobalSettings`, e.g. `AutoHDREnable=1;VRROptimizeEnable=0;`.
    /// A setting that is not in the string stays `None` (Windows default, not known here).
    pub fn parse(data: &str) -> Self {
        let flag = |name: &str| {
            data.split(';').find_map(|part| {
                let (k, v) = part.split_once('=')?;
                (k.trim() == name).then(|| v.trim() == "1")
            })
        };
        Self {
            auto_hdr: flag("AutoHDREnable"),
            windowed_optimizations: flag("SwapEffectUpgradeEnable"),
            variable_refresh_rate: flag("VRROptimizeEnable"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiskMedia {
    Ssd,
    Hdd,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disk {
    pub number: u32,
    pub model: String,
    pub media: DiskMedia,
    /// e.g. `NVMe`, `SATA`, `USB`.
    pub bus: Option<String>,
    pub size_bytes: u64,
    /// Windows' own health status (from SMART); `None` if not reported.
    pub healthy: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Volume {
    pub letter: char,
    pub size_bytes: u64,
    pub free_bytes: u64,
    /// Number of the physical disk it is on.
    pub disk: Option<u32>,
    /// The Windows drive.
    pub system: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerPlan {
    Balanced,
    HighPerformance,
    UltimatePerformance,
    PowerSaver,
    Custom,
}

impl PowerPlan {
    /// Built-in plans by their fixed GUID.
    pub fn from_guid(guid: &str) -> Self {
        match guid.trim_matches(['{', '}']).to_ascii_lowercase().as_str() {
            "381b4222-f694-41f0-9685-ff5bb260df2e" => PowerPlan::Balanced,
            "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c" => PowerPlan::HighPerformance,
            "e9a42b02-d5df-448d-aa00-03f14749eb61" => PowerPlan::UltimatePerformance,
            "a1841308-3541-4fab-bc81-f71556f20b4a" => PowerPlan::PowerSaver,
            _ => PowerPlan::Custom,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Power {
    pub plan: Option<PowerPlan>,
    pub plan_name: Option<String>,
    pub has_battery: Option<bool>,
    pub on_battery: Option<bool>,
    pub form_factor: Option<FormFactor>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Security {
    /// Virtualization-based security running.
    pub vbs_running: Option<bool>,
    /// Memory Integrity (HVCI) running.
    pub hvci_running: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppKind {
    /// Another optimizer: its tweaks may conflict with ours.
    Optimizer,
    /// Draws over games (FPS cost, input lag on some setups).
    Overlay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledApp {
    /// Id from the known-apps list, e.g. `discord`.
    pub id: String,
    /// Name as installed.
    pub name: String,
    pub kind: AppKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameId {
    Cs2,
    Dota2,
    Apex,
    Fortnite,
    Valorant,
    LeagueOfLegends,
}

impl GameId {
    pub fn name(self) -> &'static str {
        match self {
            GameId::Cs2 => "Counter-Strike 2",
            GameId::Dota2 => "Dota 2",
            GameId::Apex => "Apex Legends",
            GameId::Fortnite => "Fortnite",
            GameId::Valorant => "VALORANT",
            GameId::LeagueOfLegends => "League of Legends",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Launcher {
    Steam,
    Epic,
    Riot,
    Ea,
}

/// Settings › Display › Graphics, per app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuPreference {
    /// "Let Windows decide".
    Default,
    /// "Power saving" (usually the integrated GPU).
    PowerSaving,
    /// "High performance" (usually the dedicated GPU).
    HighPerformance,
}

impl GpuPreference {
    /// Parses `GpuPreference=2;` as stored under `UserGpuPreferences`.
    pub fn parse(data: &str) -> Self {
        let value = data
            .split(';')
            .find_map(|part| part.trim().strip_prefix("GpuPreference="))
            .map(str::trim);
        match value {
            Some("1") => GpuPreference::PowerSaving,
            Some("2") => GpuPreference::HighPerformance,
            _ => GpuPreference::Default,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub id: GameId,
    pub launcher: Launcher,
    pub install_dir: String,
    /// Main executable (full path).
    pub exe: String,
    pub exe_exists: bool,
    pub gpu_preference: GpuPreference,
    /// Drive letter the game is on.
    pub volume: Option<char>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(slot: &str, bank: &str) -> MemoryModule {
        MemoryModule {
            capacity_mb: 8192,
            rated_mhz: None,
            configured_mhz: None,
            slot: Some(slot.into()),
            bank: Some(bank.into()),
        }
    }

    #[test]
    fn memory_channels() {
        let key = |slot, bank| module(slot, bank).channel_key();
        assert_eq!(key("ChannelA-DIMM0", "BANK 0").as_deref(), Some("A"));
        assert_eq!(key("DIMM 0", "P0 CHANNEL B").as_deref(), Some("B"));
        assert_eq!(key("DIMM_A1", "BANK 0").as_deref(), Some("A"));
        assert_eq!(key("DIMM_B2", "BANK 2").as_deref(), Some("B"));
        // The development laptop: two controllers, each with a channel A = dual channel.
        assert_eq!(key("Controller0-ChannelA", "BANK 0").as_deref(), Some("0A"));
        assert_eq!(key("Controller1-ChannelA", "BANK 0").as_deref(), Some("1A"));
        assert_eq!(key("DIMM 0", "BANK 0"), None);
        assert_eq!(key("XMM1", "BANK 0"), None);
    }

    #[test]
    fn power_plans_and_gpu_preferences() {
        assert_eq!(
            PowerPlan::from_guid("{8C5E7FDA-E8BF-4A96-9A85-A6E23A8C635C}"),
            PowerPlan::HighPerformance
        );
        assert_eq!(PowerPlan::from_guid("1234"), PowerPlan::Custom);
        assert_eq!(
            GpuPreference::parse("GpuPreference=2;"),
            GpuPreference::HighPerformance
        );
        assert_eq!(
            GpuPreference::parse("SwapEffectUpgradeEnable=1;GpuPreference=1;"),
            GpuPreference::PowerSaving
        );
        assert_eq!(
            GpuPreference::parse("GpuPreference=0;"),
            GpuPreference::Default
        );
        assert_eq!(GpuPreference::parse(""), GpuPreference::Default);
    }

    #[test]
    fn directx_global_settings() {
        let s = DirectXSettings::parse("SwapEffectUpgradeEnable=1;AutoHDREnable=0;");
        assert_eq!(s.windowed_optimizations, Some(true));
        assert_eq!(s.auto_hdr, Some(false));
        assert_eq!(s.variable_refresh_rate, None);
        assert_eq!(DirectXSettings::parse(""), DirectXSettings::default());
    }

    #[test]
    fn snapshot_round_trips_as_json() {
        let info = HwInfo {
            memory: vec![module("DIMM_A1", "BANK 0")],
            ..HwInfo::default()
        };
        let json = serde_json::to_string(&info).unwrap();
        assert_eq!(serde_json::from_str::<HwInfo>(&json).unwrap(), info);
    }
}
