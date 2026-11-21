//! Recipe loading: the real recipes in `recipes/`, invalid recipes, and agreement between
//! the Rust validation and `recipes/schema/recipe.schema.json`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use pco_core::recipe::{self, Action, Hive, RecipeError, RegValue, RegistryOp};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn recipes_dir() -> PathBuf {
    repo_root().join("recipes")
}

/// A minimal valid recipe; every invalid case below is a small change to it.
const VALID: &str = r#"id: test.example-recipe
version: 1
name:
  ro: Exemplu
  en: Example
description:
  ro: Descriere
  en: Description
category: gaming
risk: safe
profiles: [recommended]
impact:
  ro: Mic
  en: Low
why:
  ro: Motiv
  en: Reason
breaks:
  ro: Nimic
  en: Nothing
requires_restart: none
platform:
  min_build: 22000
sources:
  - https://learn.microsoft.com/example
verified: false
actions:
  - type: registry
    hive: HKCU
    key: Software\PcOptimizerTest\Example
    value: Enabled
    kind: dword
    set: 0
  - type: service
    name: ExampleSvc
    startup: manual
"#;

struct Case {
    name: &'static str,
    text: String,
    /// Must appear in the error message (usually the field path).
    expect: &'static str,
    /// Whether the JSON schema can express this rule too.
    schema_rejects: bool,
}

fn replace(
    name: &'static str,
    edits: &[(&str, &str)],
    expect: &'static str,
    schema_rejects: bool,
) -> Case {
    let mut text = VALID.to_owned();
    for (from, to) in edits {
        assert!(text.contains(from), "case `{name}`: `{from}` not found");
        text = text.replacen(from, to, 1);
    }
    Case {
        name,
        text,
        expect,
        schema_rejects,
    }
}

fn cases() -> Vec<Case> {
    let before_actions = &VALID[..VALID.find("actions:").unwrap()];
    vec![
        replace(
            "unknown top-level field",
            &[("verified: false", "verified: false\nmagic: true")],
            "magic",
            true,
        ),
        replace(
            "missing why",
            &[("why:\n  ro: Motiv\n  en: Reason\n", "")],
            "field `why`",
            true,
        ),
        replace(
            "unknown risk",
            &[("risk: safe", "risk: yolo")],
            "risk",
            true,
        ),
        replace(
            "unknown category",
            &[("category: gaming", "category: overclock")],
            "category",
            true,
        ),
        replace(
            "Windows 10 build",
            &[("min_build: 22000", "min_build: 19041")],
            "platform.min_build",
            true,
        ),
        replace(
            "max below min",
            &[(
                "  min_build: 22000",
                "  min_build: 22000\n  max_build: 21000",
            )],
            "platform.max_build",
            false,
        ),
        replace(
            "bad id",
            &[("id: test.example-recipe", "id: Test Example")],
            "id",
            true,
        ),
        replace(
            "version 0",
            &[("version: 1", "version: 0")],
            "version",
            true,
        ),
        replace(
            "empty translation",
            &[("  en: Reason", "  en: \"\"")],
            "why.en",
            true,
        ),
        replace(
            "duplicate profile",
            &[(
                "profiles: [recommended]",
                "profiles: [recommended, recommended]",
            )],
            "profiles",
            true,
        ),
        replace(
            "no sources",
            &[(
                "sources:\n  - https://learn.microsoft.com/example\n",
                "sources: []\n",
            )],
            "sources",
            true,
        ),
        replace(
            "source is not a URL",
            &[("  - https://learn.microsoft.com/example", "  - trust me")],
            "sources[0]",
            false,
        ),
        Case {
            name: "no actions",
            text: format!("{before_actions}actions: []\n"),
            expect: "actions",
            schema_rejects: true,
        },
        replace(
            "unknown action type",
            &[("type: service", "type: script")],
            "script",
            true,
        ),
        replace(
            "unknown action field",
            &[(
                "    startup: manual",
                "    startup: manual\n    run: calc.exe",
            )],
            "run",
            true,
        ),
        replace(
            "unknown startup",
            &[("startup: manual", "startup: boot")],
            "boot",
            true,
        ),
        replace(
            "unknown hive",
            &[("hive: HKCU", "hive: HKCR")],
            "HKCR",
            true,
        ),
        replace(
            "set and delete",
            &[("    set: 0", "    set: 0\n    delete: true")],
            "actions[0].delete",
            true,
        ),
        replace(
            "kind without set",
            &[("    set: 0\n", "")],
            "actions[0].set",
            true,
        ),
        replace(
            "dword written as text",
            &[("    set: 0", "    set: \"0\"")],
            "actions[0].set",
            false,
        ),
        replace(
            "dword too large",
            &[("    set: 0", "    set: 4294967296")],
            "actions[0].set",
            false,
        ),
        replace(
            "string kind with number",
            &[("kind: dword", "kind: string")],
            "actions[0].set",
            false,
        ),
        replace(
            "hive inside key",
            &[(
                r"key: Software\PcOptimizerTest",
                r"key: HKCU\Software\PcOptimizerTest",
            )],
            "actions[0].key",
            false,
        ),
        replace(
            "protected service",
            &[("name: ExampleSvc", "name: WinDefend")],
            "safety",
            false,
        ),
        replace(
            "UAC value",
            &[
                (
                    r"Software\PcOptimizerTest\Example",
                    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System",
                ),
                ("value: Enabled", "value: EnableLUA"),
            ],
            "safety",
            false,
        ),
        replace(
            "YAML syntax error",
            &[("risk: safe", "risk: [safe")],
            "fixture.yaml",
            false,
        ),
        replace(
            "only_if with delete",
            &[(
                "    kind: dword\n    set: 0",
                "    delete: true\n    only_if: 1",
            )],
            "actions[0].only_if",
            true,
        ),
        replace(
            "only_if of the wrong type",
            &[("    set: 0", "    set: 0\n    only_if: \"1\"")],
            "actions[0].only_if",
            false,
        ),
        replace(
            "only_if equal to set",
            &[("    set: 0", "    set: 0\n    only_if: 0")],
            "actions[0].only_if",
            false,
        ),
    ]
}

#[test]
fn base_fixture_is_valid() {
    let r = recipe::parse_recipe(VALID, Path::new("fixture.yaml")).unwrap();
    assert_eq!(r.id, "test.example-recipe");
    assert_eq!(r.actions.len(), 2);
    assert!(r.needs_admin(), "service actions need admin");
}

#[test]
fn invalid_recipes_are_rejected_with_file_and_field() {
    for case in cases() {
        let err = match recipe::parse_recipe(&case.text, Path::new("fixture.yaml")) {
            Ok(_) => panic!("case `{}` was accepted:\n{}", case.name, case.text),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains("fixture.yaml"),
            "case `{}`: no file name in: {err}",
            case.name
        );
        assert!(
            err.contains(case.expect),
            "case `{}`: expected `{}` in: {err}",
            case.name,
            case.expect
        );
    }
}

#[test]
fn all_repository_recipes_load() {
    let recipes = recipe::load_dir(&recipes_dir()).unwrap_or_else(|e| panic!("{e}"));
    let ids: Vec<&str> = recipes.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "cleanup.launcher-caches",
            "cleanup.recycle-bin",
            "cleanup.user-temp",
            "cleanup.windows-temp",
            "debloat.app-clipchamp",
            "debloat.app-copilot",
            "debloat.app-family",
            "debloat.app-feedback-hub",
            "debloat.app-get-help",
            "debloat.app-maps",
            "debloat.app-news",
            "debloat.app-office-hub",
            "debloat.app-power-automate",
            "debloat.app-solitaire",
            "debloat.app-teams",
            "debloat.app-tips",
            "debloat.app-todo",
            "debloat.app-weather",
            "debloat.copilot-off",
            "debloat.edge-background-off",
            "debloat.recall-off",
            "debloat.widgets-off",
            "gaming.game-dvr-off",
            "gaming.game-mode-on",
            "gaming.hags-on",
            "input.filter-keys-hotkey-off",
            "input.mouse-acceleration-off",
            "input.sticky-keys-hotkey-off",
            "network.eee-off",
            "power.core-parking-off",
            "power.high-performance-plan",
            "power.usb-suspend-off",
            "privacy.activity-history-off",
            "privacy.advertising-id-off",
            "privacy.start-web-search-off",
            "privacy.telemetry-minimum",
            "services.fax-disabled",
            "windows.feature-updates-deferred",
            "windows.taskbar-end-task",
        ]
    );
    // Every recipe states its sources, and cleanups are never in a profile.
    for r in &recipes {
        assert!(!r.sources.is_empty(), "{}", r.id);
        if r.is_cleanup() {
            assert!(r.profiles.is_empty(), "{}", r.id);
        }
    }

    let sticky = recipes
        .iter()
        .find(|r| r.id == "input.sticky-keys-hotkey-off")
        .unwrap();
    match &sticky.actions[..] {
        [Action::Registry(a)] => {
            assert_eq!(a.hive, Hive::CurrentUser);
            assert_eq!(a.op, RegistryOp::Set(RegValue::String("506".into())));
            assert_eq!(a.only_if, Some(RegValue::String("510".into())));
        }
        other => panic!("unexpected actions: {other:?}"),
    }
}

#[test]
fn load_dir_reports_every_problem_and_duplicates() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("load_dir_reports_every_problem");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("a.yaml"), VALID).unwrap();
    fs::write(dir.join("sub").join("b.yml"), VALID).unwrap(); // same id as a.yaml
    fs::write(
        dir.join("broken.yaml"),
        VALID.replace("risk: safe", "risk: yolo"),
    )
    .unwrap();
    fs::write(dir.join("notes.txt"), "not a recipe").unwrap();

    let errors = recipe::load_dir(&dir).unwrap_err().0;
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors
        .iter()
        .any(|e| matches!(e, RecipeError::Invalid { .. })));
    assert!(errors
        .iter()
        .any(|e| matches!(e, RecipeError::DuplicateId { id, .. } if id == "test.example-recipe")));
}

#[test]
fn load_dir_rejects_missing_or_empty_folder() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("load_dir_empty");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let errors = recipe::load_dir(&dir).unwrap_err().0;
    assert!(matches!(errors[..], [RecipeError::NoRecipes { .. }]));

    let errors = recipe::load_dir(&dir.join("does-not-exist")).unwrap_err().0;
    assert!(matches!(errors[..], [RecipeError::Io { .. }]));
}

// ---------------------------------------------------------------------------
// Rust validation and the JSON schema must agree.
// ---------------------------------------------------------------------------

fn schema_validator() -> jsonschema::Validator {
    let path = recipes_dir().join("schema").join("recipe.schema.json");
    let schema: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    jsonschema::validator_for(&schema).unwrap()
}

fn yaml_to_json(text: &str) -> serde_json::Value {
    serde_yaml_ng::from_str(text).unwrap()
}

fn yaml_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            yaml_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "yaml" || e == "yml") {
            out.push(path);
        }
    }
}

#[test]
fn schema_accepts_every_repository_recipe() {
    let validator = schema_validator();
    let mut files = Vec::new();
    yaml_files(&recipes_dir(), &mut files);
    assert!(!files.is_empty());
    for file in files {
        let json = yaml_to_json(&fs::read_to_string(&file).unwrap());
        let errors: Vec<String> = validator
            .iter_errors(&json)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{}: {errors:?}", file.display());
    }
}

#[test]
fn schema_rejects_what_it_can_express() {
    let validator = schema_validator();
    assert!(validator.is_valid(&yaml_to_json(VALID)));
    for case in cases().into_iter().filter(|c| c.schema_rejects) {
        assert!(
            !validator.is_valid(&yaml_to_json(&case.text)),
            "schema accepted case `{}`",
            case.name
        );
    }
}
