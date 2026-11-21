//! Engine with the Stage 3 kinds of change: power plans and settings, network adapter
//! settings, preinstalled apps, and the cleanup guard. In-memory backends only.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use pco_core::backend::memory::{MemoryAppx, MemoryPower, MemoryRegistry, MemoryServices};
use pco_core::backend::{
    AppxBackend, AppxState, PowerBackend, PowerValues, RegData, RegRead, RegistryBackend,
};
use pco_core::engine::{
    ApplyOutcome, Engine, NotApplicable, RecipeStatus, Snapshot, UndoOutcome, NET_ADAPTER_CLASS,
};
use pco_core::guard::SystemInfo;
use pco_core::journal::{Journal, Status};
use pco_core::recipe::{self, Edition, Hive, Recipe, RecipeError, RegValue};

const SYS: SystemInfo = SystemInfo {
    build: 26100,
    edition: Some(Edition::Pro),
    form_factor: None,
};
const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
const HIGH: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const USB: &str = "2a737441-1930-4402-8d77-b2bebba308a3";
const SUSPEND: &str = "48e6b7a6-50f5-4782-a5d4-53bb8f07e226";

fn yaml(id: &str, category: &str, profiles: &str, actions: &str) -> String {
    format!(
        "id: {id}\nversion: 1\nname: {{ro: N, en: N}}\ndescription: {{ro: D, en: D}}\n\
         category: {category}\nrisk: safe\nprofiles: {profiles}\nimpact: {{ro: I, en: I}}\n\
         why: {{ro: W, en: W}}\nbreaks: {{ro: B, en: B}}\nrequires_restart: none\n\
         platform: {{min_build: 22000}}\nsources: [https://learn.microsoft.com/x]\n\
         verified: false\nactions:\n{actions}"
    )
}

fn recipe(id: &str, actions: &str) -> Recipe {
    recipe::parse_recipe(&yaml(id, "power", "[]", actions), Path::new("t.yaml"))
        .unwrap_or_else(|e| panic!("{e}"))
}

struct World {
    reg: MemoryRegistry,
    svc: MemoryServices,
    power: MemoryPower,
    apps: MemoryAppx,
    journal: Journal,
}

impl World {
    fn new() -> Self {
        Self {
            reg: MemoryRegistry::new(),
            svc: MemoryServices::new(),
            power: MemoryPower::new(BALANCED)
                .with_scheme(HIGH)
                .with_setting(BALANCED, USB, SUSPEND, PowerValues { ac: 1, dc: 1 })
                .with_setting(HIGH, USB, SUSPEND, PowerValues { ac: 1, dc: 1 }),
            apps: MemoryAppx::new(),
            journal: Journal::open_in_memory().unwrap(),
        }
    }

    fn engine(&mut self) -> Engine<'_> {
        Engine::new(&mut self.reg, &mut self.svc, &mut self.journal, SYS)
            .with_power(&mut self.power)
            .with_apps(&mut self.apps)
    }
}

fn applied(outcome: &ApplyOutcome) -> usize {
    match outcome {
        ApplyOutcome::Applied { changed } => *changed,
        other => panic!("{other:?}"),
    }
}

#[test]
fn power_plan_switches_and_undo_restores_the_previous_plan() {
    let mut w = World::new();
    let start = w.power.clone();
    let r = recipe(
        "power.plan",
        &format!("  - {{type: power-plan, scheme: '{{{HIGH}}}'}}\n"),
    );

    let report = w.engine().apply(&[&r]).unwrap();
    assert_eq!(applied(&report.results[0].1), 1);
    assert_eq!(w.power.active_scheme().unwrap(), HIGH);
    assert_eq!(w.engine().plan(&r).unwrap().status, RecipeStatus::Applied);

    w.engine().undo(&["power.plan"]).unwrap();
    assert!(w.power.same_contents(&start));
}

#[test]
fn missing_plan_is_not_applicable() {
    let mut w = World::new();
    let ultimate = "e9a42b02-d5df-448d-aa00-03f14749eb61";
    let r = recipe(
        "power.ultimate",
        &format!("  - {{type: power-plan, scheme: {ultimate}}}\n"),
    );
    assert_eq!(
        w.engine().plan(&r).unwrap().status,
        RecipeStatus::NotApplicable(NotApplicable::PowerPlanMissing {
            scheme: ultimate.into()
        })
    );
}

#[test]
fn power_setting_changes_one_side_in_the_active_plan_and_undoes_exactly() {
    let mut w = World::new();
    let start = w.power.clone();
    let r = recipe(
        "power.usb",
        &format!("  - {{type: power-setting, subgroup: {USB}, setting: {SUSPEND}, ac: 0}}\n"),
    );
    w.engine().apply(&[&r]).unwrap();
    assert_eq!(
        w.power.read_setting(BALANCED, USB, SUSPEND).unwrap(),
        Some(PowerValues { ac: 0, dc: 1 }),
        "only the plugged-in value changes"
    );
    // The other plan is untouched.
    assert_eq!(
        w.power.read_setting(HIGH, USB, SUSPEND).unwrap(),
        Some(PowerValues { ac: 1, dc: 1 })
    );
    w.engine().undo(&["power.usb"]).unwrap();
    assert!(w.power.same_contents(&start));
}

#[test]
fn plan_and_setting_in_one_recipe_undo_in_any_order() {
    let mut w = World::new();
    let start = w.power.clone();
    // Switches to High performance, then changes the setting in it (the active plan).
    let r = recipe(
        "power.both",
        &format!(
            "  - {{type: power-plan, scheme: {HIGH}}}\n  - {{type: power-setting, subgroup: {USB}, setting: {SUSPEND}, ac: 0, dc: 0}}\n"
        ),
    );
    assert_eq!(applied(&w.engine().apply(&[&r]).unwrap().results[0].1), 2);
    assert_eq!(
        w.power.read_setting(HIGH, USB, SUSPEND).unwrap(),
        Some(PowerValues { ac: 0, dc: 0 })
    );
    assert_eq!(
        w.power.read_setting(BALANCED, USB, SUSPEND).unwrap(),
        Some(PowerValues { ac: 1, dc: 1 })
    );
    w.engine().undo(&["power.both"]).unwrap();
    assert!(w.power.same_contents(&start));
}

#[test]
fn failed_power_write_rolls_back_the_plan_switch() {
    let mut w = World::new();
    let start = w.power.clone();
    let r = recipe(
        "power.both",
        &format!(
            "  - {{type: power-plan, scheme: {HIGH}}}\n  - {{type: power-setting, subgroup: {USB}, setting: 0cc5b647-c1df-4637-891a-dec35c318583, ac: 100}}\n"
        ),
    );
    // The second setting does not exist: not applicable, nothing changes.
    let report = w.engine().apply(&[&r]).unwrap();
    assert!(matches!(
        report.results[0].1,
        ApplyOutcome::NotApplicable(NotApplicable::PowerSettingMissing { .. })
    ));
    assert!(w.power.same_contents(&start));

    // A write that fails half-way is rolled back.
    let r = recipe(
        "power.usb",
        &format!("  - {{type: power-setting, subgroup: {USB}, setting: {SUSPEND}, ac: 0}}\n"),
    );
    w.power.fail_writes();
    let report = w.engine().apply(&[&r]).unwrap();
    assert!(matches!(
        report.results[0].1,
        ApplyOutcome::Failed {
            rolled_back: true,
            ..
        }
    ));
}

fn adapter(reg: MemoryRegistry, n: &str, characteristics: u32, offers: &[&str]) -> MemoryRegistry {
    let key = format!("{NET_ADAPTER_CLASS}\\{n}");
    let mut reg = reg.with_value(
        Hive::LocalMachine,
        &key,
        "Characteristics",
        RegData::from_value(&RegValue::Dword(characteristics)),
    );
    for value in offers {
        reg = reg.with_key(Hive::LocalMachine, &format!("{key}\\Ndi\\Params\\{value}"));
    }
    reg
}

#[test]
fn adapter_setting_touches_every_physical_adapter_that_offers_it() {
    let mut w = World::new();
    // 0000: physical, offers *EEE, value "1". 0001: physical, offers it, no value yet.
    // 0002: virtual (no NCF_PHYSICAL). 0003: physical, does not offer it. "Properties": skipped.
    let mut reg = adapter(MemoryRegistry::new(), "0000", 0x84, &["*EEE"]);
    reg = reg.with_value(
        Hive::LocalMachine,
        &format!("{NET_ADAPTER_CLASS}\\0000"),
        "*EEE",
        RegData::from_value(&RegValue::String("1".into())),
    );
    reg = adapter(reg, "0001", 0x84, &["*EEE"]);
    reg = adapter(reg, "0002", 0x1, &["*EEE"]);
    reg = adapter(reg, "0003", 0x84, &[]);
    reg = reg.with_key(
        Hive::LocalMachine,
        &format!("{NET_ADAPTER_CLASS}\\Properties"),
    );
    w.reg = reg;
    let start = w.reg.clone();

    let r = recipe(
        "network.eee-off",
        "  - {type: net-adapter, value: '*EEE', kind: string, set: '0'}\n",
    );
    let plan = w.engine().plan(&r).unwrap();
    assert_eq!(plan.actions.len(), 2, "{plan:?}");

    assert_eq!(applied(&w.engine().apply(&[&r]).unwrap().results[0].1), 2);
    let zero = RegData::from_value(&RegValue::String("0".into()));
    for n in ["0000", "0001"] {
        assert_eq!(
            w.reg
                .read(
                    Hive::LocalMachine,
                    &format!("{NET_ADAPTER_CLASS}\\{n}"),
                    "*EEE"
                )
                .unwrap(),
            RegRead::Present(zero.clone())
        );
    }
    for n in ["0002", "0003"] {
        assert_eq!(
            w.reg
                .read(
                    Hive::LocalMachine,
                    &format!("{NET_ADAPTER_CLASS}\\{n}"),
                    "*EEE"
                )
                .unwrap(),
            RegRead::ValueMissing
        );
    }
    assert_eq!(w.engine().plan(&r).unwrap().status, RecipeStatus::Applied);
    w.engine().undo(&["network.eee-off"]).unwrap();
    assert!(w.reg.same_contents(&start));
}

#[test]
fn adapter_setting_without_adapters_is_not_applicable() {
    let mut w = World::new();
    w.reg = adapter(MemoryRegistry::new(), "0000", 0x84, &[]);
    let r = recipe(
        "network.eee-off",
        "  - {type: net-adapter, value: '*EEE', kind: string, set: '0'}\n",
    );
    assert_eq!(
        w.engine().plan(&r).unwrap().status,
        RecipeStatus::NotApplicable(NotApplicable::NoNetworkAdapter {
            value: "*EEE".into()
        })
    );
    // Not advertised: every physical adapter.
    let r = recipe(
        "network.pnp",
        "  - {type: net-adapter, value: PnPCapabilities, kind: dword, set: 24, advertised: false}\n",
    );
    assert_eq!(w.engine().plan(&r).unwrap().actions.len(), 1);
}

/// Panics (like a crash) on the second write.
struct CrashOnSecondWrite {
    inner: MemoryRegistry,
    writes: usize,
}

impl RegistryBackend for CrashOnSecondWrite {
    fn read(&self, h: Hive, k: &str, v: &str) -> Result<RegRead, pco_core::backend::BackendError> {
        self.inner.read(h, k, v)
    }
    fn write(
        &mut self,
        h: Hive,
        k: &str,
        v: &str,
        d: &RegData,
    ) -> Result<Vec<String>, pco_core::backend::BackendError> {
        self.writes += 1;
        assert!(self.writes < 2, "simulated crash");
        self.inner.write(h, k, v, d)
    }
    fn delete_value(
        &mut self,
        h: Hive,
        k: &str,
        v: &str,
    ) -> Result<bool, pco_core::backend::BackendError> {
        self.inner.delete_value(h, k, v)
    }
    fn delete_key_if_empty(
        &mut self,
        h: Hive,
        k: &str,
    ) -> Result<bool, pco_core::backend::BackendError> {
        self.inner.delete_key_if_empty(h, k)
    }
    fn subkeys(&self, h: Hive, k: &str) -> Result<Vec<String>, pco_core::backend::BackendError> {
        self.inner.subkeys(h, k)
    }
    fn values(&self, h: Hive, k: &str) -> Result<Vec<String>, pco_core::backend::BackendError> {
        self.inner.values(h, k)
    }
}

#[test]
fn interrupted_adapter_change_is_recovered_per_adapter() {
    let start = adapter(
        adapter(MemoryRegistry::new(), "0000", 0x84, &["*EEE"]),
        "0001",
        0x84,
        &["*EEE"],
    );
    let r = recipe(
        "network.eee-off",
        "  - {type: net-adapter, value: '*EEE', kind: string, set: '0'}\n",
    );
    let mut journal = Journal::open_in_memory().unwrap();
    let mut crashing = CrashOnSecondWrite {
        inner: start.clone(),
        writes: 0,
    };
    let mut svc = MemoryServices::new();
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = Engine::new(&mut crashing, &mut svc, &mut journal, SYS).apply(&[&r]);
    }));
    assert!(crashed.is_err());
    // The first adapter was changed, the second was recorded as pending.
    assert_eq!(journal.pending_sessions().unwrap().len(), 1);

    let mut reg = crashing.inner;
    let recovered = Engine::new(&mut reg, &mut svc, &mut journal, SYS)
        .recover()
        .unwrap();
    assert!(recovered[0].errors.is_empty(), "{recovered:?}");
    assert!(reg.same_contents(&start));
    assert_eq!(
        journal.session(recovered[0].session).unwrap().status,
        Status::RolledBack
    );
}

#[test]
fn app_is_removed_and_reinstalled_by_undo() {
    let mut w = World::new();
    w.apps = MemoryAppx::new().with_app("Clipchamp.Clipchamp", AppxState::Installed);
    let start = w.apps.clone();
    let r = recipe(
        "debloat.clipchamp",
        "  - {type: appx, package: Clipchamp.Clipchamp}\n",
    );

    assert_eq!(applied(&w.engine().apply(&[&r]).unwrap().results[0].1), 1);
    assert_eq!(
        w.apps.query("Clipchamp.Clipchamp").unwrap(),
        AppxState::Removed
    );
    assert_eq!(w.engine().plan(&r).unwrap().status, RecipeStatus::Applied);
    let undo = w.engine().undo(&["debloat.clipchamp"]).unwrap();
    assert!(matches!(
        undo.results[0].1,
        UndoOutcome::Undone { restored: 1 }
    ));
    assert!(w.apps.same_contents(&start));
}

#[test]
fn apps_windows_cannot_reinstall_are_never_removed() {
    let mut w = World::new();
    w.apps = MemoryAppx::new().with_app("Some.App", AppxState::NotRestorable);
    let r = recipe("debloat.some", "  - {type: appx, package: Some.App}\n");
    assert!(matches!(
        w.engine().apply(&[&r]).unwrap().results[0].1,
        ApplyOutcome::NotApplicable(NotApplicable::AppNotRestorable { .. })
    ));
    assert_eq!(w.apps.query("Some.App").unwrap(), AppxState::NotRestorable);

    let r = recipe("debloat.none", "  - {type: appx, package: Not.There}\n");
    assert!(matches!(
        w.engine().plan(&r).unwrap().status,
        RecipeStatus::NotApplicable(NotApplicable::AppMissing { .. })
    ));
}

#[test]
fn snapshots_of_new_kinds_survive_the_journal() {
    let mut w = World::new();
    w.apps = MemoryAppx::new().with_app("Clipchamp.Clipchamp", AppxState::Installed);
    let r = recipe(
        "mixed.all",
        &format!(
            "  - {{type: power-plan, scheme: {HIGH}}}\n  - {{type: appx, package: Clipchamp.Clipchamp}}\n"
        ),
    );
    w.engine().apply(&[&r]).unwrap();
    let entries = w.journal.applied_entries("mixed.all").unwrap();
    assert_eq!(entries.len(), 2);
    let first: Snapshot = entries[0].previous().unwrap();
    assert_eq!(
        first,
        Snapshot::PowerPlan {
            active: BALANCED.into()
        }
    );
    let second: Snapshot = entries[1].previous().unwrap();
    assert_eq!(
        second,
        Snapshot::Appx {
            package: "Clipchamp.Clipchamp".into(),
            installed: true
        }
    );
    assert!(w.engine().undo_needs_admin(&["mixed.all"]).unwrap());
}

#[test]
fn cleanup_recipes_stay_apart_and_never_apply() {
    let ok = yaml(
        "cleanup.temp",
        "cleanup",
        "[]",
        "  - {type: cleanup, target: user-temp}\n  - {type: cleanup, target: windows-temp}\n",
    );
    let r = recipe::parse_recipe(&ok, Path::new("t.yaml")).unwrap();
    assert!(r.is_cleanup());
    assert_eq!(r.cleanup_targets().len(), 2);
    let mut w = World::new();
    assert!(matches!(
        w.engine().apply(&[&r]).unwrap().results[0].1,
        ApplyOutcome::NotApplicable(NotApplicable::Cleanup)
    ));

    let mixed = yaml(
        "cleanup.mixed",
        "cleanup",
        "[]",
        &format!(
            "  - {{type: cleanup, target: user-temp}}\n  - {{type: power-plan, scheme: {HIGH}}}\n"
        ),
    );
    assert!(matches!(
        recipe::parse_recipe(&mixed, Path::new("t.yaml")),
        Err(RecipeError::Invalid { .. })
    ));
    let recommended = yaml(
        "cleanup.rec",
        "cleanup",
        "[recommended]",
        "  - {type: cleanup, target: recycle-bin}\n",
    );
    assert!(recipe::parse_recipe(&recommended, Path::new("t.yaml")).is_err());
}

#[test]
fn bad_new_actions_are_refused_at_load_time() {
    for (actions, why) in [
        ("  - {type: power-plan, scheme: high}\n", "not a GUID"),
        (
            &*format!("  - {{type: power-setting, subgroup: {USB}, setting: {SUSPEND}}}\n"),
            "neither ac nor dc",
        ),
        (
            "  - {type: net-adapter, value: 'a\\b', kind: dword, set: 1}\n",
            "backslash",
        ),
        (
            "  - {type: net-adapter, value: '*EEE', kind: dword, set: '0'}\n",
            "kind mismatch",
        ),
        ("  - {type: appx, package: \"x'; rm\"}\n", "unsafe package"),
        (
            "  - {type: appx, package: Microsoft.SecHealthUI}\n",
            "protected",
        ),
        (
            "  - {type: appx, package: Microsoft.MicrosoftEdge.Stable}\n",
            "Edge",
        ),
        ("  - {type: cleanup, target: system32}\n", "unknown target"),
    ] {
        let text = yaml("t.bad", "power", "[]", actions);
        assert!(
            recipe::parse_recipe(&text, Path::new("t.yaml")).is_err(),
            "{why}"
        );
    }
}
