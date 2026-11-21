//! The helper's request handling end to end, on in-memory backends with the real built-in
//! recipes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pco_core::backend::memory::{MemoryRegistry, MemoryServices};
use pco_core::backend::{RegData, RegRead, RegistryBackend, ServiceStart, ServiceState};
use pco_core::guard::{Arch, BlockedProcesses, Host};
use pco_core::journal::{EntryKind, Journal, NewEntry, SessionKind};
use pco_core::recipe::{self, Edition, Hive, RegValue};
use pco_helper::protocol::{
    parse_request, BlockReason, Event, NotApplicableReason, Outcome, Refusal, Request, Restart,
};
use pco_helper::serve::{serve, Env};

const MODE: &str = "gaming.game-mode-on";
const DVR: &str = "gaming.game-dvr-off";
const FAX: &str = "services.fax-disabled";

struct World {
    reg: MemoryRegistry,
    svc: MemoryServices,
    journal: Journal,
    host: Host,
    running: Vec<String>,
    restore_points: u32,
    restore_point_result: Result<Option<String>, String>,
}

impl World {
    fn new() -> Self {
        Self {
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
            journal: Journal::open_in_memory().unwrap(),
            host: Host {
                build: 26100,
                arch: Arch::X64,
                elevated: true,
                edition: Some(Edition::Pro),
                form_factor: None,
            },
            running: vec!["explorer.exe".into()],
            restore_points: 0,
            restore_point_result: Ok(None),
        }
    }

    fn send(&mut self, request: Request) -> Vec<Event> {
        let mut env = Env {
            registry: &mut self.reg,
            services: &mut self.svc,
            journal: &mut self.journal,
            host: self.host.clone(),
            running: self.running.clone(),
            blocked: BlockedProcesses::builtin().unwrap(),
            recipes: recipe::load_embedded(pco_core::embedded::RECIPES).unwrap(),
            restore_point: &mut || {
                self.restore_points += 1;
                self.restore_point_result.clone()
            },
        };
        let mut events = Vec::new();
        serve(&mut env, &request, &mut |e| events.push(e));
        // Every run ends with exactly one final event.
        assert_eq!(
            events.iter().filter(|e| e.is_final()).count(),
            1,
            "{events:?}"
        );
        assert!(events.last().unwrap().is_final());
        events
    }

    fn game_mode(&self) -> RegRead {
        self.reg
            .read(
                Hive::CurrentUser,
                r"Software\Microsoft\GameBar",
                "AutoGameModeEnabled",
            )
            .unwrap()
    }
}

fn apply(ids: &[&str]) -> Request {
    Request::Apply {
        ids: ids.iter().map(|s| (*s).to_owned()).collect(),
        restore_point: false,
    }
}

fn undo(ids: &[&str]) -> Request {
    Request::Undo {
        ids: ids.iter().map(|s| (*s).to_owned()).collect(),
    }
}

#[test]
fn applies_one_recipe_at_a_time_then_undoes_exactly() {
    let mut w = World::new();
    let (reg0, svc0) = (w.reg.clone(), w.svc.clone());

    let events = w.send(apply(&[MODE, DVR, FAX]));
    assert_eq!(
        events,
        vec![
            Event::Started { id: MODE.into() },
            Event::Result {
                id: MODE.into(),
                outcome: Outcome::Applied { changed: 1 }
            },
            Event::Started { id: DVR.into() },
            Event::Result {
                id: DVR.into(),
                outcome: Outcome::Applied { changed: 2 }
            },
            Event::Started { id: FAX.into() },
            Event::Result {
                id: FAX.into(),
                outcome: Outcome::Applied { changed: 1 }
            },
            Event::Done {
                restart: Restart::None
            },
        ]
    );
    assert!(!w.reg.same_contents(&reg0));

    // Applying again changes nothing.
    let again = w.send(apply(&[MODE]));
    assert_eq!(
        again[1],
        Event::Result {
            id: MODE.into(),
            outcome: Outcome::AlreadyApplied
        }
    );

    let events = w.send(Request::UndoAll);
    let undone: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::Result {
                    outcome: Outcome::Undone { .. },
                    ..
                }
            )
        })
        .collect();
    assert_eq!(undone.len(), 3, "{events:?}");
    assert!(w.reg.same_contents(&reg0), "registry not as before");
    assert!(w.svc.same_contents(&svc0), "services not as before");

    // Nothing left: undo all is a no-op.
    assert_eq!(
        w.send(Request::UndoAll),
        vec![Event::Done {
            restart: Restart::None
        }]
    );
}

#[test]
fn undo_by_id() {
    let mut w = World::new();
    w.send(apply(&[MODE]));
    assert!(matches!(w.game_mode(), RegRead::Present(_)));
    let events = w.send(undo(&[MODE]));
    assert_eq!(
        events[1],
        Event::Result {
            id: MODE.into(),
            outcome: Outcome::Undone { restored: 1 }
        }
    );
    assert!(!matches!(w.game_mode(), RegRead::Present(_)));
    let again = w.send(undo(&[MODE]));
    assert_eq!(
        again[1],
        Event::Result {
            id: MODE.into(),
            outcome: Outcome::NothingToUndo
        }
    );
}

#[test]
fn refuses_recipes_that_are_not_built_in() {
    let mut w = World::new();
    let reg0 = w.reg.clone();
    for request in [apply(&[MODE, "evil.anything"]), undo(&["evil.anything"])] {
        assert_eq!(
            w.send(request),
            vec![Event::Refused {
                reason: Refusal::UnknownRecipe("evil.anything".into())
            }]
        );
    }
    assert!(w.reg.same_contents(&reg0));
    assert!(w.journal.sessions().unwrap().is_empty());
}

#[test]
fn malformed_requests_never_reach_the_engine() {
    for line in [
        r#"{"v":1,"op":"apply","ids":["gaming.game-mode-on"],"path":"C:\\x.yaml"}"#,
        r#"{"v":1,"op":"exec","cmd":"reg add HKLM\\..."}"#,
        r#"{"v":1,"op":"apply","ids":["HKLM\\SYSTEM\\x"]}"#,
    ] {
        assert!(parse_request(line).is_err(), "{line}");
    }
}

#[test]
fn blocked_while_a_game_runs() {
    let mut w = World::new();
    w.running.push("cs2.exe".into());
    let events = w.send(apply(&[MODE]));
    assert!(
        matches!(&events[..], [Event::Blocked { reason: BlockReason::GameRunning { processes } }] if processes == &["cs2.exe".to_owned()]),
        "{events:?}"
    );
    assert!(!matches!(w.game_mode(), RegRead::Present(_)));
}

#[test]
fn admin_only_when_needed() {
    let mut w = World::new();
    w.host.elevated = false;
    assert_eq!(
        w.send(apply(&[MODE, FAX])),
        vec![Event::Blocked {
            reason: BlockReason::NeedsAdmin
        }]
    );
    assert!(!matches!(w.game_mode(), RegRead::Present(_)));
    // Settings for the current user alone do not need it.
    let events = w.send(apply(&[MODE]));
    assert_eq!(
        events.last(),
        Some(&Event::Done {
            restart: Restart::None
        })
    );
}

#[test]
fn not_windows_11() {
    let mut w = World::new();
    w.host.build = 19045;
    assert_eq!(
        w.send(apply(&[MODE])),
        vec![Event::Blocked {
            reason: BlockReason::NotWindows11 { build: 19045 }
        }]
    );
}

#[test]
fn reports_recipes_that_do_not_apply() {
    let mut w = World::new();
    w.svc = MemoryServices::new();
    let events = w.send(apply(&[FAX]));
    assert_eq!(
        events[1],
        Event::Result {
            id: FAX.into(),
            outcome: Outcome::NotApplicable {
                reason: NotApplicableReason::ServiceMissing { name: "Fax".into() }
            }
        }
    );
}

/// A pending session, as a crash right after a write leaves it, for `target_key`.
fn pending_entry(w: &mut World, recipe: &str, hive: &str, key: &str, value: &str) {
    let s = w.journal.begin_session(SessionKind::Apply).unwrap();
    let previous = serde_json::json!({
        "type": "registry", "hive": hive, "key": key, "value": value,
        "state": {"state": "value_missing"},
    });
    let new = serde_json::json!({"type": "registry", "hive": hive, "key": key, "value": value,
        "state": {"state": "present", "data": {"reg_type": 4, "bytes": [1,0,0,0]}}});
    w.journal
        .record_pending(
            s,
            &NewEntry {
                recipe_id: recipe,
                recipe_version: 1,
                action_index: 0,
                kind: EntryKind::Registry,
                target: "x",
                previous_state: &previous,
                new_state: &new,
            },
        )
        .unwrap();
}

#[test]
fn recovers_an_interrupted_session_first() {
    let mut w = World::new();
    w.reg
        .write(
            Hive::CurrentUser,
            r"Software\Microsoft\GameBar",
            "AutoGameModeEnabled",
            &RegData::from_value(&RegValue::Dword(1)),
        )
        .unwrap();
    pending_entry(
        &mut w,
        MODE,
        "HKCU",
        r"Software\Microsoft\GameBar",
        "AutoGameModeEnabled",
    );
    let events = w.send(apply(&[DVR]));
    assert!(
        matches!(events[0], Event::Recovered { restored: 1, .. }),
        "{events:?}"
    );
    assert_eq!(
        events.last(),
        Some(&Event::Done {
            restart: Restart::None
        })
    );
    assert!(w.journal.pending_sessions().unwrap().is_empty());
}

#[test]
fn refuses_a_journal_edited_to_point_elsewhere() {
    // Something running as the user rewrote the journal so that "undoing Game Mode" would
    // write an HKLM value of its choosing with administrator rights.
    let mut w = World::new();
    pending_entry(
        &mut w,
        MODE,
        "HKLM",
        r"SYSTEM\CurrentControlSet\Services\Evil",
        "Start",
    );
    let reg0 = w.reg.clone();
    let events = w.send(apply(&[DVR]));
    assert_eq!(
        events,
        vec![Event::Blocked {
            reason: BlockReason::JournalUntrusted {
                recipe: MODE.into()
            }
        }]
    );
    assert!(w.reg.same_contents(&reg0));
    assert_eq!(
        w.journal.pending_sessions().unwrap().len(),
        1,
        "left for a person to look at"
    );

    // An entry naming a recipe that is not built in is refused too.
    let mut w = World::new();
    pending_entry(
        &mut w,
        "evil.recipe",
        "HKCU",
        r"Software\Microsoft\GameBar",
        "AutoGameModeEnabled",
    );
    assert!(matches!(
        &w.send(Request::UndoAll)[..],
        [Event::Blocked {
            reason: BlockReason::JournalUntrusted { .. }
        }]
    ));
}

#[test]
fn restore_point_first_when_asked() {
    let mut w = World::new();
    let events = w.send(Request::Apply {
        ids: vec![MODE.into()],
        restore_point: true,
    });
    assert_eq!(
        events[0],
        Event::RestorePoint {
            created: true,
            message: None
        }
    );
    assert_eq!(events[1], Event::Started { id: MODE.into() });
    assert_eq!(w.restore_points, 1);

    // Not asked: none. Failed: reported, and the tweaks still apply.
    let mut w = World::new();
    w.send(apply(&[MODE]));
    assert_eq!(w.restore_points, 0);
    w.restore_point_result = Err("System Restore is off".into());
    let events = w.send(Request::Apply {
        ids: vec![DVR.into()],
        restore_point: true,
    });
    assert_eq!(
        events[0],
        Event::RestorePoint {
            created: false,
            message: Some("System Restore is off".into())
        }
    );
    assert_eq!(
        events.last(),
        Some(&Event::Done {
            restart: Restart::None
        })
    );
}
