//! The real `pco-helper.exe` over the real pipe (Windows).
//!
//! The tests without `#[ignore]` change nothing on the PC: they send requests that are
//! refused before the engine runs, with the journal in a temporary folder. The `#[ignore]`d
//! test applies and undoes a real recipe and runs only in CI (CLAUDE.md, safety rule 1).

#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use pco_helper::pipe::{self, HelperProcess, RunError, Server};
use pco_helper::protocol::{Event, Outcome, Refusal, Request, Restart};

const HELPER: &str = env!("CARGO_BIN_EXE_pco-helper");

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pco-helper-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs the helper as a plain child (no UAC) with its journal in `data`.
fn run(data: &Path, parent: Option<u32>, request: &Request) -> Result<Vec<Event>, RunError> {
    let server = Server::create().unwrap();
    let child = Command::new(HELPER)
        .args([
            "--pipe",
            server.name(),
            "--parent",
            &parent.unwrap_or_else(std::process::id).to_string(),
        ])
        .env("PCO_DATA_DIR", data)
        .spawn()
        .unwrap();
    let process = HelperProcess::Child(child);
    let mut events = Vec::new();
    let result = pipe::exchange(server, &process, request, &mut |e| events.push(e.clone()));
    let code = process.wait(Duration::from_secs(30));
    assert!(code.is_some(), "the helper did not exit");
    result.map(|()| events)
}

#[test]
fn unknown_recipe_is_refused_over_the_pipe() {
    let data = temp_dir("unknown");
    let events = run(
        &data,
        None,
        &Request::Apply {
            ids: vec!["evil.anything".into()],
        },
    )
    .unwrap();
    assert_eq!(
        events,
        vec![Event::Refused {
            reason: Refusal::UnknownRecipe("evil.anything".into())
        }]
    );
}

#[test]
fn helper_refuses_a_pipe_that_is_not_its_parents() {
    let data = temp_dir("peer");
    // Claim that some other process (the System process) started it.
    let events = run(&data, Some(4), &Request::UndoAll).unwrap();
    assert_eq!(
        events,
        vec![Event::Refused {
            reason: Refusal::WrongPeer
        }]
    );
}

#[test]
fn helper_rejects_bad_arguments() {
    let out = Command::new(HELPER).output().unwrap();
    assert_eq!(out.status.code(), Some(64));
    let out = Command::new(HELPER)
        .args(["--pipe", r"\\.\pipe\something-else", "--parent", "1"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(65),
        "pipes of other programs are refused"
    );
    let out = Command::new(HELPER)
        .args([
            "--pipe",
            r"\\.\pipe\pco-helper-x",
            "--parent",
            "1",
            "--run",
            "cmd",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(64));
}

#[test]
fn pipe_names_are_random_and_private() {
    let a = Server::create().unwrap();
    let b = Server::create().unwrap();
    assert_ne!(a.name(), b.name());
    assert!(a.name().starts_with(pipe::PIPE_PREFIX));
    assert!(pipe::current_user_sid().unwrap().starts_with("S-1-"));
    assert!(pipe::same_user_as(std::process::id()).unwrap());
}

#[test]
fn the_app_gives_up_when_the_helper_exits_without_connecting() {
    let server = Server::create().unwrap();
    // A process that exits at once instead of connecting.
    let child = Command::new(HELPER).spawn().unwrap();
    let process = HelperProcess::Child(child);
    let result = pipe::exchange(server, &process, &Request::UndoAll, &mut |_| {});
    assert_eq!(result, Err(RunError::NoConnection));
}

/// Real change on a throwaway Windows: Game Mode applied and undone through the helper.
#[test]
#[ignore = "changes real settings; runs only in CI (PCO_CI_REAL_TWEAKS=1)"]
fn ci_applies_and_undoes_through_the_helper() {
    assert!(
        std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
            && std::env::var("PCO_CI_REAL_TWEAKS").as_deref() == Ok("1"),
        "refusing to change real settings outside CI"
    );
    use pco_core::backend::windows::WindowsRegistry;
    use pco_core::backend::RegistryBackend;
    use pco_core::recipe::Hive;
    let read = || {
        WindowsRegistry::system()
            .read(
                Hive::CurrentUser,
                r"Software\Microsoft\GameBar",
                "AutoGameModeEnabled",
            )
            .unwrap()
    };
    let data = temp_dir("real");
    let before = read();
    // Make sure there is something to change.
    let events = run(
        &data,
        None,
        &Request::Apply {
            ids: vec!["gaming.game-mode-on".into(), "services.fax-disabled".into()],
        },
    )
    .unwrap();
    println!("apply: {events:?}");
    assert!(matches!(
        events.last(),
        Some(Event::Done {
            restart: Restart::None
        })
    ));
    let game_mode = events.iter().find_map(|e| match e {
        Event::Result { id, outcome } if id == "gaming.game-mode-on" => Some(outcome.clone()),
        _ => None,
    });
    assert!(matches!(
        game_mode,
        Some(Outcome::Applied { .. } | Outcome::AlreadyApplied)
    ));

    let events = run(&data, None, &Request::UndoAll).unwrap();
    println!("undo: {events:?}");
    assert!(matches!(events.last(), Some(Event::Done { .. })));
    assert_eq!(read(), before, "Game Mode value not as before");
}
