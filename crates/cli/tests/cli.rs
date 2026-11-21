//! `pco` commands end to end, on in-memory backends with the real built-in recipes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use pco_cli::args::{BenchCmd, BenchRun};
use pco_cli::bench::BenchDriver;
use pco_core::hw::GameId;
use std::ffi::OsString;
use std::path::Path;

use pco_cli::app::{run, Env, Exit};
use pco_cli::args::{self, Cmd};
use pco_cli::embedded::RECIPES;
use pco_cli::i18n::{self, Texts};
use pco_core::backend::memory::{MemoryAppx, MemoryPower, MemoryRegistry, MemoryServices};
use pco_core::backend::{RegData, RegRead, RegistryBackend, ServiceStart, ServiceState};
use pco_core::guard::{Arch, BlockedProcesses, Host};
use pco_core::journal::{EntryKind, Journal, NewEntry, SessionKind};
use pco_core::recipe::{self, Edition, Hive, Lang, RegValue};

fn win11(elevated: bool) -> Host {
    Host {
        build: 26100,
        arch: Arch::X64,
        elevated,
        edition: Some(Edition::Pro),
        form_factor: None,
    }
}

/// Simulated PresentMon, launchers, keyboard and clock.
#[derive(Default)]
struct FakeBench {
    presentmon_error: Option<String>,
    /// Processes running now.
    running: Vec<String>,
    /// Starts running once a game is launched.
    starts_on_launch: Option<String>,
    launch_error: Option<String>,
    launched: Vec<GameId>,
    /// Answers to "press Enter" (`false` = the user stopped); missing = Enter.
    enters: VecDeque<bool>,
    /// Average FPS of each capture; a capture without a value fails.
    fps: VecDeque<f64>,
    captures: Vec<(String, u32)>,
    slept: u32,
}

impl BenchDriver for FakeBench {
    fn presentmon(&mut self) -> Result<(), String> {
        self.presentmon_error.clone().map_or(Ok(()), Err)
    }
    fn is_running(&mut self, process: &str) -> bool {
        self.running.iter().any(|p| p.eq_ignore_ascii_case(process))
    }
    fn launch(&mut self, game: GameId) -> Result<(), String> {
        if let Some(e) = &self.launch_error {
            return Err(e.clone());
        }
        self.launched.push(game);
        if let Some(p) = self.starts_on_launch.take() {
            self.running.push(p);
        }
        Ok(())
    }
    fn wait_for_enter(&mut self) -> bool {
        self.enters.pop_front().unwrap_or(true)
    }
    fn sleep(&mut self, seconds: u32) {
        self.slept += seconds;
    }
    fn capture(&mut self, process: &str, seconds: u32) -> Result<String, String> {
        self.captures.push((process.to_owned(), seconds));
        let fps = self.fps.pop_front().ok_or("simulated PresentMon failure")?;
        let mut csv = String::from("Application,SwapChainAddress,MsBetweenPresents\n");
        for _ in 0..(fps as usize * 10) {
            csv.push_str(&format!("{process},0x1,{:.6}\n", 1000.0 / fps));
        }
        Ok(csv)
    }
    fn now_ms(&mut self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }
}

struct World {
    reg: MemoryRegistry,
    svc: MemoryServices,
    power: MemoryPower,
    apps: MemoryAppx,
    journal: Journal,
    host: Host,
    running: Vec<String>,
    bench: FakeBench,
}

impl World {
    fn new() -> Self {
        Self {
            // A fresh Windows: only the default Sticky Keys value (that recipe requires it).
            reg: MemoryRegistry::new().with_value(
                Hive::CurrentUser,
                r"Control Panel\Accessibility\StickyKeys",
                "Flags",
                RegData::from_value(&RegValue::String("510".into())),
            ),
            svc: MemoryServices::new().with_service(
                "Fax",
                ServiceState {
                    start: ServiceStart::Manual,
                    delayed_auto_start: false,
                },
            ),
            power: MemoryPower::new("381b4222-f694-41f0-9685-ff5bb260df2e"),
            apps: MemoryAppx::new(),
            journal: Journal::open_in_memory().unwrap(),
            host: win11(false),
            running: vec!["explorer.exe".into()],
            bench: FakeBench::default(),
        }
    }

    fn run(&mut self, lang: Lang, cmd: Cmd) -> (Exit, String) {
        let texts = Texts::load(lang).unwrap();
        let mut env = Env {
            registry: &mut self.reg,
            services: &mut self.svc,
            power: &mut self.power,
            apps: &mut self.apps,
            journal: &mut self.journal,
            host: self.host.clone(),
            running: self.running.clone(),
            blocked: BlockedProcesses::builtin().unwrap(),
            recipes: recipe::load_embedded(RECIPES).unwrap(),
            hw: Some(dev_laptop()),
            today: pco_core::health::days_from_date("2026-11-21").unwrap(),
            bench: &mut self.bench,
        };
        let mut out = Vec::new();
        let exit = run(&mut env, &texts, &cmd, &mut out);
        (exit, String::from_utf8(out).unwrap())
    }

    fn ro(&mut self, cmd: Cmd) -> (Exit, String) {
        self.run(Lang::Ro, cmd)
    }
}

/// The development laptop's real hardware snapshot (shared with the core tests).
fn dev_laptop() -> pco_core::hw::HwInfo {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/hw/dev-laptop.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

fn apply(list: &[&str]) -> Cmd {
    Cmd::Apply {
        ids: ids(list),
        dry_run: false,
    }
}

const DVR: &str = "gaming.game-dvr-off";

// --- translations and built-in recipes ---------------------------------------------------

fn translation(lang: Lang) -> BTreeMap<String, String> {
    serde_yaml_ng::from_str(i18n::source(lang)).unwrap()
}

/// Placeholder names; a plural choice `{n|one|few|other}` counts as `n`.
fn placeholders(text: &str) -> BTreeSet<String> {
    text.split('{')
        .skip(1)
        .filter_map(|s| {
            s.split_once('}')
                .map(|(inner, _)| inner.split('|').next().unwrap_or_default().to_owned())
        })
        .collect()
}

#[test]
fn plurals_are_correct_in_both_languages() {
    let ro = Texts::load(Lang::Ro).unwrap();
    let en = Texts::load(Lang::En).unwrap();
    assert_eq!(ro.f("bench.forgot", &[("count", &1)]), "Am șters 1 rulare.");
    assert_eq!(ro.f("bench.forgot", &[("count", &3)]), "Am șters 3 rulări.");
    assert_eq!(
        ro.f("bench.forgot", &[("count", &20)]),
        "Am șters 20 de rulări."
    );
    assert_eq!(en.f("bench.forgot", &[("count", &1)]), "Deleted 1 run.");
    assert_eq!(en.f("bench.forgot", &[("count", &2)]), "Deleted 2 runs.");
    // Every plural choice has 3 forms in Romanian and 2 in English.
    for (lang, forms) in [(Lang::Ro, 3), (Lang::En, 2)] {
        for (key, text) in translation(lang) {
            for inner in text
                .split('{')
                .skip(1)
                .filter_map(|s| s.split_once('}'))
                .map(|(i, _)| i)
            {
                let parts = inner.split('|').count();
                assert!(
                    parts == 1 || parts == forms + 1,
                    "`{key}` ({lang:?}): {{{inner}}}"
                );
            }
        }
    }
}

#[test]
fn translations_have_the_same_keys_and_placeholders() {
    let (ro, en) = (translation(Lang::Ro), translation(Lang::En));
    let ro_keys: BTreeSet<_> = ro.keys().collect();
    let en_keys: BTreeSet<_> = en.keys().collect();
    assert_eq!(ro_keys, en_keys, "keys differ between ro.yaml and en.yaml");
    for (key, ro_text) in &ro {
        assert_eq!(
            placeholders(ro_text),
            placeholders(&en[key]),
            "placeholders differ for `{key}`"
        );
        assert!(!ro_text.trim().is_empty(), "`{key}` is empty");
    }
}

#[test]
fn every_key_used_in_the_code_exists() {
    let ro = translation(Lang::Ro);
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in ["app.rs", "format.rs", "args.rs", "main.rs"] {
        let code = std::fs::read_to_string(src.join(file)).unwrap();
        for part in code.split('"').skip(1).step_by(2) {
            let looks_like_key = part.contains('.')
                && !part.contains(' ')
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_')
                && ro
                    .keys()
                    .any(|k| k.split('.').next() == part.split('.').next());
            if looks_like_key {
                assert!(
                    ro.contains_key(part),
                    "{file}: missing translation `{part}`"
                );
            }
        }
    }
}

#[test]
fn built_in_recipes_match_the_repository() {
    let embedded = recipe::load_embedded(RECIPES).unwrap();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../recipes");
    let on_disk = recipe::load_dir(&dir).unwrap();
    let a: Vec<_> = embedded.iter().map(|r| (&r.id, &r.actions)).collect();
    let b: Vec<_> = on_disk.iter().map(|r| (&r.id, &r.actions)).collect();
    assert_eq!(a, b);
    assert_eq!(embedded.len(), 39);
}

// --- arguments ----------------------------------------------------------------------------

fn parse(line: &str) -> Result<Cmd, clap::Error> {
    let argv: Vec<OsString> = line.split_whitespace().map(OsString::from).collect();
    args::parse(argv, &Texts::load(Lang::En).unwrap())
}

#[test]
fn arguments() {
    assert_eq!(parse("pco list").unwrap(), Cmd::List);
    assert_eq!(parse("pco --lang ro status").unwrap(), Cmd::Status);
    assert_eq!(
        parse("pco apply a.b c.d --dry-run").unwrap(),
        Cmd::Apply {
            ids: ids(&["a.b", "c.d"]),
            dry_run: true
        }
    );
    assert_eq!(
        parse("pco undo --all").unwrap(),
        Cmd::Undo {
            ids: vec![],
            all: true
        }
    );
    assert!(parse("pco undo").is_err(), "undo needs ids or --all");
    assert!(parse("pco undo a.b --all").is_err(), "not both");
    assert!(parse("pco plan").is_err(), "plan needs ids");
    assert!(parse("pco apply").is_err(), "apply needs ids");
    assert!(parse("pco --lang de list").is_err());
    assert!(parse("pco").is_err());

    let argv = |s: &str| -> Vec<OsString> { s.split_whitespace().map(OsString::from).collect() };
    assert_eq!(
        args::lang_from_args(&argv("pco --lang ro list")),
        Some(Lang::Ro)
    );
    assert_eq!(
        args::lang_from_args(&argv("pco list --lang=en")),
        Some(Lang::En)
    );
    assert_eq!(args::lang_from_args(&argv("pco list")), None);
}

// --- commands -----------------------------------------------------------------------------

#[test]
fn list_shows_every_recipe_and_its_state() {
    let mut w = World::new();
    let (exit, out) = w.ro(Cmd::List);
    assert_eq!(exit, Exit::Ok);
    assert!(out.starts_with("ID"), "{out}");
    for r in recipe::load_embedded(RECIPES).unwrap() {
        assert!(out.contains(&r.id), "{} missing:\n{out}", r.id);
    }
    assert!(out.contains("neaplicat"), "{out}");

    w.ro(apply(&[DVR]));
    let (_, out) = w.ro(Cmd::List);
    let line = out.lines().find(|l| l.starts_with(DVR)).unwrap();
    assert!(
        line.ends_with("aplicat") && !line.ends_with("neaplicat"),
        "{line}"
    );
}

#[test]
fn plan_shows_old_and_new_values_without_changing_anything() {
    let mut w = World::new();
    let before = w.reg.clone();
    let (exit, out) = w.ro(Cmd::Plan { ids: ids(&[DVR]) });
    assert_eq!(exit, Exit::Ok);
    assert!(out.contains("Captură în fundal (Game DVR) oprită"), "{out}");
    assert!(
        out.contains(r"HKCU\System\GameConfigStore\GameDVR_Enabled: cheia nu există → 0"),
        "{out}"
    );
    assert!(w.reg.same_contents(&before));
    assert!(w.journal.sessions().unwrap().is_empty());
}

#[test]
fn dry_run_changes_nothing() {
    let mut w = World::new();
    let before = w.reg.clone();
    let (exit, out) = w.ro(Cmd::Apply {
        ids: ids(&[DVR]),
        dry_run: true,
    });
    assert_eq!(exit, Exit::Ok);
    assert!(out.contains("Simulare: nu am schimbat nimic."), "{out}");
    assert!(w.reg.same_contents(&before));
    assert!(w.journal.sessions().unwrap().is_empty());
}

#[test]
fn apply_then_undo_all() {
    let mut w = World::new();
    let before = (w.reg.clone(), w.svc.clone());
    w.host = win11(true);
    let all: Vec<String> = recipe::load_embedded(RECIPES)
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();

    let (exit, out) = w.ro(apply(&refs));
    assert_eq!(exit, Exit::Ok, "{out}");
    assert!(
        out.contains("+ gaming.game-dvr-off: aplicat (2 modificări)"),
        "{out}"
    );
    // HAGS needs a reboot, the strongest restart among these recipes.
    assert!(
        out.contains("Unele modificări au efect abia după: repornirea PC-ului."),
        "{out}"
    );
    let (_, again) = w.ro(apply(&[DVR]));
    assert!(
        again.contains("= gaming.game-dvr-off: era deja aplicat"),
        "{again}"
    );

    let (exit, out) = w.ro(Cmd::Undo {
        ids: vec![],
        all: true,
    });
    assert_eq!(exit, Exit::Ok, "{out}");
    assert!(out.contains("+ services.fax-disabled: anulat"), "{out}");
    assert!(w.reg.same_contents(&before.0));
    assert!(w.svc.same_contents(&before.1));

    let (_, out) = w.ro(Cmd::Undo {
        ids: vec![],
        all: true,
    });
    assert!(
        out.contains("Aplicația nu are nicio modificare de anulat."),
        "{out}"
    );
}

#[test]
fn windows_10_and_arm_are_blocked() {
    let mut w = World::new();
    w.host.build = 19045;
    let (exit, out) = w.ro(Cmd::List);
    assert_eq!(exit, Exit::Blocked);
    assert!(
        out.contains("doar pe Windows 11") && out.contains("19045"),
        "{out}"
    );

    let mut w = World::new();
    w.host.arch = Arch::Arm64;
    let (exit, out) = w.run(Lang::En, apply(&[DVR]));
    assert_eq!(exit, Exit::Blocked);
    assert!(out.contains("ARM64"), "{out}");
    assert!(w.journal.sessions().unwrap().is_empty());
}

#[test]
fn running_game_blocks_changes_but_not_reading() {
    let mut w = World::new();
    w.running.push("cs2.exe".into());
    let before = w.reg.clone();

    let (exit, out) = w.ro(apply(&[DVR]));
    assert_eq!(exit, Exit::Blocked);
    assert!(
        out.contains("Închide jocul întâi: Counter-Strike 2 (cs2.exe)"),
        "{out}"
    );
    assert!(w.reg.same_contents(&before));

    assert_eq!(w.ro(Cmd::List).0, Exit::Ok);
    assert_eq!(w.ro(Cmd::Plan { ids: ids(&[DVR]) }).0, Exit::Ok);
}

#[test]
fn admin_is_required_only_when_needed() {
    let mut w = World::new();
    // HKCU only: fine without admin.
    assert_eq!(w.ro(apply(&[DVR])).0, Exit::Ok);

    // A service: needs admin.
    let (exit, out) = w.ro(apply(&["services.fax-disabled"]));
    assert_eq!(exit, Exit::Blocked);
    assert!(out.contains("Run as administrator"), "{out}");

    w.host = win11(true);
    assert_eq!(w.ro(apply(&["services.fax-disabled"])).0, Exit::Ok);

    // Undoing the service needs admin too.
    w.host = win11(false);
    let (exit, _) = w.ro(Cmd::Undo {
        ids: ids(&["services.fax-disabled"]),
        all: false,
    });
    assert_eq!(exit, Exit::Blocked);
}

#[test]
fn not_installed_service_needs_no_admin_and_is_reported() {
    let mut w = World::new();
    w.svc = MemoryServices::new();
    let (exit, out) = w.ro(apply(&["services.fax-disabled"]));
    assert_eq!(exit, Exit::Ok, "{out}");
    assert!(out.contains("serviciul „Fax” nu e instalat"), "{out}");
}

#[test]
fn unknown_recipe_is_an_error() {
    let mut w = World::new();
    let (exit, out) = w.ro(apply(&["gaming.turbo"]));
    assert_eq!(exit, Exit::Error);
    assert!(out.contains("Nu există rețeta „gaming.turbo”"), "{out}");
    // Undo of an id the journal does not know is just "nothing to undo".
    let (exit, out) = w.ro(Cmd::Undo {
        ids: ids(&["gaming.turbo"]),
        all: false,
    });
    assert_eq!(exit, Exit::Ok);
    assert!(out.contains("= gaming.turbo: nimic de anulat"), "{out}");
}

/// Leaves a `pending` session behind, as a crash right after a write would.
fn interrupted(w: &mut World) -> MemoryRegistry {
    let before = w.reg.clone();
    let key = r"System\GameConfigStore";
    let s = w.journal.begin_session(SessionKind::Apply).unwrap();
    let previous = serde_json::json!({
        "type": "registry", "hive": "HKCU", "key": key, "value": "GameDVR_Enabled",
        "state": {"state": "key_missing"}, "missing_keys": ["System", key],
    });
    let new = serde_json::json!({"type": "registry", "hive": "HKCU", "key": key,
        "value": "GameDVR_Enabled", "state": {"state": "present", "data": {"reg_type": 4, "bytes": [0,0,0,0]}}});
    w.journal
        .record_pending(
            s,
            &NewEntry {
                recipe_id: DVR,
                recipe_version: 1,
                action_index: 0,
                kind: EntryKind::Registry,
                target: "HKCU\\System\\GameConfigStore\\GameDVR_Enabled",
                previous_state: &previous,
                new_state: &new,
            },
        )
        .unwrap();
    w.reg
        .write(
            Hive::CurrentUser,
            key,
            "GameDVR_Enabled",
            &RegData::from_value(&RegValue::Dword(0)),
        )
        .unwrap();
    before
}

#[test]
fn interrupted_session_is_undone_at_startup() {
    let mut w = World::new();
    let before = interrupted(&mut w);
    assert!(!w.reg.same_contents(&before));

    let (exit, out) = w.ro(Cmd::Status);
    assert_eq!(exit, Exit::Ok, "{out}");
    assert!(
        out.contains("a fost întreruptă. Am anulat-o automat: 1 modificare restaurată."),
        "{out}"
    );
    assert!(out.contains("anulată"), "{out}");
    assert!(w.reg.same_contents(&before));
    assert_eq!(
        w.reg
            .read(
                Hive::CurrentUser,
                r"System\GameConfigStore",
                "GameDVR_Enabled"
            )
            .unwrap(),
        RegRead::KeyMissing
    );
}

#[test]
fn interrupted_session_waits_while_a_game_runs() {
    let mut w = World::new();
    let before = interrupted(&mut w);
    w.running.push("VALORANT-Win64-Shipping.exe".into());

    let (exit, out) = w.ro(Cmd::List);
    assert_eq!(exit, Exit::Ok);
    assert!(out.contains("după ce închizi jocul: VALORANT"), "{out}");
    assert!(out.contains("Atenție: stările afișate"), "{out}");

    let (exit, _) = w.ro(apply(&[DVR]));
    assert_eq!(exit, Exit::Blocked);
    assert_eq!(w.journal.pending_sessions().unwrap().len(), 1);

    w.running.pop();
    w.ro(Cmd::Status);
    assert!(w.reg.same_contents(&before));
    assert!(w.journal.pending_sessions().unwrap().is_empty());
}

#[test]
fn english_output() {
    let mut w = World::new();
    let (_, out) = w.run(Lang::En, apply(&[DVR]));
    assert!(
        out.contains("+ gaming.game-dvr-off: applied (2 changes)"),
        "{out}"
    );
    let (_, out) = w.run(Lang::En, Cmd::Status);
    assert!(out.contains("Sessions (newest first):"), "{out}");
    assert!(
        out.contains("Recipes currently applied by the app: gaming.game-dvr-off"),
        "{out}"
    );
}

#[test]
fn every_health_check_has_its_texts_in_both_languages() {
    use pco_core::health::CheckId;
    for lang in [Lang::Ro, Lang::En] {
        let map = translation(lang);
        for check in CheckId::ALL {
            for suffix in ["title", "fix"] {
                let key = format!("health.{}.{suffix}", check.key());
                assert!(map.contains_key(&key), "{lang:?}: missing {key}");
            }
            for outcome in check.reported_outcomes() {
                let key = format!("health.{}.{}", check.key(), outcome.key());
                assert!(map.contains_key(&key), "{lang:?}: missing {key}");
                assert!(map.contains_key(&format!("outcome.{}", outcome.key())));
            }
        }
    }
}

#[test]
fn health_report_in_romanian() {
    let mut w = World::new();
    let before = w.reg.clone();
    let (exit, out) = w.ro(Cmd::Health { json: false });
    assert_eq!(exit, Exit::Ok, "{out}");
    for expected in [
        "Hardware detectat",
        "Windows 11 Pro 24H2 (build 26100), laptop",
        "Procesor: 12th Gen Intel(R) Core(TM) i5-1245U (10 nuclee, 12 fire)",
        "Memorie: 16 GB, 2 module la 3200 MHz",
        "Placă video: Intel(R) Iris(R) Xe Graphics (integrată), driver 32.0.101.7088 din 2026-06-17",
        "Monitor: Built-in display — 1920×1200 la 60 Hz",
        "Disc: SAMSUNG MZAL4512HBLU-00BL2 — SSD NVMe, 476 GB",
        "Alimentare: plan „Balanced”, pe baterie",
        "Jocuri găsite: niciunul",
        "[Atenție] Laptop pe baterie",
        "Cum rezolvi: Pentru jocuri, ține laptopul la priză.",
        "[Info] Securitate bazată pe virtualizare",
        "VBS e activ (Memory Integrity: activ)",
        "OK (",
    ] {
        assert!(out.contains(expected), "missing `{expected}` in:\n{out}");
    }
    assert!(!out.contains("[Problemă]"), "{out}");
    // Warnings come before infos.
    assert!(out.find("[Atenție]").unwrap() < out.find("[Info]").unwrap());
    // Read-only.
    assert!(w.reg.same_contents(&before));
    assert!(w.journal.sessions().unwrap().is_empty());
}

#[test]
fn health_json_export() {
    let mut w = World::new();
    let (exit, out) = w.run(Lang::En, Cmd::Health { json: true });
    assert_eq!(exit, Exit::Ok);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["hardware"]["cpu"]["cores"], 10);
    let findings = v["findings"].as_array().unwrap();
    assert!(findings
        .iter()
        .any(|f| f["check"] == "on_battery" && f["outcome"] == "warning"));
}

#[test]
fn health_without_detection_is_an_error() {
    let texts = Texts::load(Lang::En).unwrap();
    let mut w = World::new();
    let mut env = Env {
        registry: &mut w.reg,
        services: &mut w.svc,
        power: &mut w.power,
        apps: &mut w.apps,
        journal: &mut w.journal,
        host: w.host.clone(),
        running: vec![],
        blocked: BlockedProcesses::builtin().unwrap(),
        recipes: vec![],
        hw: None,
        today: 0,
        bench: &mut w.bench,
    };
    let mut out = Vec::new();
    assert_eq!(
        run(&mut env, &texts, &Cmd::Health { json: false }, &mut out),
        Exit::Error
    );
}

#[test]
fn health_arguments() {
    assert_eq!(parse("pco health").unwrap(), Cmd::Health { json: false });
    assert_eq!(
        parse("pco health --json").unwrap(),
        Cmd::Health { json: true }
    );
}

#[test]
fn status_on_an_empty_journal() {
    let mut w = World::new();
    let (exit, out) = w.ro(Cmd::Status);
    assert_eq!(exit, Exit::Ok);
    assert!(out.contains("Nicio sesiune în jurnal."), "{out}");
    assert!(
        out.contains("Nicio rețetă aplicată acum de aplicație."),
        "{out}"
    );
}

// --- pco bench ------------------------------------------------------------------------------

fn bench_run(target: &str, after: bool) -> Cmd {
    Cmd::Bench(BenchCmd::Run(BenchRun {
        target: target.into(),
        after,
        runs: 3,
        duration: 60,
        warmup: 10,
        launch: true,
        wait_for_enter: true,
    }))
}

/// An administrator with CS2 running and PresentMon in place.
fn bench_world() -> World {
    let mut w = World::new();
    w.host = win11(true);
    w.bench.running = vec!["cs2.exe".into()];
    w
}

#[test]
fn bench_arguments() {
    match parse("pco bench run cs2 --before").unwrap() {
        Cmd::Bench(BenchCmd::Run(r)) => {
            assert_eq!(
                (r.target.as_str(), r.after, r.runs, r.duration, r.warmup),
                ("cs2", false, 3, 60, 10)
            );
            assert!(r.launch && r.wait_for_enter);
        }
        other => panic!("{other:?}"),
    }
    match parse("pco bench run msedge.exe --after --runs 1 --duration 20 --warmup 2 --no-launch --start-now").unwrap() {
        Cmd::Bench(BenchCmd::Run(r)) => {
            assert_eq!((r.runs, r.duration, r.warmup), (1, 20, 2));
            assert!(r.after && !r.launch && !r.wait_for_enter);
        }
        other => panic!("{other:?}"),
    }
    assert!(
        parse("pco bench run cs2").is_err(),
        "--before or --after is required"
    );
    assert!(parse("pco bench run cs2 --before --after").is_err());
    assert!(parse("pco bench run cs2 --before --runs 0").is_err());
    assert_eq!(
        parse("pco bench compare cs2").unwrap(),
        Cmd::Bench(BenchCmd::Compare {
            target: "cs2".into()
        })
    );
    assert_eq!(
        parse("pco bench history").unwrap(),
        Cmd::Bench(BenchCmd::History { target: None })
    );
    assert_eq!(
        parse("pco bench forget --all").unwrap(),
        Cmd::Bench(BenchCmd::Forget { target: None })
    );
    assert!(parse("pco bench forget").is_err());
}

#[test]
fn bench_before_then_after_shows_the_gain() {
    let mut w = bench_world();
    w.bench.fps = VecDeque::from([140.0, 142.0, 144.0]);
    let (exit, out) = w.ro(bench_run("cs2", false));
    assert_eq!(exit, Exit::Ok, "{out}");
    assert!(out.contains("Măsor cs2: 3 rulări de câte 60 s."), "{out}");
    assert!(out.contains("Rularea 1/3: apasă Enter"), "{out}");
    assert!(out.contains("Rularea 2: FPS mediu 142,0"), "{out}");
    assert!(
        out.contains("Salvat: 3 rulări „înainte” pentru cs2."),
        "{out}"
    );
    assert!(out.contains("pco bench run cs2 --after"), "{out}");
    assert_eq!(w.bench.slept, 30, "10 s before each run");
    assert_eq!(w.bench.captures, vec![("cs2.exe".to_owned(), 60); 3]);

    w.bench.fps = VecDeque::from([150.0, 151.0, 152.0]);
    let (exit, out) = w.ro(bench_run("cs2", true));
    assert_eq!(exit, Exit::Ok, "{out}");
    assert!(out.contains("cs2: înainte → după"), "{out}");
    assert!(out.contains("FPS mediu:  142,0 → 151,0 (+6,3%)"), "{out}");
    assert!(out.contains("Tweak-urile au ajutat."), "{out}");
    assert!(!out.contains("pco undo"), "{out}");
    assert!(
        w.journal.applied_recipe_ids().unwrap().is_empty(),
        "bench changes no settings"
    );
}

#[test]
fn bench_drop_after_tweaks_proposes_undoing_them() {
    let mut w = bench_world();
    w.bench.fps = VecDeque::from([140.0, 142.0, 144.0]);
    w.ro(bench_run("cs2", false));
    // Tweaks between the two measurements (CS2 is closed for applying, as the guard requires).
    w.running = vec!["explorer.exe".into()];
    assert_eq!(w.ro(apply(&[DVR])).0, Exit::Ok);
    w.bench.fps = VecDeque::from([130.0, 131.0, 132.0]);
    let (exit, out) = w.ro(bench_run("cs2", true));
    assert_eq!(exit, Exit::Ok, "{out}");
    assert!(out.contains("(-7,7%)"), "{out}");
    assert!(
        out.contains("FPS-ul a scăzut clar după tweak-uri. Anulez modificările? Rulează: pco undo gaming.game-dvr-off"),
        "{out}"
    );
}

#[test]
fn bench_drop_without_tweaks_in_between() {
    let mut w = bench_world();
    w.bench.fps = VecDeque::from([140.0, 142.0, 144.0, 130.0, 131.0, 132.0]);
    w.ro(bench_run("cs2", false));
    let (_, out) = w.run(Lang::En, bench_run("cs2", true));
    assert!(
        out.contains("no tweaks were applied between the two measurements"),
        "{out}"
    );
}

#[test]
fn bench_small_differences_are_noise() {
    let mut w = bench_world();
    w.bench.fps = VecDeque::from([140.0, 142.0, 144.0, 141.0, 142.5, 143.0]);
    w.ro(bench_run("cs2", false));
    let (_, out) = w.ro(bench_run("cs2", true));
    assert!(out.contains("Nicio diferență clară"), "{out}");
}

#[test]
fn bench_needs_admin_and_a_verified_presentmon() {
    let mut w = bench_world();
    w.host = win11(false);
    let (exit, out) = w.ro(bench_run("cs2", false));
    assert_eq!(exit, Exit::Blocked);
    assert!(out.contains("Run as administrator"), "{out}");
    assert!(w.bench.captures.is_empty());

    let mut w = bench_world();
    w.bench.presentmon_error = Some("SHA-256 mismatch".into());
    let (exit, out) = w.ro(bench_run("cs2", false));
    assert_eq!(exit, Exit::Error);
    assert!(
        out.contains("PresentMon nu e disponibil: SHA-256 mismatch"),
        "{out}"
    );
    assert!(w.bench.captures.is_empty());
}

#[test]
fn bench_starts_the_game_through_its_launcher() {
    let mut w = bench_world();
    w.bench.running = vec![];
    w.bench.starts_on_launch = Some("FortniteClient-Win64-Shipping.exe".into());
    w.bench.fps = VecDeque::from([200.0, 201.0, 202.0]);
    let (exit, out) = w.ro(bench_run("fortnite", false));
    assert_eq!(exit, Exit::Ok, "{out}");
    assert!(out.contains("Pornesc Fortnite"), "{out}");
    assert_eq!(w.bench.launched, [GameId::Fortnite]);
    assert_eq!(w.bench.captures[0].0, "FortniteClient-Win64-Shipping.exe");

    let mut w = bench_world();
    w.bench.running = vec![];
    w.bench.launch_error = Some("not installed".into());
    let (exit, out) = w.ro(bench_run("valorant", false));
    assert_eq!(exit, Exit::Error);
    assert!(
        out.contains("Nu am putut porni VALORANT: not installed"),
        "{out}"
    );

    // --no-launch and not running.
    let mut w = bench_world();
    w.bench.running = vec![];
    let mut cmd = bench_run("cs2", false);
    if let Cmd::Bench(BenchCmd::Run(r)) = &mut cmd {
        r.launch = false;
    }
    let (exit, out) = w.ro(cmd);
    assert_eq!(exit, Exit::Error);
    assert!(out.contains("cs2.exe nu rulează"), "{out}");
    assert!(w.bench.launched.is_empty());
}

#[test]
fn bench_any_process_and_the_less_precise_games() {
    let mut w = bench_world();
    w.bench.running = vec!["msedge.exe".into()];
    w.bench.fps = VecDeque::from([60.0, 60.0, 60.0]);
    let (exit, out) = w.ro(bench_run("MSEdge.exe", false));
    assert_eq!(exit, Exit::Ok, "{out}");
    assert_eq!(
        w.journal.benchmark_series(None).unwrap()[0].target,
        "msedge.exe"
    );

    let mut w = bench_world();
    w.bench.running = vec!["VALORANT-Win64-Shipping.exe".into()];
    w.bench.fps = VecDeque::from([300.0, 300.0, 300.0]);
    let (_, out) = w.ro(bench_run("valorant", false));
    assert!(
        out.contains("VALORANT nu are o hartă fixă de test"),
        "{out}"
    );

    let (exit, out) = w.ro(bench_run("minecraft", false));
    assert_eq!(exit, Exit::Error);
    assert!(out.contains("Nu cunosc „minecraft”"), "{out}");
}

#[test]
fn bench_stop_keeps_finished_runs_and_failures_are_reported() {
    let mut w = bench_world();
    w.bench.fps = VecDeque::from([140.0, 141.0]);
    w.bench.enters = VecDeque::from([true, false]);
    let (exit, out) = w.ro(bench_run("cs2", false));
    assert_eq!(exit, Exit::Ok, "{out}");
    assert!(
        out.contains("Oprit. Rulările terminate până acum sunt salvate."),
        "{out}"
    );
    assert_eq!(
        w.journal
            .latest_series("cs2", pco_core::bench::Label::Before)
            .unwrap()
            .unwrap()
            .runs
            .len(),
        1
    );

    let mut w = bench_world();
    w.bench.fps = VecDeque::from([140.0]); // the second capture fails
    let (exit, out) = w.ro(bench_run("cs2", false));
    assert_eq!(exit, Exit::Error);
    assert!(
        out.contains("Rularea 2 nu a reușit: simulated PresentMon failure"),
        "{out}"
    );
}

#[test]
fn bench_history_compare_and_forget() {
    let mut w = bench_world();
    let (_, out) = w.ro(Cmd::Bench(BenchCmd::History { target: None }));
    assert!(out.contains("Nicio măsurătoare salvată."), "{out}");
    let (_, out) = w.ro(Cmd::Bench(BenchCmd::Compare {
        target: "cs2".into(),
    }));
    assert!(
        out.contains("Nu am o măsurătoare „înainte” pentru cs2"),
        "{out}"
    );

    w.bench.fps = VecDeque::from([140.0, 142.0, 144.0, 150.0, 151.0, 152.0]);
    w.ro(bench_run("cs2", false));
    let (_, out) = w.ro(Cmd::Bench(BenchCmd::Compare {
        target: "cs2".into(),
    }));
    assert!(out.contains("lipsește măsurătoarea „după”"), "{out}");
    w.ro(bench_run("cs2", true));

    let (_, out) = w.ro(Cmd::Bench(BenchCmd::History {
        target: Some("cs2".into()),
    }));
    assert!(out.contains("cs2  după  3 rulări  FPS 151,0"), "{out}");
    assert!(out.contains("cs2  înainte  3 rulări  FPS 142,0"), "{out}");
    assert!(
        out.find("după").unwrap() < out.find("înainte").unwrap(),
        "newest first"
    );

    let (_, out) = w.ro(Cmd::Bench(BenchCmd::Compare {
        target: "cs2".into(),
    }));
    assert!(out.contains("+6,3%"), "{out}");

    let (_, out) = w.ro(Cmd::Bench(BenchCmd::Forget {
        target: Some("cs2".into()),
    }));
    assert!(out.contains("Am șters 6 rulări."), "{out}");
    assert!(w.journal.benchmark_series(None).unwrap().is_empty());
}
