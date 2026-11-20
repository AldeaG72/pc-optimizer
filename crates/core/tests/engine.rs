//! Engine: plan, apply, undo and recovery, all on in-memory backends.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use pco_core::backend::memory::{MemoryRegistry, MemoryServices};
use pco_core::backend::{
    BackendError, RegData, RegRead, RegistryBackend, ServiceBackend, ServiceStart, ServiceState,
};
use pco_core::engine::{
    ApplyOutcome, Engine, EngineError, NotApplicable, RecipeStatus, UndoOutcome,
};
use pco_core::journal::{Journal, SessionKind, Status};
use pco_core::recipe::{self, Hive, Recipe, RegValue};

const HKCU: Hive = Hive::CurrentUser;
const KEY: &str = r"Software\PcOptimizerTest\Engine";

fn dword(v: u32) -> RegData {
    RegData::from_value(&RegValue::Dword(v))
}

/// A recipe with the given actions (YAML list items, indented by two spaces).
fn recipe(id: &str, actions: &str) -> Recipe {
    let text = format!(
        "id: {id}\nversion: 1\nname: {{ro: N, en: N}}\ndescription: {{ro: D, en: D}}\n\
         category: gaming\nrisk: safe\nprofiles: []\nimpact: {{ro: I, en: I}}\n\
         why: {{ro: W, en: W}}\nbreaks: {{ro: B, en: B}}\nrequires_restart: none\n\
         platform: {{min_build: 22000}}\nsources: [https://learn.microsoft.com/x]\n\
         verified: false\nactions:\n{actions}"
    );
    recipe::parse_recipe(&text, Path::new("test.yaml")).unwrap_or_else(|e| panic!("{e}"))
}

fn reg_action(key: &str, value: &str, set: u32) -> String {
    format!(
        "  - {{type: registry, hive: HKCU, key: '{key}', value: {value}, kind: dword, set: {set}}}\n"
    )
}

fn two_values() -> Recipe {
    recipe(
        "test.two-values",
        &(reg_action(KEY, "A", 1) + &reg_action(KEY, "B", 2)),
    )
}

struct World {
    reg: MemoryRegistry,
    svc: MemoryServices,
    journal: Journal,
}

impl World {
    fn new(reg: MemoryRegistry) -> Self {
        Self {
            reg,
            svc: MemoryServices::new(),
            journal: Journal::open_in_memory().unwrap(),
        }
    }

    fn engine(&mut self) -> Engine<'_> {
        Engine::new(&mut self.reg, &mut self.svc, &mut self.journal)
    }
}

#[test]
fn plan_apply_undo_round_trip() {
    let start = MemoryRegistry::new().with_key(HKCU, r"Software\PcOptimizerTest");
    let mut w = World::new(start.clone());
    let r = two_values();

    let plan = w.engine().plan(&r).unwrap();
    assert_eq!(plan.status, RecipeStatus::NotApplied);
    assert_eq!(plan.actions.len(), 2);
    assert!(plan.actions.iter().all(|a| !a.in_place));
    assert!(
        w.journal.sessions().unwrap().is_empty(),
        "plan writes nothing"
    );
    assert!(w.reg.same_contents(&start), "plan changes nothing");

    let report = w.engine().apply(&[&r]).unwrap();
    assert_eq!(
        report.results,
        [(
            "test.two-values".into(),
            ApplyOutcome::Applied { changed: 2 }
        )]
    );
    assert_eq!(
        w.reg.read(HKCU, KEY, "A").unwrap(),
        RegRead::Present(dword(1))
    );
    assert_eq!(w.engine().plan(&r).unwrap().status, RecipeStatus::Applied);
    let session = w.journal.session(report.session.unwrap()).unwrap();
    assert_eq!(session.status, Status::Applied);

    let report = w.engine().undo(&["test.two-values"]).unwrap();
    assert_eq!(
        report.results,
        [(
            "test.two-values".into(),
            UndoOutcome::Undone { restored: 2 }
        )]
    );
    assert!(w.reg.same_contents(&start), "undo restores everything");
    assert_eq!(
        w.engine().plan(&r).unwrap().status,
        RecipeStatus::NotApplied
    );
    assert!(w.journal.applied_recipe_ids().unwrap().is_empty());
}

#[test]
fn missing_is_not_zero_and_created_keys_are_removed() {
    // Only `Software` exists; the recipe's key path is three levels deeper.
    let start = MemoryRegistry::new().with_key(HKCU, "Software");
    let mut w = World::new(start.clone());
    let deep = r"Software\PcOptimizerTest\A\B";
    let r = recipe("test.deep", &reg_action(deep, "Flag", 0));

    w.engine().apply(&[&r]).unwrap();
    assert_eq!(
        w.reg.read(HKCU, deep, "Flag").unwrap(),
        RegRead::Present(dword(0))
    );
    w.engine().undo(&["test.deep"]).unwrap();
    assert!(w.reg.same_contents(&start));
    assert!(!w.reg.key_exists(HKCU, r"Software\PcOptimizerTest"));
}

#[test]
fn undo_keeps_keys_that_something_else_filled() {
    let start = MemoryRegistry::new().with_key(HKCU, "Software");
    let mut w = World::new(start);
    let deep = r"Software\PcOptimizerTest\Shared";
    let r = recipe("test.shared", &reg_action(deep, "Flag", 1));
    w.engine().apply(&[&r]).unwrap();
    // Another program adds its own value to the key the recipe created.
    w.reg.write(HKCU, deep, "Theirs", &dword(9)).unwrap();

    w.engine().undo(&["test.shared"]).unwrap();
    assert_eq!(
        w.reg.read(HKCU, deep, "Flag").unwrap(),
        RegRead::ValueMissing
    );
    assert_eq!(
        w.reg.read(HKCU, deep, "Theirs").unwrap(),
        RegRead::Present(dword(9))
    );
}

#[test]
fn undo_restores_raw_values_exactly() {
    // Before: a REG_EXPAND_SZ the recipe would replace with a dword.
    let original = RegData {
        reg_type: 2,
        bytes: vec![0x25, 0, 0x41, 0, 0x25, 0, 0, 0],
    };
    let start = MemoryRegistry::new().with_value(HKCU, KEY, "A", original.clone());
    let mut w = World::new(start.clone());
    let r = recipe("test.raw", &reg_action(KEY, "A", 1));

    w.engine().apply(&[&r]).unwrap();
    w.engine().undo(&["test.raw"]).unwrap();
    assert_eq!(
        w.reg.read(HKCU, KEY, "A").unwrap(),
        RegRead::Present(original)
    );
    assert!(w.reg.same_contents(&start));
}

#[test]
fn already_in_place_changes_nothing() {
    let start = MemoryRegistry::new()
        .with_value(HKCU, KEY, "A", dword(1))
        .with_value(HKCU, KEY, "B", dword(2));
    let mut w = World::new(start.clone());
    let r = two_values();

    assert_eq!(
        w.engine().plan(&r).unwrap().status,
        RecipeStatus::AlreadyInPlace
    );
    let report = w.engine().apply(&[&r]).unwrap();
    assert_eq!(report.session, None, "no session when nothing changes");
    assert_eq!(report.results[0].1, ApplyOutcome::AlreadyApplied);
    assert_eq!(
        w.engine().undo(&["test.two-values"]).unwrap().results[0].1,
        UndoOutcome::NothingToUndo
    );
    assert!(w.reg.same_contents(&start));
}

#[test]
fn only_changed_actions_are_recorded() {
    // `A` is already right; only `B` changes, so only `B` is undone.
    let start = MemoryRegistry::new().with_value(HKCU, KEY, "A", dword(1));
    let mut w = World::new(start.clone());
    let report = w.engine().apply(&[&two_values()]).unwrap();
    assert_eq!(report.results[0].1, ApplyOutcome::Applied { changed: 1 });
    w.engine().undo(&["test.two-values"]).unwrap();
    assert!(w.reg.same_contents(&start));
}

#[test]
fn modified_outside_is_detected_and_undo_restores_the_original() {
    let start = MemoryRegistry::new().with_value(HKCU, KEY, "A", dword(5));
    let mut w = World::new(start.clone());
    let r = two_values();
    w.engine().apply(&[&r]).unwrap();

    // Something else changes the value.
    w.reg.write(HKCU, KEY, "A", &dword(7)).unwrap();
    assert_eq!(
        w.engine().plan(&r).unwrap().status,
        RecipeStatus::ModifiedOutside
    );

    // Re-applying records the outside value too; undo still ends at the original 5.
    assert_eq!(
        w.engine().apply(&[&r]).unwrap().results[0].1,
        ApplyOutcome::Applied { changed: 1 }
    );
    w.engine().undo(&["test.two-values"]).unwrap();
    assert!(w.reg.same_contents(&start));
}

#[test]
fn failing_action_rolls_back_its_recipe_and_others_continue() {
    let start = MemoryRegistry::new().with_key(HKCU, KEY);
    let mut w = World::new(start.clone());
    w.reg.fail_on(HKCU, KEY, "B");
    let failing = two_values(); // A succeeds, B fails
    let other = recipe("test.other", &reg_action(KEY, "C", 3));

    let report = w.engine().apply(&[&failing, &other]).unwrap();
    match &report.results[0].1 {
        ApplyOutcome::Failed { error, rolled_back } => {
            assert!(error.contains("simulated failure"), "{error}");
            assert!(rolled_back);
        }
        other => panic!("unexpected: {other:?}"),
    }
    assert_eq!(report.results[1].1, ApplyOutcome::Applied { changed: 1 });

    assert_eq!(w.reg.read(HKCU, KEY, "A").unwrap(), RegRead::ValueMissing);
    assert_eq!(
        w.reg.read(HKCU, KEY, "C").unwrap(),
        RegRead::Present(dword(3))
    );
    assert!(w
        .journal
        .applied_entries("test.two-values")
        .unwrap()
        .is_empty());
    assert_eq!(w.journal.applied_recipe_ids().unwrap(), ["test.other"]);
    assert_eq!(
        w.journal.session(report.session.unwrap()).unwrap().status,
        Status::Applied
    );

    let statuses: Vec<Status> = w
        .journal
        .entries(report.session.unwrap())
        .unwrap()
        .iter()
        .map(|e| e.status)
        .collect();
    assert_eq!(
        statuses,
        [Status::RolledBack, Status::Failed, Status::Applied]
    );
}

#[test]
fn session_fails_when_every_recipe_fails() {
    let mut w = World::new(MemoryRegistry::new());
    w.reg.fail_on(HKCU, KEY, "A");
    let report = w
        .engine()
        .apply(&[&recipe("test.a", &reg_action(KEY, "A", 1))])
        .unwrap();
    assert!(matches!(report.results[0].1, ApplyOutcome::Failed { .. }));
    assert_eq!(
        w.journal.session(report.session.unwrap()).unwrap().status,
        Status::Failed
    );
}

/// Writes something other than what it was asked to.
struct LyingRegistry(MemoryRegistry);

impl RegistryBackend for LyingRegistry {
    fn read(&self, h: Hive, k: &str, v: &str) -> Result<RegRead, BackendError> {
        self.0.read(h, k, v)
    }
    fn write(
        &mut self,
        h: Hive,
        k: &str,
        v: &str,
        _: &RegData,
    ) -> Result<Vec<String>, BackendError> {
        self.0.write(h, k, v, &dword(12345))
    }
    fn delete_value(&mut self, h: Hive, k: &str, v: &str) -> Result<bool, BackendError> {
        self.0.delete_value(h, k, v)
    }
    fn delete_key_if_empty(&mut self, h: Hive, k: &str) -> Result<bool, BackendError> {
        self.0.delete_key_if_empty(h, k)
    }
}

#[test]
fn read_back_mismatch_fails_and_rolls_back() {
    let start = MemoryRegistry::new().with_key(HKCU, KEY);
    let mut reg = LyingRegistry(start.clone());
    let mut svc = MemoryServices::new();
    let mut journal = Journal::open_in_memory().unwrap();
    let r = recipe("test.lie", &reg_action(KEY, "A", 1));

    let report = Engine::new(&mut reg, &mut svc, &mut journal)
        .apply(&[&r])
        .unwrap();
    match &report.results[0].1 {
        ApplyOutcome::Failed { error, rolled_back } => {
            assert!(error.contains("not in the expected state"), "{error}");
            assert!(
                rolled_back,
                "the value did not exist, so deleting restores it"
            );
        }
        other => panic!("unexpected: {other:?}"),
    }
    assert!(reg.0.same_contents(&start));
}

#[test]
fn services_round_trip_and_keep_the_delayed_flag() {
    let delayed = ServiceState {
        start: ServiceStart::Automatic,
        delayed_auto_start: true,
    };
    let mut w = World::new(MemoryRegistry::new());
    w.svc = MemoryServices::new().with_service("Fax", delayed);
    let start_svc = w.svc.clone();
    let r = recipe(
        "test.fax",
        "  - {type: service, name: Fax, startup: disabled}\n",
    );

    w.engine().apply(&[&r]).unwrap();
    assert_eq!(
        w.svc.query("Fax").unwrap(),
        Some(ServiceState {
            start: ServiceStart::Disabled,
            delayed_auto_start: true
        })
    );
    w.engine().undo(&["test.fax"]).unwrap();
    assert!(w.svc.same_contents(&start_svc));

    // "automatic" means plain automatic: a delayed service is not "already in place".
    let auto = recipe(
        "test.auto",
        "  - {type: service, name: Fax, startup: automatic}\n",
    );
    assert_eq!(
        w.engine().plan(&auto).unwrap().status,
        RecipeStatus::NotApplied
    );
}

#[test]
fn missing_service_makes_the_recipe_not_applicable() {
    let mut w = World::new(MemoryRegistry::new());
    let r = recipe(
        "test.nofax",
        &(reg_action(KEY, "A", 1) + "  - {type: service, name: Fax, startup: disabled}\n"),
    );
    let expected = NotApplicable::ServiceMissing { name: "Fax".into() };
    assert_eq!(
        w.engine().plan(&r).unwrap().status,
        RecipeStatus::NotApplicable(expected.clone())
    );
    let report = w.engine().apply(&[&r]).unwrap();
    assert_eq!(report.results[0].1, ApplyOutcome::NotApplicable(expected));
    assert_eq!(report.session, None);
    assert_eq!(w.reg.read(HKCU, KEY, "A").unwrap(), RegRead::KeyMissing);
}

/// Panics (like a crash) on the n-th write.
struct CrashingRegistry {
    inner: MemoryRegistry,
    writes_left: usize,
}

impl RegistryBackend for CrashingRegistry {
    fn read(&self, h: Hive, k: &str, v: &str) -> Result<RegRead, BackendError> {
        self.inner.read(h, k, v)
    }
    fn write(
        &mut self,
        h: Hive,
        k: &str,
        v: &str,
        d: &RegData,
    ) -> Result<Vec<String>, BackendError> {
        if self.writes_left == 0 {
            panic!("simulated crash");
        }
        self.writes_left -= 1;
        self.inner.write(h, k, v, d)
    }
    fn delete_value(&mut self, h: Hive, k: &str, v: &str) -> Result<bool, BackendError> {
        self.inner.delete_value(h, k, v)
    }
    fn delete_key_if_empty(&mut self, h: Hive, k: &str) -> Result<bool, BackendError> {
        self.inner.delete_key_if_empty(h, k)
    }
}

fn crash<F: FnOnce()>(f: F) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    assert!(result.is_err(), "expected a simulated crash");
}

#[test]
fn interrupted_apply_is_undone_by_recovery() {
    let start = MemoryRegistry::new().with_value(HKCU, KEY, "A", dword(0));
    let mut reg = CrashingRegistry {
        inner: start.clone(),
        writes_left: 2,
    };
    let mut svc = MemoryServices::new();
    let mut journal = Journal::open_in_memory().unwrap();
    let first = two_values(); // 2 writes
    let second = recipe("test.third", &reg_action(KEY, "C", 3)); // crashes here

    crash(|| {
        let _ = Engine::new(&mut reg, &mut svc, &mut journal).apply(&[&first, &second]);
    });
    assert!(
        !reg.inner.same_contents(&start),
        "the crash left changes behind"
    );

    // Nothing may run before recovery.
    reg.writes_left = usize::MAX;
    let mut engine = Engine::new(&mut reg, &mut svc, &mut journal);
    assert!(matches!(
        engine.apply(&[&first]),
        Err(EngineError::PendingRecovery { .. })
    ));
    assert!(matches!(
        engine.undo(&["test.two-values"]),
        Err(EngineError::PendingRecovery { .. })
    ));

    let recovered = engine.recover().unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].kind, SessionKind::Apply);
    assert_eq!(recovered[0].restored, 3, "A, B and the pending C");
    assert!(recovered[0].errors.is_empty());
    assert!(
        reg.inner.same_contents(&start),
        "exactly as before the session"
    );

    assert!(journal.pending_sessions().unwrap().is_empty());
    assert_eq!(
        journal.session(recovered[0].session).unwrap().status,
        Status::RolledBack
    );
    assert!(journal.applied_recipe_ids().unwrap().is_empty());
}

#[test]
fn interrupted_undo_is_reverted_and_can_be_retried() {
    let start = MemoryRegistry::new().with_value(HKCU, KEY, "A", dword(5));
    let mut reg = CrashingRegistry {
        inner: start.clone(),
        writes_left: usize::MAX,
    };
    let mut svc = MemoryServices::new();
    let mut journal = Journal::open_in_memory().unwrap();
    let r = two_values();
    Engine::new(&mut reg, &mut svc, &mut journal)
        .apply(&[&r])
        .unwrap();
    let applied = reg.inner.clone();

    // Undo restores B (a delete), then crashes writing A back to 5.
    reg.writes_left = 0;
    crash(|| {
        let _ = Engine::new(&mut reg, &mut svc, &mut journal).undo(&["test.two-values"]);
    });

    reg.writes_left = usize::MAX;
    let recovered = Engine::new(&mut reg, &mut svc, &mut journal)
        .recover()
        .unwrap();
    assert_eq!(recovered[0].kind, SessionKind::Undo);
    assert!(
        reg.inner.same_contents(&applied),
        "back to the applied state"
    );
    assert_eq!(journal.applied_recipe_ids().unwrap(), ["test.two-values"]);

    Engine::new(&mut reg, &mut svc, &mut journal)
        .undo(&["test.two-values"])
        .unwrap();
    assert!(reg.inner.same_contents(&start));
}

#[test]
fn undo_all_undoes_every_applied_recipe() {
    let start = MemoryRegistry::new().with_key(HKCU, KEY);
    let mut w = World::new(start.clone());
    let a = two_values();
    let b = recipe("test.b", &reg_action(KEY, "C", 3));
    w.engine().apply(&[&a]).unwrap();
    w.engine().apply(&[&b]).unwrap();

    let report = w.engine().undo_all().unwrap();
    assert_eq!(report.results.len(), 2);
    assert!(w.reg.same_contents(&start));
    assert!(w.engine().undo_all().unwrap().results.is_empty());
}

#[test]
fn every_repository_recipe_applies_and_undoes_exactly() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../recipes");
    let recipes = recipe::load_dir(&dir).unwrap_or_else(|e| panic!("{e}"));
    let manual = ServiceState {
        start: ServiceStart::Manual,
        delayed_auto_start: false,
    };

    for r in &recipes {
        // An empty registry is the hardest case: every key must be created and removed.
        let mut w = World::new(MemoryRegistry::new());
        w.svc = MemoryServices::new().with_service("Fax", manual);
        let (reg0, svc0) = (w.reg.clone(), w.svc.clone());

        let report = w.engine().apply(&[r]).unwrap();
        assert!(
            matches!(report.results[0].1, ApplyOutcome::Applied { .. }),
            "{}: {:?}",
            r.id,
            report.results
        );
        assert_eq!(
            w.engine().plan(r).unwrap().status,
            RecipeStatus::Applied,
            "{}",
            r.id
        );

        w.engine().undo(&[&r.id]).unwrap();
        assert!(
            w.reg.same_contents(&reg0),
            "{}: registry differs after undo",
            r.id
        );
        assert!(
            w.svc.same_contents(&svc0),
            "{}: services differ after undo",
            r.id
        );
    }
}
