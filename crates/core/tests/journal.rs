//! Undo journal: durability, migrations, status rules and queries.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use pco_core::backend::{RegData, RegRead, ServiceStart, ServiceState};
use pco_core::journal::{EntryKind, Journal, JournalError, NewEntry, SessionKind, Status};
use pco_core::recipe::RegValue;

fn temp_db(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("journal-tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("nested").join("journal.db")
}

fn dword(v: u32) -> RegRead {
    RegRead::Present(RegData::from_value(&RegValue::Dword(v)))
}

fn reg_entry<'a>(
    recipe_id: &'a str,
    index: usize,
    previous: &'a RegRead,
    new: &'a RegRead,
) -> NewEntry<'a, RegRead, RegRead> {
    NewEntry {
        recipe_id,
        recipe_version: 1,
        action_index: index,
        kind: EntryKind::Registry,
        target: r"HKCU\Software\PcOptimizerTest\Flag",
        previous_state: previous,
        new_state: new,
    }
}

#[test]
fn creates_folder_and_schema_and_reopens() {
    let path = temp_db("reopen");
    let j = Journal::open(&path).unwrap();
    assert_eq!(j.schema_version().unwrap(), 1);
    drop(j);
    assert!(path.exists());
    let j = Journal::open(&path).unwrap();
    assert_eq!(j.schema_version().unwrap(), 1);
    assert!(j.sessions().unwrap().is_empty());
}

#[test]
fn pending_entry_survives_an_interruption() {
    let path = temp_db("interrupted");
    let previous = RegRead::ValueMissing;
    let new = dword(0);
    {
        let mut j = Journal::open(&path).unwrap();
        let s = j.begin_session(SessionKind::Apply).unwrap();
        j.record_pending(s, &reg_entry("gaming.game-dvr-off", 0, &previous, &new))
            .unwrap();
        // The app stops here: no `applied`, no `finish_session`.
    }

    let j = Journal::open(&path).unwrap();
    let pending = j.pending_sessions().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].kind, SessionKind::Apply);
    let entries = j.entries(pending[0].id).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].status, Status::Pending);
    // The exact previous state is back: "value did not exist", not "0".
    assert_eq!(
        entries[0].previous::<RegRead>().unwrap(),
        RegRead::ValueMissing
    );
    assert_eq!(entries[0].target_state::<RegRead>().unwrap(), dword(0));
}

/// Runs this test binary again as a child process that records a pending entry and then
/// exits abruptly (no destructors, no `finish_session`), like a crash or power cut.
#[test]
fn pending_entry_survives_a_killed_process() {
    const CHILD: &str = "PCO_JOURNAL_CRASH_CHILD";
    if let Ok(path) = std::env::var(CHILD) {
        let mut j = Journal::open(Path::new(&path)).unwrap();
        let s = j.begin_session(SessionKind::Apply).unwrap();
        let (prev, new) = (RegRead::KeyMissing, dword(1));
        j.record_pending(s, &reg_entry("gaming.game-mode-on", 0, &prev, &new))
            .unwrap();
        std::process::exit(3);
    }

    let path = temp_db("killed");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "pending_entry_survives_a_killed_process",
            "--test-threads=1",
        ])
        .env(CHILD, &path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(3), "child did not stop where expected");

    let j = Journal::open(&path).unwrap();
    let pending = j.pending_sessions().unwrap();
    assert_eq!(pending.len(), 1);
    let entries = j.entries(pending[0].id).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].previous::<RegRead>().unwrap(),
        RegRead::KeyMissing
    );
}

#[test]
fn states_round_trip_exactly() {
    let mut j = Journal::open_in_memory().unwrap();
    let s = j.begin_session(SessionKind::Apply).unwrap();

    let registry_states = [
        RegRead::KeyMissing,
        RegRead::ValueMissing,
        dword(0),
        RegRead::Present(RegData {
            reg_type: 3,
            bytes: vec![0, 255, 7],
        }),
        RegRead::Present(RegData::from_value(&RegValue::String("ăîșțâ".into()))),
    ];
    for (i, state) in registry_states.iter().enumerate() {
        let id = j
            .record_pending(s, &reg_entry("t.registry", i, state, state))
            .unwrap();
        assert_eq!(&j.entry(id).unwrap().previous::<RegRead>().unwrap(), state);
    }

    let service_states: [Option<ServiceState>; 2] = [
        None,
        Some(ServiceState {
            start: ServiceStart::Automatic,
            delayed_auto_start: true,
        }),
    ];
    for (i, state) in service_states.iter().enumerate() {
        let id = j
            .record_pending(
                s,
                &NewEntry {
                    recipe_id: "t.service",
                    recipe_version: 1,
                    action_index: i,
                    kind: EntryKind::Service,
                    target: "service Fax",
                    previous_state: state,
                    new_state: state,
                },
            )
            .unwrap();
        let e = j.entry(id).unwrap();
        assert_eq!(e.kind, EntryKind::Service);
        assert_eq!(&e.previous::<Option<ServiceState>>().unwrap(), state);
    }
}

#[test]
fn apply_then_undo_flow() {
    let mut j = Journal::open_in_memory().unwrap();
    let (prev, new) = (RegRead::ValueMissing, dword(0));

    let apply = j.begin_session(SessionKind::Apply).unwrap();
    let a0 = j
        .record_pending(apply, &reg_entry("gaming.a", 0, &prev, &new))
        .unwrap();
    let a1 = j
        .record_pending(apply, &reg_entry("gaming.a", 1, &prev, &new))
        .unwrap();
    let b0 = j
        .record_pending(apply, &reg_entry("gaming.b", 0, &prev, &new))
        .unwrap();
    j.set_entry_status(a0, Status::Applied, None).unwrap();
    j.set_entry_status(a1, Status::Applied, None).unwrap();
    j.set_entry_status(b0, Status::Failed, Some("access denied"))
        .unwrap();
    j.finish_session(apply, Status::Applied).unwrap();

    assert!(j.pending_sessions().unwrap().is_empty());
    let active: Vec<_> = j
        .applied_entries("gaming.a")
        .unwrap()
        .iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(active, [a0, a1]);
    assert!(j.applied_entries("gaming.b").unwrap().is_empty());
    assert_eq!(j.applied_recipe_ids().unwrap(), ["gaming.a"]);
    assert_eq!(j.entry(b0).unwrap().error.as_deref(), Some("access denied"));

    // Undo: its own entries do not count as "applied changes".
    let undo = j.begin_session(SessionKind::Undo).unwrap();
    for (i, original) in [a1, a0].into_iter().enumerate() {
        let u = j
            .record_pending(undo, &reg_entry("gaming.a", i, &new, &prev))
            .unwrap();
        j.set_entry_status(u, Status::Applied, None).unwrap();
        j.set_entry_status(original, Status::RolledBack, None)
            .unwrap();
    }
    j.finish_session(undo, Status::Applied).unwrap();

    assert!(j.applied_entries("gaming.a").unwrap().is_empty());
    assert!(j.applied_recipe_ids().unwrap().is_empty());

    let sessions = j.sessions().unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].id, undo, "newest first");
    assert_eq!(sessions[0].app_version, pco_core::VERSION);
    assert!(sessions[1].finished_at.is_some());
}

#[test]
fn invalid_transitions_are_refused() {
    let mut j = Journal::open_in_memory().unwrap();
    let (prev, new) = (RegRead::ValueMissing, dword(1));
    let s = j.begin_session(SessionKind::Apply).unwrap();
    let e = j
        .record_pending(s, &reg_entry("t.x", 0, &prev, &new))
        .unwrap();

    j.set_entry_status(e, Status::Failed, None).unwrap();
    for to in [
        Status::Applied,
        Status::Pending,
        Status::RolledBack,
        Status::Failed,
    ] {
        assert!(matches!(
            j.set_entry_status(e, to, None),
            Err(JournalError::InvalidTransition { .. })
        ));
    }

    // The same action cannot be recorded twice in one session.
    assert!(matches!(
        j.record_pending(s, &reg_entry("t.x", 0, &prev, &new)),
        Err(JournalError::Sqlite(_))
    ));

    j.finish_session(s, Status::Failed).unwrap();
    assert!(matches!(
        j.finish_session(s, Status::Applied),
        Err(JournalError::InvalidTransition { .. })
    ));
    // No new entries in a finished session.
    assert!(matches!(
        j.record_pending(s, &reg_entry("t.x", 1, &prev, &new)),
        Err(JournalError::InvalidTransition { .. })
    ));
}

#[test]
fn unknown_ids_are_reported() {
    let mut j = Journal::open_in_memory().unwrap();
    assert!(matches!(
        j.finish_session(pco_core::journal::SessionId(42), Status::Applied),
        Err(JournalError::NotFound {
            what: "session",
            id: 42
        })
    ));
    assert!(matches!(
        j.set_entry_status(pco_core::journal::EntryId(7), Status::Applied, None),
        Err(JournalError::NotFound {
            what: "entry",
            id: 7
        })
    ));
}

#[test]
fn journal_from_a_newer_version_is_not_touched() {
    let path = temp_db("newer");
    drop(Journal::open(&path).unwrap());
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(
        Journal::open(&path),
        Err(JournalError::NewerSchema {
            found: 99,
            supported: 1
        })
    ));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "file must be unchanged"
    );
}
