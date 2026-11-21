//! Health checks (SPEC 4.1): pure functions over an [`HwInfo`] snapshot.
//!
//! They only explain; nothing here changes the system (owner decision, 2026-11-21). Each
//! [`Finding`] carries facts for the message; the texts live in the CLI translation files.

use serde::Serialize;

use crate::hw::{AppKind, DiskMedia, Game, GpuPreference, GpuVendor, HwInfo, PowerPlan};
use crate::recipe::FormFactor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Costs performance; worth fixing.
    Problem,
    /// May cost performance, or cannot be fixed easily.
    Warning,
    /// Good to know; a choice, not a fault.
    Info,
    /// Could not be checked on this PC.
    Unknown,
    Ok,
}

impl Outcome {
    pub fn key(self) -> &'static str {
        match self {
            Outcome::Problem => "problem",
            Outcome::Warning => "warning",
            Outcome::Info => "info",
            Outcome::Unknown => "unknown",
            Outcome::Ok => "ok",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    RamSpeed,
    RamChannels,
    DisplayRefresh,
    DisplayResolution,
    Hdr,
    AutoHdr,
    GpuDriver,
    Vbs,
    PowerPlan,
    OnBattery,
    GameGpu,
    GameDisk,
    Trim,
    DiskHealth,
    FreeSpace,
    OtherOptimizers,
    Overlays,
    Managed,
}

impl CheckId {
    pub const ALL: [CheckId; 18] = [
        CheckId::RamSpeed,
        CheckId::RamChannels,
        CheckId::DisplayRefresh,
        CheckId::DisplayResolution,
        CheckId::Hdr,
        CheckId::AutoHdr,
        CheckId::GpuDriver,
        CheckId::Vbs,
        CheckId::PowerPlan,
        CheckId::OnBattery,
        CheckId::GameGpu,
        CheckId::GameDisk,
        CheckId::Trim,
        CheckId::DiskHealth,
        CheckId::FreeSpace,
        CheckId::OtherOptimizers,
        CheckId::Overlays,
        CheckId::Managed,
    ];

    /// The outcomes this check can report besides OK and Unknown (each needs a message).
    pub fn reported_outcomes(self) -> &'static [Outcome] {
        use Outcome::{Info, Problem, Warning};
        match self {
            CheckId::RamSpeed => &[Warning, Info],
            CheckId::RamChannels | CheckId::PowerPlan => &[Problem, Warning],
            CheckId::DisplayRefresh | CheckId::Trim | CheckId::DiskHealth => &[Problem],
            CheckId::DisplayResolution
            | CheckId::GpuDriver
            | CheckId::OnBattery
            | CheckId::GameDisk
            | CheckId::FreeSpace
            | CheckId::OtherOptimizers => &[Warning],
            CheckId::Hdr
            | CheckId::AutoHdr
            | CheckId::Vbs
            | CheckId::Overlays
            | CheckId::Managed => &[Info],
            CheckId::GameGpu => &[Problem, Info],
        }
    }

    /// Key used in the translation files (`health.<key>.…`).
    pub fn key(self) -> &'static str {
        match self {
            CheckId::RamSpeed => "ram_speed",
            CheckId::RamChannels => "ram_channels",
            CheckId::DisplayRefresh => "display_refresh",
            CheckId::DisplayResolution => "display_resolution",
            CheckId::Hdr => "hdr",
            CheckId::AutoHdr => "auto_hdr",
            CheckId::GpuDriver => "gpu_driver",
            CheckId::Vbs => "vbs",
            CheckId::PowerPlan => "power_plan",
            CheckId::OnBattery => "on_battery",
            CheckId::GameGpu => "game_gpu",
            CheckId::GameDisk => "game_disk",
            CheckId::Trim => "trim",
            CheckId::DiskHealth => "disk_health",
            CheckId::FreeSpace => "free_space",
            CheckId::OtherOptimizers => "other_optimizers",
            CheckId::Overlays => "overlays",
            CheckId::Managed => "managed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub check: CheckId,
    pub outcome: Outcome,
    /// What it is about when a check runs per item: a monitor, a game, a drive.
    pub subject: Option<String>,
    /// Values for the message, e.g. `("current", "60")`.
    pub facts: Vec<(&'static str, String)>,
    /// Official page that helps (e.g. the GPU vendor's drivers).
    pub link: Option<&'static str>,
}

impl Finding {
    fn new(check: CheckId, outcome: Outcome) -> Self {
        Self {
            check,
            outcome,
            subject: None,
            facts: Vec::new(),
            link: None,
        }
    }

    fn about(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    fn fact(mut self, name: &'static str, value: impl ToString) -> Self {
        self.facts.push((name, value.to_string()));
        self
    }

    fn link(mut self, link: Option<&'static str>) -> Self {
        self.link = link;
        self
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.facts
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The words for a finding, in one language: the same for `pco health` and the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindingText {
    pub title: String,
    pub message: String,
    pub fix: String,
}

impl Finding {
    /// Title, message and how to fix it, from `health.<check>.…` in the translation files.
    /// The language-neutral facts `on`/`off` are translated.
    pub fn text(&self, t: &crate::i18n::Texts) -> FindingText {
        use std::fmt::Display;
        let mut args: Vec<(&str, &dyn Display)> = Vec::new();
        if let Some(s) = &self.subject {
            args.push(("subject", s));
        }
        for (name, value) in &self.facts {
            match value.as_str() {
                "on" => args.push((name, t.text("value.on"))),
                "off" => args.push((name, t.text("value.off"))),
                _ => args.push((name, value)),
            }
        }
        let key = self.check.key();
        FindingText {
            title: t.t(&format!("health.{key}.title")).to_owned(),
            message: t.f(&format!("health.{key}.{}", self.outcome.key()), &args),
            fix: t.f(&format!("health.{key}.fix"), &args),
        }
    }
}

/// A driver older than this many days gets a warning.
pub const DRIVER_MAX_AGE_DAYS: i64 = 183;
/// Free space below this share of the drive, or below this many bytes, gets a warning.
pub const FREE_SPACE_MIN_PERCENT: u64 = 10;
pub const FREE_SPACE_MIN_BYTES: u64 = 20 * 1024 * 1024 * 1024;

/// Standard (JEDEC) memory speeds: running exactly at one of these on a desktop often means
/// XMP/EXPO is off. Windows does not report the XMP speed itself.
const JEDEC_MHZ: &[u32] = &[2133, 2400, 2666, 2933, 3200, 4800, 5200, 5600];

/// Runs every check. `today` is days since 1970-01-01 (for the driver's age).
/// Results are sorted: problems first, OK last.
pub fn run(info: &HwInfo, today: i64) -> Vec<Finding> {
    let laptop = info.power.form_factor == Some(FormFactor::Laptop);
    let mut out = Vec::new();
    out.push(ram_speed(info, laptop));
    out.push(ram_channels(info, laptop));
    displays(info, &mut out);
    out.push(auto_hdr(info));
    gpu_drivers(info, today, &mut out);
    out.push(vbs(info));
    out.push(power_plan(info, laptop));
    if laptop {
        out.push(on_battery(info));
    }
    for game in &info.games {
        if let Some(f) = game_gpu(info, game) {
            out.push(f);
        }
        out.push(game_disk(info, game));
    }
    out.push(trim(info));
    disk_health(info, &mut out);
    free_space(info, &mut out);
    out.push(apps(info, AppKind::Optimizer));
    out.push(apps(info, AppKind::Overlay));
    out.push(managed(info));
    out.sort_by_key(|f| f.outcome);
    out
}

fn ram_speed(info: &HwInfo, laptop: bool) -> Finding {
    let known: Vec<_> = info
        .memory
        .iter()
        .filter_map(|m| Some((m.rated_mhz?, m.configured_mhz?)))
        .collect();
    let Some(&(rated, running)) = known.iter().min_by_key(|(_, c)| *c) else {
        return Finding::new(CheckId::RamSpeed, Outcome::Unknown);
    };
    let f = |o| {
        Finding::new(CheckId::RamSpeed, o)
            .fact("running", running)
            .fact("rated", rated)
    };
    // Allow small differences (e.g. 3200 vs 3196).
    if u64::from(running) * 100 < u64::from(rated) * 98 {
        f(Outcome::Warning)
    } else if !laptop && known.iter().all(|(_, c)| JEDEC_MHZ.contains(c)) {
        // Laptops rarely have XMP; on desktops a standard speed is worth a look in the BIOS.
        f(Outcome::Info)
    } else {
        f(Outcome::Ok)
    }
}

fn ram_channels(info: &HwInfo, laptop: bool) -> Finding {
    let count = info.memory.len();
    let f = |o| Finding::new(CheckId::RamChannels, o).fact("modules", count);
    match count {
        0 => f(Outcome::Unknown),
        1 => f(if laptop {
            Outcome::Warning
        } else {
            Outcome::Problem
        }),
        _ => {
            let keys: Option<Vec<String>> = info.memory.iter().map(|m| m.channel_key()).collect();
            match keys {
                Some(mut keys) => {
                    keys.sort();
                    keys.dedup();
                    if keys.len() >= 2 {
                        f(Outcome::Ok)
                    } else if laptop {
                        f(Outcome::Warning)
                    } else {
                        f(Outcome::Problem)
                    }
                }
                // Two or more modules, slots not named: most likely dual channel.
                None => f(Outcome::Unknown),
            }
        }
    }
}

fn displays(info: &HwInfo, out: &mut Vec<Finding>) {
    for d in &info.displays {
        let refresh = match d.max_refresh_hz {
            None => Outcome::Unknown,
            // 59 vs 60 Hz is the same mode.
            Some(max) if d.refresh_hz + 1 < max => Outcome::Problem,
            Some(_) => Outcome::Ok,
        };
        out.push(
            Finding::new(CheckId::DisplayRefresh, refresh)
                .about(&d.name)
                .fact("current", d.refresh_hz)
                .fact(
                    "max",
                    d.max_refresh_hz.map_or("?".into(), |m| m.to_string()),
                ),
        );

        let resolution = match (d.native_width, d.native_height) {
            (Some(w), Some(h)) if (w, h) != (d.width, d.height) => Outcome::Warning,
            (Some(_), Some(_)) => Outcome::Ok,
            _ => Outcome::Unknown,
        };
        out.push(
            Finding::new(CheckId::DisplayResolution, resolution)
                .about(&d.name)
                .fact("current", format!("{}×{}", d.width, d.height))
                .fact(
                    "native",
                    match (d.native_width, d.native_height) {
                        (Some(w), Some(h)) => format!("{w}×{h}"),
                        _ => "?".into(),
                    },
                ),
        );

        let hdr = match d.hdr_enabled {
            Some(true) => Outcome::Info,
            Some(false) => Outcome::Ok,
            None => Outcome::Unknown,
        };
        out.push(Finding::new(CheckId::Hdr, hdr).about(&d.name));
    }
}

fn auto_hdr(info: &HwInfo) -> Finding {
    // Not set means the Windows default, which is off.
    let outcome = if info.directx.auto_hdr == Some(true) {
        Outcome::Info
    } else {
        Outcome::Ok
    };
    Finding::new(CheckId::AutoHdr, outcome)
}

/// `YYYY-MM-DD` → days since 1970-01-01.
pub fn days_from_date(date: &str) -> Option<i64> {
    let mut parts = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn gpu_drivers(info: &HwInfo, today: i64, out: &mut Vec<Finding>) {
    for g in &info.gpus {
        let age = g
            .driver_date
            .as_deref()
            .and_then(days_from_date)
            .map(|d| today - d);
        let outcome = match age {
            None => Outcome::Unknown,
            Some(a) if a > DRIVER_MAX_AGE_DAYS => Outcome::Warning,
            Some(_) => Outcome::Ok,
        };
        out.push(
            Finding::new(CheckId::GpuDriver, outcome)
                .about(&g.name)
                .fact("version", g.driver_version.as_deref().unwrap_or("?"))
                .fact("date", g.driver_date.as_deref().unwrap_or("?"))
                .fact("months", age.map_or("?".into(), |a| (a / 30).to_string()))
                .link(g.vendor.driver_url()),
        );
    }
}

fn vbs(info: &HwInfo) -> Finding {
    let outcome = match (info.security.vbs_running, info.security.hvci_running) {
        (_, Some(true)) | (Some(true), _) => Outcome::Info,
        (Some(false), _) => Outcome::Ok,
        (None, _) => Outcome::Unknown,
    };
    Finding::new(CheckId::Vbs, outcome).fact(
        "hvci",
        info.security
            .hvci_running
            .map_or("?", |h| if h { "on" } else { "off" }),
    )
}

fn power_plan(info: &HwInfo, laptop: bool) -> Finding {
    let name = info.power.plan_name.clone().unwrap_or_else(|| "?".into());
    let outcome = match info.power.plan {
        None => Outcome::Unknown,
        Some(PowerPlan::PowerSaver) if laptop => Outcome::Warning,
        Some(PowerPlan::PowerSaver) => Outcome::Problem,
        Some(_) => Outcome::Ok,
    };
    Finding::new(CheckId::PowerPlan, outcome).fact("plan", name)
}

fn on_battery(info: &HwInfo) -> Finding {
    let outcome = match info.power.on_battery {
        Some(true) => Outcome::Warning,
        Some(false) => Outcome::Ok,
        None => Outcome::Unknown,
    };
    Finding::new(CheckId::OnBattery, outcome)
}

/// Only on PCs with both an integrated and a dedicated GPU.
fn game_gpu(info: &HwInfo, game: &Game) -> Option<Finding> {
    let has = |integrated| info.gpus.iter().any(|g| g.integrated == Some(integrated));
    if !(has(true) && has(false)) {
        return None;
    }
    let dedicated = info
        .gpus
        .iter()
        .find(|g| g.integrated == Some(false))
        .map_or("?", |g| g.name.as_str());
    let outcome = match game.gpu_preference {
        GpuPreference::PowerSaving => Outcome::Problem,
        GpuPreference::Default => Outcome::Info,
        GpuPreference::HighPerformance => Outcome::Ok,
    };
    Some(
        Finding::new(CheckId::GameGpu, outcome)
            .about(game.id.name())
            .fact("gpu", dedicated),
    )
}

fn game_disk(info: &HwInfo, game: &Game) -> Finding {
    let media = game
        .volume
        .and_then(|l| info.volumes.iter().find(|v| v.letter == l))
        .and_then(|v| v.disk)
        .and_then(|n| info.disks.iter().find(|d| d.number == n))
        .map(|d| d.media);
    let outcome = match media {
        Some(DiskMedia::Hdd) => Outcome::Warning,
        Some(DiskMedia::Ssd) => Outcome::Ok,
        _ => Outcome::Unknown,
    };
    Finding::new(CheckId::GameDisk, outcome)
        .about(game.id.name())
        .fact("drive", game.volume.map_or("?".into(), |l| format!("{l}:")))
}

fn trim(info: &HwInfo) -> Finding {
    let has_ssd = info.disks.iter().any(|d| d.media == DiskMedia::Ssd);
    let outcome = match info.trim_enabled {
        Some(false) if has_ssd => Outcome::Problem,
        Some(_) => Outcome::Ok,
        None => Outcome::Unknown,
    };
    Finding::new(CheckId::Trim, outcome)
}

fn disk_health(info: &HwInfo, out: &mut Vec<Finding>) {
    for d in &info.disks {
        let outcome = match d.healthy {
            Some(false) => Outcome::Problem,
            Some(true) => Outcome::Ok,
            None => Outcome::Unknown,
        };
        out.push(Finding::new(CheckId::DiskHealth, outcome).about(&d.model));
    }
}

fn free_space(info: &HwInfo, out: &mut Vec<Finding>) {
    let game_letters: Vec<char> = info.games.iter().filter_map(|g| g.volume).collect();
    for v in &info.volumes {
        if !(v.system || game_letters.contains(&v.letter)) || v.size_bytes == 0 {
            continue;
        }
        let low = v.free_bytes * 100 < v.size_bytes * FREE_SPACE_MIN_PERCENT
            || v.free_bytes < FREE_SPACE_MIN_BYTES;
        let gb = |b: u64| b / (1024 * 1024 * 1024);
        out.push(
            Finding::new(
                CheckId::FreeSpace,
                if low { Outcome::Warning } else { Outcome::Ok },
            )
            .about(format!("{}:", v.letter))
            .fact("free_gb", gb(v.free_bytes))
            .fact("size_gb", gb(v.size_bytes))
            .fact("percent", v.free_bytes * 100 / v.size_bytes),
        );
    }
}

fn apps(info: &HwInfo, kind: AppKind) -> Finding {
    let names: Vec<&str> = info
        .software
        .iter()
        .filter(|a| a.kind == kind)
        .map(|a| a.name.as_str())
        .collect();
    let (check, found) = match kind {
        AppKind::Optimizer => (CheckId::OtherOptimizers, Outcome::Warning),
        AppKind::Overlay => (CheckId::Overlays, Outcome::Info),
    };
    let outcome = if names.is_empty() { Outcome::Ok } else { found };
    Finding::new(check, outcome).fact("list", names.join(", "))
}

fn managed(info: &HwInfo) -> Finding {
    let outcome = match (info.windows.domain_joined, info.windows.mdm_enrolled) {
        (Some(true), _) | (_, Some(true)) => Outcome::Info,
        (Some(false), Some(false)) => Outcome::Ok,
        _ => Outcome::Unknown,
    };
    Finding::new(CheckId::Managed, outcome)
}

/// GPU vendor of the dedicated (or only) GPU, for messages.
pub fn main_gpu_vendor(info: &HwInfo) -> Option<GpuVendor> {
    info.gpus
        .iter()
        .find(|g| g.integrated == Some(false))
        .or(info.gpus.first())
        .map(|g| g.vendor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(days_from_date("1970-01-01"), Some(0));
        assert_eq!(days_from_date("2000-02-29"), Some(11_016));
        assert_eq!(days_from_date("2026-11-21"), Some(20_778));
        assert_eq!(days_from_date("2026-13-01"), None);
        assert_eq!(days_from_date("x"), None);
    }
}
