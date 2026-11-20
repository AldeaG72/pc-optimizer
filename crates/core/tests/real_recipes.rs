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
    RegData, RegRead, RegistryBackend, ServiceBackend, ServiceStart, ServiceState,
};
use pco_core::engine::{ApplyOutcome, Engine, NotApplicable, RecipeStatus, UndoOutcome};
use pco_core::guard::{self, SystemInfo};
use pco_core::journal::Journal;
use pco_core::recipe::{self, Action, Hive, Recipe, RegValue, RegistryOp};

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

/// Everything a recipe could touch: each key on each registry path (exists or not), each
/// value exactly, each service's start configuration.
fn fingerprint(reg: &WindowsRegistry, svc: &WindowsServices, r: &Recipe) -> Vec<String> {
    let mut out = Vec::new();
    for action in &r.actions {
        match action {
            Action::Registry(a) => {
                let parts: Vec<&str> = a.key.split('\\').collect();
                for end in 1..=parts.len() {
                    let key = parts[..end].join("\\");
                    let exists = reg.read(a.hive, &key, "").unwrap() != RegRead::KeyMissing;
                    out.push(format!("{}\\{key} exists={exists}", a.hive));
                }
                let value = reg.read(a.hive, &a.key, &a.value).unwrap();
                out.push(format!("{}\\{}\\{} = {value:?}", a.hive, a.key, a.value));
            }
            Action::Service(s) => out.push(format!(
                "service {} = {:?}",
                s.name,
                svc.query(&s.name).unwrap()
            )),
        }
    }
    out
}

/// Makes sure every registry action really has something to change on the runner, so the
/// whole apply → undo path runs even where the runner already has the target value:
/// - `only_if` actions get their expected value;
/// - values already at the target are removed for the duration of the test.
///
/// Returns what to put back afterwards.
fn seed_starting_state(
    reg: &mut WindowsRegistry,
    r: &Recipe,
) -> Vec<(Hive, String, String, RegRead)> {
    let mut seeded = Vec::new();
    for action in &r.actions {
        let Action::Registry(a) = action else {
            continue;
        };
        let RegistryOp::Set(target) = &a.op else {
            continue;
        };
        let original = reg.read(a.hive, &a.key, &a.value).unwrap();
        let has = |v: &RegValue| matches!(&original, RegRead::Present(d) if d.matches(v));
        let changed = match &a.only_if {
            Some(expected) if !has(expected) => {
                reg.write(a.hive, &a.key, &a.value, &RegData::from_value(expected))
                    .unwrap();
                true
            }
            Some(_) => false,
            None if has(target) => {
                reg.delete_value(a.hive, &a.key, &a.value).unwrap();
                true
            }
            None => false,
        };
        if changed {
            seeded.push((a.hive, a.key.clone(), a.value.clone(), original));
        }
    }
    seeded
}

fn unseed(reg: &mut WindowsRegistry, seeded: Vec<(Hive, String, String, RegRead)>) {
    for (hive, key, value, original) in seeded {
        match original {
            RegRead::Present(d) => {
                reg.write(hive, &key, &value, &d).unwrap();
            }
            RegRead::ValueMissing | RegRead::KeyMissing => {
                reg.delete_value(hive, &key, &value).unwrap();
            }
        }
    }
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
    let mut reg = WindowsRegistry::system();
    let mut svc = WindowsServices::system();
    let mut journal = journal("all");
    let mut applied = 0;

    for r in &recipes {
        let original = fingerprint(&reg, &svc, r);
        let seeded = seed_starting_state(&mut reg, r);
        let before = fingerprint(&reg, &svc, r);

        let outcome = Engine::new(&mut reg, &mut svc, &mut journal, system)
            .apply(&[r])
            .unwrap()
            .results
            .remove(0)
            .1;
        let result = match outcome {
            ApplyOutcome::Applied { changed } => {
                applied += 1;
                let mut engine = Engine::new(&mut reg, &mut svc, &mut journal, system);
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
                    fingerprint(&reg, &svc, r),
                    before,
                    "{}: not exactly as before after undo",
                    r.id
                );
                format!("applied ({changed} changes) and undone exactly")
            }
            // Anything else (including "already applied", impossible after seeding) fails.
            ApplyOutcome::NotApplicable(NotApplicable::ServiceMissing { name }) => {
                format!("not applicable: service {name} is not installed")
            }
            other => panic!("{}: {other:?}", r.id),
        };
        unseed(&mut reg, seeded);
        assert_eq!(
            fingerprint(&reg, &svc, r),
            original,
            "{}: runner not restored",
            r.id
        );
        summary(&format!("| `{}` | {result} |", r.id));
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
