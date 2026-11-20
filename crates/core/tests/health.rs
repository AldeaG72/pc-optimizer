//! Health checks on saved and constructed snapshots: for every check, "problem", "OK" and
//! "unknown" (ETAPA-1, exit criterion 2).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pco_core::health::{self, days_from_date, CheckId, Finding, Outcome};
use pco_core::hw::{
    AppKind, Disk, DiskMedia, Display, Game, GameId, Gpu, GpuPreference, GpuVendor, HwInfo,
    InstalledApp, Launcher, MemoryModule, PowerPlan, Volume,
};
use pco_core::recipe::FormFactor;

const GB: u64 = 1024 * 1024 * 1024;

fn today() -> i64 {
    days_from_date("2026-11-21").unwrap()
}

/// The development laptop, as detected for real (Intel integrated graphics only).
fn dev_laptop() -> HwInfo {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/hw/dev-laptop.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn find(findings: &[Finding], check: CheckId) -> Vec<&Finding> {
    findings.iter().filter(|f| f.check == check).collect()
}

fn outcome(findings: &[Finding], check: CheckId) -> Outcome {
    let found = find(findings, check);
    assert_eq!(found.len(), 1, "{check:?}: {found:?}");
    found[0].outcome
}

fn module(rated: u32, running: u32, slot: &str) -> MemoryModule {
    MemoryModule {
        capacity_mb: 16_384,
        rated_mhz: Some(rated),
        configured_mhz: Some(running),
        slot: Some(slot.into()),
        bank: None,
    }
}

fn gpu(name: &str, vendor: GpuVendor, integrated: bool, date: &str) -> Gpu {
    Gpu {
        name: name.into(),
        vendor,
        vendor_id: 0,
        device_id: 0,
        dedicated_mb: None,
        integrated: Some(integrated),
        driver_version: Some("1.0".into()),
        driver_date: Some(date.into()),
    }
}

fn game(id: GameId, drive: char, pref: GpuPreference) -> Game {
    Game {
        id,
        launcher: Launcher::Steam,
        install_dir: format!(r"{drive}:\Games"),
        exe: format!(r"{drive}:\Games\game.exe"),
        exe_exists: true,
        gpu_preference: pref,
        volume: Some(drive),
    }
}

/// A gaming desktop with many fixable problems.
fn troubled_desktop() -> HwInfo {
    let mut info = dev_laptop();
    info.power.form_factor = Some(FormFactor::Desktop);
    info.power.has_battery = Some(false);
    info.power.on_battery = Some(false);
    info.power.plan = Some(PowerPlan::PowerSaver);
    info.power.plan_name = Some("Power saver".into());
    info.memory = vec![module(3600, 2133, "DIMM_A2")];
    info.gpus = vec![gpu(
        "NVIDIA GeForce RTX 4070",
        GpuVendor::Nvidia,
        false,
        "2025-01-10",
    )];
    info.displays = vec![Display {
        name: "DELL S2721DGF".into(),
        primary: true,
        width: 1920,
        height: 1080,
        refresh_hz: 60,
        max_refresh_hz: Some(165),
        native_width: Some(2560),
        native_height: Some(1440),
        hdr_enabled: Some(true),
    }];
    info.directx.auto_hdr = Some(true);
    info.disks.push(Disk {
        number: 1,
        model: "ST2000DM008".into(),
        media: DiskMedia::Hdd,
        bus: Some("SATA".into()),
        size_bytes: 2000 * GB,
        healthy: Some(false),
    });
    info.volumes.push(Volume {
        letter: 'D',
        size_bytes: 2000 * GB,
        free_bytes: 500 * GB,
        disk: Some(1),
        system: false,
    });
    info.volumes[0].free_bytes = 10 * GB; // C: almost full
    info.trim_enabled = Some(false);
    info.games = vec![game(GameId::Cs2, 'D', GpuPreference::Default)];
    info.software = vec![
        InstalledApp {
            id: "razer-cortex".into(),
            name: "Razer Cortex".into(),
            kind: AppKind::Optimizer,
        },
        InstalledApp {
            id: "discord".into(),
            name: "Discord".into(),
            kind: AppKind::Overlay,
        },
    ];
    info.windows.domain_joined = Some(true);
    info
}

/// A laptop with integrated + dedicated graphics, on battery.
fn hybrid_laptop() -> HwInfo {
    let mut info = dev_laptop();
    info.gpus = vec![
        gpu(
            "Intel(R) UHD Graphics",
            GpuVendor::Intel,
            true,
            "2026-10-01",
        ),
        gpu(
            "NVIDIA GeForce RTX 4060 Laptop GPU",
            GpuVendor::Nvidia,
            false,
            "2026-10-01",
        ),
    ];
    info.games = vec![
        game(GameId::Valorant, 'C', GpuPreference::PowerSaving),
        game(GameId::Fortnite, 'C', GpuPreference::HighPerformance),
        game(GameId::LeagueOfLegends, 'C', GpuPreference::Default),
    ];
    info.memory = vec![module(5600, 5600, "DIMM 0")];
    info
}

/// A GitHub Actions runner: no monitor, many things unknown.
fn ci_runner() -> HwInfo {
    HwInfo {
        windows: pco_core::hw::WindowsInfo {
            build: 26100,
            ..Default::default()
        },
        ..HwInfo::default()
    }
}

#[test]
fn dev_laptop_has_only_real_findings() {
    let f = health::run(&dev_laptop(), today());
    assert_eq!(
        outcome(&f, CheckId::RamChannels),
        Outcome::Ok,
        "two controllers = dual channel"
    );
    assert_eq!(outcome(&f, CheckId::RamSpeed), Outcome::Ok);
    assert_eq!(outcome(&f, CheckId::DisplayRefresh), Outcome::Ok);
    assert_eq!(outcome(&f, CheckId::DisplayResolution), Outcome::Ok);
    assert_eq!(outcome(&f, CheckId::Hdr), Outcome::Ok);
    assert_eq!(outcome(&f, CheckId::GpuDriver), Outcome::Ok, "5 months old");
    assert_eq!(outcome(&f, CheckId::Vbs), Outcome::Info);
    assert_eq!(outcome(&f, CheckId::PowerPlan), Outcome::Ok);
    assert_eq!(
        outcome(&f, CheckId::OnBattery),
        Outcome::Warning,
        "unplugged when detected"
    );
    assert_eq!(outcome(&f, CheckId::Trim), Outcome::Ok);
    assert_eq!(outcome(&f, CheckId::DiskHealth), Outcome::Ok);
    assert_eq!(outcome(&f, CheckId::FreeSpace), Outcome::Ok);
    assert_eq!(outcome(&f, CheckId::OtherOptimizers), Outcome::Ok);
    assert_eq!(outcome(&f, CheckId::Overlays), Outcome::Ok);
    assert_eq!(outcome(&f, CheckId::Managed), Outcome::Ok);
    assert!(find(&f, CheckId::GameGpu).is_empty(), "no dedicated GPU");
    assert!(find(&f, CheckId::GameDisk).is_empty(), "no games");
}

#[test]
fn troubled_desktop_problems_are_found() {
    let f = health::run(&troubled_desktop(), today());
    assert_eq!(outcome(&f, CheckId::RamSpeed), Outcome::Warning);
    assert_eq!(
        outcome(&f, CheckId::RamChannels),
        Outcome::Problem,
        "one module on a desktop"
    );
    assert_eq!(outcome(&f, CheckId::DisplayRefresh), Outcome::Problem);
    let refresh = &find(&f, CheckId::DisplayRefresh)[0];
    assert_eq!(refresh.subject.as_deref(), Some("DELL S2721DGF"));
    assert_eq!(
        (refresh.get("current"), refresh.get("max")),
        (Some("60"), Some("165"))
    );
    assert_eq!(outcome(&f, CheckId::DisplayResolution), Outcome::Warning);
    assert_eq!(outcome(&f, CheckId::Hdr), Outcome::Info);
    assert_eq!(outcome(&f, CheckId::AutoHdr), Outcome::Info);
    assert_eq!(outcome(&f, CheckId::GpuDriver), Outcome::Warning);
    let driver = &find(&f, CheckId::GpuDriver)[0];
    assert_eq!(
        driver.link,
        Some("https://www.nvidia.com/Download/index.aspx")
    );
    assert_eq!(outcome(&f, CheckId::PowerPlan), Outcome::Problem);
    assert!(
        find(&f, CheckId::OnBattery).is_empty(),
        "desktops have no battery check"
    );
    assert_eq!(
        outcome(&f, CheckId::GameDisk),
        Outcome::Warning,
        "CS2 on the HDD"
    );
    assert_eq!(outcome(&f, CheckId::Trim), Outcome::Problem);
    let health: Vec<_> = find(&f, CheckId::DiskHealth)
        .iter()
        .map(|x| x.outcome)
        .collect();
    assert!(health.contains(&Outcome::Problem) && health.contains(&Outcome::Ok));
    let space: Vec<_> = find(&f, CheckId::FreeSpace)
        .iter()
        .map(|x| (x.subject.clone().unwrap(), x.outcome))
        .collect();
    assert!(
        space.contains(&("C:".into(), Outcome::Warning)),
        "{space:?}"
    );
    assert!(
        space.contains(&("D:".into(), Outcome::Ok)),
        "game drive checked too: {space:?}"
    );
    assert_eq!(outcome(&f, CheckId::OtherOptimizers), Outcome::Warning);
    assert_eq!(
        find(&f, CheckId::OtherOptimizers)[0].get("list"),
        Some("Razer Cortex")
    );
    assert_eq!(outcome(&f, CheckId::Overlays), Outcome::Info);
    assert_eq!(outcome(&f, CheckId::Managed), Outcome::Info);
    // Problems come first.
    assert_eq!(f.first().unwrap().outcome, Outcome::Problem);
    assert_eq!(f.last().unwrap().outcome, Outcome::Ok);
}

#[test]
fn desktop_at_a_standard_memory_speed_gets_an_xmp_hint() {
    let mut info = troubled_desktop();
    info.memory = vec![module(3200, 3200, "DIMM_A2"), module(3200, 3200, "DIMM_B2")];
    let f = health::run(&info, today());
    assert_eq!(outcome(&f, CheckId::RamSpeed), Outcome::Info);
    assert_eq!(outcome(&f, CheckId::RamChannels), Outcome::Ok);
    // Same modules, both on channel A: single channel.
    info.memory = vec![module(3200, 3200, "DIMM_A1"), module(3200, 3200, "DIMM_A2")];
    let f = health::run(&info, today());
    assert_eq!(outcome(&f, CheckId::RamChannels), Outcome::Problem);
}

#[test]
fn hybrid_laptop_game_gpu_and_battery() {
    let f = health::run(&hybrid_laptop(), today());
    let games: Vec<_> = find(&f, CheckId::GameGpu)
        .iter()
        .map(|x| (x.subject.clone().unwrap(), x.outcome))
        .collect();
    assert!(
        games.contains(&("VALORANT".into(), Outcome::Problem)),
        "{games:?}"
    );
    assert!(
        games.contains(&("Fortnite".into(), Outcome::Ok)),
        "{games:?}"
    );
    assert!(
        games.contains(&("League of Legends".into(), Outcome::Info)),
        "{games:?}"
    );
    let valorant = find(&f, CheckId::GameGpu)
        .into_iter()
        .find(|x| x.subject.as_deref() == Some("VALORANT"))
        .unwrap();
    assert_eq!(
        valorant.get("gpu"),
        Some("NVIDIA GeForce RTX 4060 Laptop GPU")
    );
    assert_eq!(outcome(&f, CheckId::OnBattery), Outcome::Warning);
    assert_eq!(
        outcome(&f, CheckId::RamChannels),
        Outcome::Warning,
        "one module on a laptop"
    );
    assert_eq!(
        outcome(&f, CheckId::RamSpeed),
        Outcome::Ok,
        "no XMP hint on laptops"
    );
    let disks: Vec<_> = find(&f, CheckId::GameDisk)
        .iter()
        .map(|x| x.outcome)
        .collect();
    assert_eq!(disks, [Outcome::Ok; 3]);
}

#[test]
fn ci_runner_reports_unknown_instead_of_guessing() {
    let f = health::run(&ci_runner(), today());
    for check in [
        CheckId::RamSpeed,
        CheckId::RamChannels,
        CheckId::Vbs,
        CheckId::PowerPlan,
        CheckId::Trim,
        CheckId::Managed,
    ] {
        assert_eq!(outcome(&f, check), Outcome::Unknown, "{check:?}");
    }
    // Nothing per-item when there is nothing to check.
    for check in [
        CheckId::DisplayRefresh,
        CheckId::GpuDriver,
        CheckId::DiskHealth,
        CheckId::GameDisk,
    ] {
        assert!(find(&f, check).is_empty(), "{check:?}");
    }
    assert!(
        find(&f, CheckId::OnBattery).is_empty(),
        "form factor unknown"
    );
    assert!(f.iter().all(|x| x.outcome != Outcome::Problem));
}

#[test]
fn unknown_details_stay_unknown() {
    let mut info = troubled_desktop();
    info.displays[0].max_refresh_hz = None;
    info.displays[0].native_width = None;
    info.displays[0].hdr_enabled = None;
    info.gpus[0].driver_date = None;
    info.disks[1].healthy = None;
    info.games[0].volume = None;
    info.security = Default::default();
    info.trim_enabled = None;
    let f = health::run(&info, today());
    for check in [
        CheckId::DisplayRefresh,
        CheckId::DisplayResolution,
        CheckId::Hdr,
        CheckId::GpuDriver,
        CheckId::GameDisk,
        CheckId::Vbs,
        CheckId::Trim,
    ] {
        assert_eq!(outcome(&f, check), Outcome::Unknown, "{check:?}");
    }
    let disks: Vec<_> = find(&f, CheckId::DiskHealth)
        .iter()
        .map(|x| x.outcome)
        .collect();
    assert!(disks.contains(&Outcome::Unknown));
}

#[test]
fn findings_serialize_for_json_export() {
    let f = health::run(&troubled_desktop(), today());
    let json = serde_json::to_value(&f).unwrap();
    let first = &json[0];
    assert_eq!(first["outcome"], "problem");
    assert!(first["check"].is_string());
}
