//! Guards: the blocked-process list and, on Windows, the real host probes (read-only).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use pco_core::guard::{check_no_games, Blocked, BlockedProcesses, ProcessKind};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn builtin_list_parses_and_covers_the_launch_games() {
    let list = BlockedProcesses::builtin().unwrap();
    let lower: Vec<String> = list
        .entries()
        .iter()
        .map(|p| p.name.to_lowercase())
        .collect();
    for required in [
        // Anti-cheat named in CLAUDE.md rule 6.
        "vgc.exe",
        "easyanticheat.exe",
        "easyanticheat_eos.exe",
        "beservice.exe",
        "faceitclient.exe",
        // The six launch games (SPEC 1).
        "cs2.exe",
        "valorant-win64-shipping.exe",
        "fortniteclient-win64-shipping.exe",
        "r5apex.exe",
        "r5apex_dx12.exe",
        "league of legends.exe",
        "dota2.exe",
    ] {
        assert!(lower.iter().any(|n| n == required), "{required} missing");
    }
    assert!(list
        .entries()
        .iter()
        .any(|p| p.kind == ProcessKind::AntiCheat));
}

#[test]
fn the_repository_file_is_the_builtin_one() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/blocked-processes.yaml");
    assert_eq!(
        BlockedProcesses::load(&path).unwrap(),
        BlockedProcesses::builtin().unwrap()
    );
}

#[test]
fn running_game_blocks_case_insensitively() {
    let list = BlockedProcesses::builtin().unwrap();
    assert_eq!(
        check_no_games(&names(&["explorer.exe", "Discord.exe"]), &list),
        Ok(())
    );
    match check_no_games(&names(&["explorer.exe", "CS2.EXE", "VGC.exe"]), &list) {
        Err(Blocked::GameRunning { running }) => {
            let found: Vec<&str> = running.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(found, ["vgc.exe", "cs2.exe"]);
            let msg = Blocked::GameRunning { running }.to_string();
            assert!(msg.starts_with("close the game first"), "{msg}");
            assert!(msg.contains("Counter-Strike 2"), "{msg}");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn invalid_lists_are_rejected() {
    let entry = |name: &str, extra: &str| {
        format!("processes:\n  - {{name: '{name}', what: X, kind: game, verified: false{extra}}}\n")
    };
    for (text, why) in [
        ("processes: []\n".to_owned(), "empty"),
        (entry("cs2", ""), "no .exe"),
        (entry(".exe", ""), "no name"),
        (entry(r"C:\Games\cs2.exe", ""), "a path"),
        (entry("cs2.exe", ", oops: 1"), "unknown field"),
        (
            "processes:\n  - {name: cs2.exe, what: X, kind: game, verified: true}\n".to_owned(),
            "verified without source",
        ),
        (
            "processes:\n  - {name: a.exe, what: X, kind: game, verified: false}\n  \
             - {name: A.EXE, what: Y, kind: game, verified: false}\n"
                .to_owned(),
            "duplicate",
        ),
        (
            "processes:\n  - {name: a.exe, what: X, kind: driver, verified: false}\n".to_owned(),
            "unknown kind",
        ),
    ] {
        assert!(
            BlockedProcesses::parse(&text, "test").is_err(),
            "accepted a list with {why}"
        );
    }
}

#[cfg(windows)]
mod real_windows {
    use pco_core::guard::windows::{host, running_processes};
    use pco_core::guard::Arch;

    #[test]
    fn host_facts_are_read() {
        let h = host().unwrap();
        assert!(h.build > 10000, "build {}", h.build);
        // The development PC and GitHub's runners are x64.
        assert_eq!(h.arch, Arch::X64);
    }

    #[test]
    fn this_test_is_among_the_running_processes() {
        let me = std::env::current_exe().unwrap();
        let me = me.file_name().unwrap().to_string_lossy().to_lowercase();
        let running = running_processes().unwrap();
        assert!(running.len() > 5);
        assert!(
            running.iter().any(|n| n.to_lowercase() == me),
            "{me} not in {running:?}"
        );
    }
}
