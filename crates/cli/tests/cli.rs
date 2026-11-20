//! `pco` commands end to end, on in-memory backends with the real built-in recipes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;

use pco_cli::app::{run, Env, Exit};
use pco_cli::args::{self, Cmd};
use pco_cli::embedded::RECIPES;
use pco_cli::i18n::{self, Texts};
use pco_core::backend::memory::{MemoryRegistry, MemoryServices};
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

struct World {
    reg: MemoryRegistry,
    svc: MemoryServices,
    journal: Journal,
    host: Host,
    running: Vec<String>,
}

impl World {
    fn new() -> Self {
        Self {
            reg: MemoryRegistry::new(),
            svc: MemoryServices::new().with_service(
                "Fax",
                ServiceState {
                    start: ServiceStart::Manual,
                    delayed_auto_start: false,
                },
            ),
            journal: Journal::open_in_memory().unwrap(),
            host: win11(false),
            running: vec!["explorer.exe".into()],
        }
    }

    fn run(&mut self, lang: Lang, cmd: Cmd) -> (Exit, String) {
        let texts = Texts::load(lang).unwrap();
        let mut env = Env {
            registry: &mut self.reg,
            services: &mut self.svc,
            journal: &mut self.journal,
            host: self.host.clone(),
            running: self.running.clone(),
            blocked: BlockedProcesses::builtin().unwrap(),
            recipes: recipe::load_embedded(RECIPES).unwrap(),
        };
        let mut out = Vec::new();
        let exit = run(&mut env, &texts, &cmd, &mut out);
        (exit, String::from_utf8(out).unwrap())
    }

    fn ro(&mut self, cmd: Cmd) -> (Exit, String) {
        self.run(Lang::Ro, cmd)
    }
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

fn placeholders(text: &str) -> BTreeSet<String> {
    text.split('{')
        .skip(1)
        .filter_map(|s| s.split_once('}').map(|(name, _)| name.to_owned()))
        .collect()
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
    assert_eq!(embedded.len(), 6);
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
    // Sticky Keys needs a sign-out, the strongest restart among these recipes.
    assert!(
        out.contains("Unele modificări au efect abia după: delogare și logare din nou."),
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
        out.contains("a fost întreruptă. Am anulat-o automat: 1 modificări restaurate."),
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
