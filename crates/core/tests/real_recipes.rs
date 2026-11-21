//! Every real recipe applied and undone on a real Windows — **only on GitHub Actions**.
//!
//! CLAUDE.md safety rule 1: real tweaks never run on the development PC. Three guards:
//! 1. `#[ignore]`: a normal `cargo test` skips these tests;
//! 2. they refuse to start unless `GITHUB_ACTIONS=true` (set by GitHub's runners);
//! 3. and `PCO_CI_REAL_TWEAKS=1` (set only by the `real-recipes` job in ci.yml).
//!
//! Exit criterion 1 of Stage 0: after apply → undo, every target is exactly as before.

#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use pco_core::backend::windows::{WindowsRegistry, WindowsServices};
use pco_core::backend::{
    AppxBackend, PowerBackend, PowerValues, RegData, RegRead, RegistryBackend, ServiceBackend,
    ServiceStart, ServiceState, WindowsAppx, WindowsPower,
};
use pco_core::engine::{
    is_net_adapter_key, ApplyOutcome, Engine, NotApplicable, RecipeStatus, UndoOutcome,
    NET_ADAPTER_CLASS,
};
use pco_core::guard::{self, SystemInfo};
use pco_core::journal::Journal;
use pco_core::recipe::{self, Action, Hive, Recipe, RegValue, RegistryOp};

/// Built-in plans whose settings are compared before/after (the ones present on the runner).
const PLANS: &[&str] = &[
    "381b4222-f694-41f0-9685-ff5bb260df2e",
    "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c",
    "a1841308-3541-4fab-bc81-f71556f20b4a",
    "e9a42b02-d5df-448d-aa00-03f14749eb61",
];

fn require_ci() {
    let on_github = std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true");
    let allowed = std::env::var("PCO_CI_REAL_TWEAKS").as_deref() == Ok("1");
    assert!(
        on_github && allowed,
        "refusing to change real Windows settings outside GitHub Actions (CLAUDE.md, rule 1)"
    );
}

/// Appends a line to the run's public summary page (GitHub's `GITHUB_STEP_SUMMARY`), and to
/// the log.
fn summary(line: &str) {
    println!("{line}");
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(f, "{line}").unwrap();
    }
}

fn journal(name: &str) -> Journal {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("real-recipes")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    Journal::open(&dir.join("journal.db")).unwrap()
}

/// The real backends, all able to write (this runs only on the CI runner).
struct Real {
    reg: WindowsRegistry,
    svc: WindowsServices,
    power: WindowsPower,
    apps: WindowsAppx,
}

impl Real {
    fn engine<'a>(&'a mut self, journal: &'a mut Journal, system: SystemInfo) -> Engine<'a> {
        Engine::new(&mut self.reg, &mut self.svc, journal, system)
            .with_power(&mut self.power)
            .with_apps(&mut self.apps)
    }

    fn adapter_keys(&self) -> Vec<String> {
        self.reg
            .subkeys(Hive::LocalMachine, NET_ADAPTER_CLASS)
            .unwrap()
            .into_iter()
            .map(|s| format!("{NET_ADAPTER_CLASS}\\{s}"))
            .filter(|k| is_net_adapter_key(k))
            .collect()
    }
}

/// Everything a recipe could touch: each key on each registry path (exists or not), each
/// value exactly, each service's start configuration, the active plan and the setting in
/// every built-in plan, the value on every network adapter, each app's state.
fn fingerprint(real: &Real, r: &Recipe) -> Vec<String> {
    let mut out = Vec::new();
    for action in &r.actions {
        match action {
            Action::Registry(a) => {
                let parts: Vec<&str> = a.key.split('\\').collect();
                for end in 1..=parts.len() {
                    let key = parts[..end].join("\\");
                    let exists = real.reg.read(a.hive, &key, "").unwrap() != RegRead::KeyMissing;
                    out.push(format!("{}\\{key} exists={exists}", a.hive));
                }
                let value = real.reg.read(a.hive, &a.key, &a.value).unwrap();
                out.push(format!("{}\\{}\\{} = {value:?}", a.hive, a.key, a.value));
            }
            Action::Service(s) => out.push(format!(
                "service {} = {:?}",
                s.name,
                real.svc.query(&s.name).unwrap()
            )),
            Action::PowerPlan(_) => {
                out.push(format!(
                    "active plan = {}",
                    real.power.active_scheme().unwrap()
                ));
            }
            Action::PowerSetting(p) => {
                out.push(format!(
                    "active plan = {}",
                    real.power.active_scheme().unwrap()
                ));
                for plan in PLANS {
                    if real.power.scheme_exists(plan).unwrap() {
                        let v = real
                            .power
                            .read_setting(plan, &p.subgroup, &p.setting)
                            .unwrap();
                        out.push(format!("{plan}/{} = {v:?}", p.setting));
                    }
                }
            }
            Action::NetAdapter(n) => {
                for key in real.adapter_keys() {
                    let v = real.reg.read(Hive::LocalMachine, &key, &n.value).unwrap();
                    out.push(format!("{key}\\{} = {v:?}", n.value));
                }
            }
            Action::Appx(a) => out.push(format!(
                "app {} = {:?}",
                a.package,
                real.apps.query(&a.package).unwrap()
            )),
            Action::Cleanup(_) => {}
        }
    }
    out
}

/// What the test changed before applying, to put back afterwards.
enum Seeded {
    Registry(Hive, String, String, RegRead),
    Plan(String),
    Setting(String, String, String, PowerValues),
}

fn put_back(reg: &mut WindowsRegistry, hive: Hive, key: &str, value: &str, original: &RegRead) {
    match original {
        RegRead::Present(d) => {
            reg.write(hive, key, value, d).unwrap();
        }
        RegRead::ValueMissing | RegRead::KeyMissing => {
            reg.delete_value(hive, key, value).unwrap();
        }
    }
}

/// Makes sure every action really has something to change on the runner, so the whole
/// apply → undo path runs even where the runner already has the target value:
/// - `only_if` actions get their expected value;
/// - registry and adapter values already at the target are removed for the test;
/// - a plan that is already active is swapped for Balanced (or another built-in plan);
/// - a power setting already at the target gets another valid value.
fn seed_starting_state(real: &mut Real, r: &Recipe) -> Vec<Seeded> {
    let mut seeded = Vec::new();
    for action in &r.actions {
        match action {
            Action::Registry(a) => {
                let RegistryOp::Set(target) = &a.op else {
                    continue;
                };
                let original = real.reg.read(a.hive, &a.key, &a.value).unwrap();
                let has = |v: &RegValue| matches!(&original, RegRead::Present(d) if d.matches(v));
                let changed = match &a.only_if {
                    Some(expected) if !has(expected) => {
                        real.reg
                            .write(a.hive, &a.key, &a.value, &RegData::from_value(expected))
                            .unwrap();
                        true
                    }
                    Some(_) => false,
                    None if has(target) => {
                        real.reg.delete_value(a.hive, &a.key, &a.value).unwrap();
                        true
                    }
                    None => false,
                };
                if changed {
                    seeded.push(Seeded::Registry(
                        a.hive,
                        a.key.clone(),
                        a.value.clone(),
                        original,
                    ));
                }
            }
            Action::NetAdapter(n) => {
                for key in real.adapter_keys() {
                    let original = real.reg.read(Hive::LocalMachine, &key, &n.value).unwrap();
                    if matches!(&original, RegRead::Present(d) if d.matches(&n.set)) {
                        real.reg
                            .delete_value(Hive::LocalMachine, &key, &n.value)
                            .unwrap();
                        seeded.push(Seeded::Registry(
                            Hive::LocalMachine,
                            key,
                            n.value.clone(),
                            original,
                        ));
                    }
                }
            }
            Action::PowerPlan(p) => {
                let active = real.power.active_scheme().unwrap();
                if active == p.scheme {
                    let other = PLANS
                        .iter()
                        .find(|s| **s != p.scheme && real.power.scheme_exists(s).unwrap())
                        .expect("a second built-in plan");
                    real.power.set_active_scheme(other).unwrap();
                    seeded.push(Seeded::Plan(active));
                }
            }
            Action::PowerSetting(p) => {
                let scheme = real.power.active_scheme().unwrap();
                let Some(values) = real
                    .power
                    .read_setting(&scheme, &p.subgroup, &p.setting)
                    .unwrap()
                else {
                    continue;
                };
                let other = |want: Option<u32>, have: u32| match want {
                    Some(w) if w == have => u32::from(w == 0),
                    _ => have,
                };
                let seed = PowerValues {
                    ac: other(p.ac, values.ac),
                    dc: other(p.dc, values.dc),
                };
                if seed != values {
                    real.power
                        .write_setting(&scheme, &p.subgroup, &p.setting, seed)
                        .unwrap();
                    seeded.push(Seeded::Setting(
                        scheme,
                        p.subgroup.clone(),
                        p.setting.clone(),
                        values,
                    ));
                }
            }
            Action::Service(_) | Action::Appx(_) | Action::Cleanup(_) => {}
        }
    }
    seeded
}

fn unseed(real: &mut Real, seeded: Vec<Seeded>) {
    for s in seeded.into_iter().rev() {
        match s {
            Seeded::Registry(hive, key, value, original) => {
                put_back(&mut real.reg, hive, &key, &value, &original);
            }
            Seeded::Plan(active) => real.power.set_active_scheme(&active).unwrap(),
            Seeded::Setting(scheme, subgroup, setting, values) => real
                .power
                .write_setting(&scheme, &subgroup, &setting, values)
                .unwrap(),
        }
    }
}

/// Why a recipe may legitimately not apply on the runner (Windows Server, a VM).
fn runner_reason(reason: &NotApplicable) -> Option<String> {
    Some(match reason {
        NotApplicable::ServiceMissing { name } => format!("service {name} is not installed"),
        NotApplicable::AppMissing { package } => format!("app {package} is not on the runner"),
        NotApplicable::AppNotRestorable { package } => {
            format!("Windows keeps no copy of {package} on the runner")
        }
        NotApplicable::NoNetworkAdapter { value } => {
            format!("no adapter on the runner offers {value}")
        }
        NotApplicable::PowerPlanMissing { scheme } => format!("plan {scheme} is not on the runner"),
        NotApplicable::PowerSettingMissing { setting } => {
            format!("power setting {setting} is not on the runner")
        }
        NotApplicable::Platform(p) => format!("not for this Windows: {p:?}"),
        _ => return None,
    })
}

#[test]
#[ignore = "changes real Windows settings; runs only in GitHub Actions"]
fn every_recipe_applies_and_undoes_exactly_on_real_windows() {
    require_ci();
    let host = guard::windows::host().unwrap();
    assert!(host.elevated, "the CI job must run as administrator");
    summary(&format!(
        "### Real recipes: apply → undo\n\nRunner: build {}, {}, edition {:?}\n\n| Recipe | Result |\n| --- | --- |",
        host.build, host.arch, host.edition
    ));
    let system = SystemInfo::from(&host);

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../recipes");
    let recipes = recipe::load_dir(&dir).unwrap_or_else(|e| panic!("{e}"));
    let mut real = Real {
        reg: WindowsRegistry::system(),
        svc: WindowsServices::system(),
        power: WindowsPower::system(),
        apps: WindowsAppx::system(),
    };
    let mut journal = journal("all");
    let mut applied = 0;

    for r in recipes.iter().filter(|r| !r.is_cleanup()) {
        let original = fingerprint(&real, r);
        let seeded = seed_starting_state(&mut real, r);
        let before = fingerprint(&real, r);

        let outcome = real
            .engine(&mut journal, system)
            .apply(&[r])
            .unwrap()
            .results
            .remove(0)
            .1;
        let result = match outcome {
            ApplyOutcome::Applied { changed } => {
                applied += 1;
                let mut engine = real.engine(&mut journal, system);
                assert_eq!(
                    engine.plan(r).unwrap().status,
                    RecipeStatus::Applied,
                    "{}",
                    r.id
                );
                let undo = engine.undo(&[&r.id]).unwrap().results.remove(0).1;
                assert!(
                    matches!(undo, UndoOutcome::Undone { .. }),
                    "{}: {undo:?}",
                    r.id
                );
                assert_eq!(
                    fingerprint(&real, r),
                    before,
                    "{}: not exactly as before after undo",
                    r.id
                );
                format!("applied ({changed} changes) and undone exactly")
            }
            // Anything else (including "already applied", impossible after seeding) fails.
            ApplyOutcome::NotApplicable(reason) => match runner_reason(&reason) {
                Some(why) => format!("not applicable: {why}"),
                None => panic!("{}: {reason:?}", r.id),
            },
            other => panic!("{}: {other:?}", r.id),
        };
        unseed(&mut real, seeded);
        assert_eq!(
            fingerprint(&real, r),
            original,
            "{}: runner not restored",
            r.id
        );
        summary(&format!("| `{}` | {result} |", r.id));
        // A GitHub annotation: visible on the run page even without signing in.
        println!("::notice title=Real recipe {}::{result}", r.id);
    }
    // Every registry-only recipe must have gone through the whole cycle.
    let registry_only = recipes
        .iter()
        .filter(|r| r.actions.iter().all(|a| matches!(a, Action::Registry(_))))
        .count();
    assert!(
        applied >= registry_only,
        "only {applied} of {registry_only} registry recipes were applied"
    );
    assert!(journal.pending_sessions().unwrap().is_empty());
    assert!(journal.applied_recipe_ids().unwrap().is_empty());
}

/// The service path, on a throwaway service created by the CI job (`PCO_CI_TEST_SERVICE`).
#[test]
#[ignore = "changes a real Windows service; runs only in GitHub Actions"]
fn service_changes_round_trip_on_a_throwaway_service() {
    require_ci();
    let name = std::env::var("PCO_CI_TEST_SERVICE").expect("PCO_CI_TEST_SERVICE is not set");
    let mut svc = WindowsServices::system();
    let original = svc
        .query(&name)
        .unwrap()
        .expect("the CI job creates this service");

    for (start, delayed) in [
        (ServiceStart::Disabled, false),
        (ServiceStart::Automatic, true),
        (ServiceStart::Automatic, false),
        (ServiceStart::Manual, false),
    ] {
        let state = ServiceState {
            start,
            delayed_auto_start: delayed,
        };
        svc.set(&name, state).unwrap();
        assert_eq!(svc.query(&name).unwrap(), Some(state));
    }
    svc.set(&name, original).unwrap();
    assert_eq!(svc.query(&name).unwrap(), Some(original));
    summary("\n### Service backend\n\nThrowaway service: disabled / automatic (delayed) / automatic / manual, then restored: OK");

    // The same through the engine, with a recipe for that service.
    let text = format!(
        "id: ci.test-service\nversion: 1\nname: {{ro: T, en: T}}\ndescription: {{ro: T, en: T}}\n\
         category: services\nrisk: safe\nprofiles: []\nimpact: {{ro: T, en: T}}\n\
         why: {{ro: T, en: T}}\nbreaks: {{ro: T, en: T}}\nrequires_restart: none\n\
         platform: {{min_build: 22000}}\nsources: [https://learn.microsoft.com/x]\n\
         verified: false\nactions:\n  - {{type: service, name: {name}, startup: disabled}}\n"
    );
    let r = recipe::parse_recipe(&text, Path::new("ci.yaml")).unwrap();
    let host = guard::windows::host().unwrap();
    let mut reg = WindowsRegistry::system();
    let mut journal = journal("service");
    let mut engine = Engine::new(&mut reg, &mut svc, &mut journal, SystemInfo::from(&host));
    let applied = engine.apply(&[&r]).unwrap().results.remove(0).1;
    assert_eq!(applied, ApplyOutcome::Applied { changed: 1 });
    engine.undo(&["ci.test-service"]).unwrap();
    assert_eq!(svc.query(&name).unwrap(), Some(original));
}

/// The app path, on a provisioned app the CI job names (`PCO_CI_TEST_APP`): removed for the
/// runner's user, then reinstalled from the copy Windows keeps.
#[test]
#[ignore = "removes and reinstalls a real app; runs only in GitHub Actions"]
fn app_removal_round_trips_on_the_runner() {
    require_ci();
    let Ok(package) = std::env::var("PCO_CI_TEST_APP") else {
        summary("\n### App backend\n\nNo provisioned app on the runner: skipped");
        return;
    };
    let text = format!(
        "id: ci.test-app\nversion: 1\nname: {{ro: T, en: T}}\ndescription: {{ro: T, en: T}}\n\
         category: debloat\nrisk: safe\nprofiles: []\nimpact: {{ro: T, en: T}}\n\
         why: {{ro: T, en: T}}\nbreaks: {{ro: T, en: T}}\nrequires_restart: none\n\
         platform: {{min_build: 22000}}\nsources: [https://learn.microsoft.com/x]\n\
         verified: false\nactions:\n  - {{type: appx, package: {package}}}\n"
    );
    let r = recipe::parse_recipe(&text, Path::new("ci.yaml")).unwrap();
    let host = guard::windows::host().unwrap();
    let mut real = Real {
        reg: WindowsRegistry::system(),
        svc: WindowsServices::system(),
        power: WindowsPower::system(),
        apps: WindowsAppx::system(),
    };
    let before = real.apps.query(&package).unwrap();
    let mut journal = journal("app");
    let mut engine = real.engine(&mut journal, SystemInfo::from(&host));
    let applied = engine.apply(&[&r]).unwrap().results.remove(0).1;
    assert_eq!(applied, ApplyOutcome::Applied { changed: 1 }, "{package}");
    let undone = engine.undo(&["ci.test-app"]).unwrap().results.remove(0).1;
    assert!(matches!(undone, UndoOutcome::Undone { .. }), "{undone:?}");
    assert_eq!(real.apps.query(&package).unwrap(), before);
    summary(&format!(
        "\n### App backend\n\n`{package}`: removed for the user, reinstalled from the copy Windows keeps: OK"
    ));
}

/// Power plan, power setting and adapter setting, through the engine on the runner's real
/// Windows, even before recipes of these kinds exist (the runner has Balanced and High
/// performance, and Hyper-V adapters).
#[test]
#[ignore = "changes real power and adapter settings; runs only in GitHub Actions"]
fn power_and_adapter_kinds_round_trip_on_the_runner() {
    require_ci();
    let host = guard::windows::host().unwrap();
    let system = SystemInfo::from(&host);
    let mut real = Real {
        reg: WindowsRegistry::system(),
        svc: WindowsServices::system(),
        power: WindowsPower::system(),
        apps: WindowsAppx::system(),
    };
    let active = real.power.active_scheme().unwrap();
    let other = PLANS
        .iter()
        .find(|s| **s != active && real.power.scheme_exists(s).unwrap())
        .expect("a second built-in plan");
    let actions = [
        format!("  - {{type: power-plan, scheme: {other}}}\n"),
        "  - {type: power-setting, subgroup: 2a737441-1930-4402-8d77-b2bebba308a3, setting: 48e6b7a6-50f5-4782-a5d4-53bb8f07e226, ac: 0, dc: 0}\n".into(),
        "  - {type: net-adapter, value: PnPCapabilities, kind: dword, set: 24, advertised: false}\n".into(),
    ];
    summary("\n### Power and adapter kinds\n");
    for (i, action) in actions.iter().enumerate() {
        let text = format!(
            "id: ci.kind-{i}\nversion: 1\nname: {{ro: T, en: T}}\ndescription: {{ro: T, en: T}}\n\
             category: power\nrisk: safe\nprofiles: []\nimpact: {{ro: T, en: T}}\n\
             why: {{ro: T, en: T}}\nbreaks: {{ro: T, en: T}}\nrequires_restart: none\n\
             platform: {{min_build: 22000}}\nsources: [https://learn.microsoft.com/x]\n\
             verified: false\nactions:\n{action}"
        );
        let r = recipe::parse_recipe(&text, Path::new("ci.yaml")).unwrap();
        let before = fingerprint(&real, &r);
        let seeded = seed_starting_state(&mut real, &r);
        let seeded_print = fingerprint(&real, &r);
        let mut journal = journal(&format!("kind-{i}"));
        let applied = real
            .engine(&mut journal, system)
            .apply(&[&r])
            .unwrap()
            .results
            .remove(0)
            .1;
        let ApplyOutcome::Applied { changed } = applied else {
            panic!("{}: {applied:?}", r.id);
        };
        assert_ne!(
            fingerprint(&real, &r),
            seeded_print,
            "{}: nothing changed",
            r.id
        );
        let undo = real
            .engine(&mut journal, system)
            .undo(&[&r.id])
            .unwrap()
            .results
            .remove(0)
            .1;
        assert!(matches!(undo, UndoOutcome::Undone { .. }), "{undo:?}");
        assert_eq!(fingerprint(&real, &r), seeded_print, "{}: undo", r.id);
        unseed(&mut real, seeded);
        assert_eq!(
            fingerprint(&real, &r),
            before,
            "{}: runner not restored",
            r.id
        );
        let line = format!(
            "{}: applied ({changed} changes) and undone exactly",
            action.trim()
        );
        summary(&format!("- {line}"));
        println!("::notice title=Real kind {i}::{line}");
    }
}

/// A startup program turned off and on again through the engine, on a throwaway Run entry
/// the test creates for the runner's user (and removes).
#[test]
#[ignore = "changes the runner's startup programs; runs only in GitHub Actions"]
fn startup_switch_round_trips_on_the_runner() {
    use pco_core::startup::{self, Location};
    require_ci();
    let host = guard::windows::host().unwrap();
    let mut real = Real {
        reg: WindowsRegistry::system(),
        svc: WindowsServices::system(),
        power: WindowsPower::system(),
        apps: WindowsAppx::system(),
    };
    let run = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let name = "PcoCiStartup";
    real.reg
        .write(
            Hive::CurrentUser,
            run,
            name,
            &RegData::from_value(&RegValue::String(r"C:\Windows\System32\notepad.exe".into())),
        )
        .unwrap();
    let (hive, approved) = Location::UserRun.approved();
    let before = real.reg.read(hive, &approved, name).unwrap();

    let entries = startup::list(&real.reg, &[]).unwrap();
    let entry = entries
        .iter()
        .find(|e| e.name == name)
        .expect("the throwaway entry is listed");
    assert!(entry.enabled);
    let mut journal = journal("startup");
    let recipe = startup::recipe_for(entry, false);
    let applied = real
        .engine(&mut journal, SystemInfo::from(&host))
        .apply(&[&recipe])
        .unwrap()
        .results
        .remove(0)
        .1;
    assert!(
        matches!(applied, ApplyOutcome::Applied { .. }),
        "{applied:?}"
    );
    let listed = startup::list(&real.reg, &[]).unwrap();
    assert!(!listed.iter().find(|e| e.name == name).unwrap().enabled);

    real.engine(&mut journal, SystemInfo::from(&host))
        .undo(&[&entry.id])
        .unwrap();
    assert_eq!(real.reg.read(hive, &approved, name).unwrap(), before);
    real.reg.delete_value(Hive::CurrentUser, run, name).unwrap();
    summary("\n### Startup programs\n\nThrowaway Run entry turned off (Task Manager's switch) and back on exactly: OK");
}
