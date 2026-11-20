//! Benchmark history in the journal: series, latest before/after, forgetting, the upgrade from
//! a Stage 0/1 journal, and which recipes to undo after a drop.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use pco_core::backend::{RegData, RegRead};
use pco_core::bench::{Label, Metrics};
use pco_core::journal::{EntryKind, Journal, NewEntry, SessionKind, Status};
use pco_core::recipe::RegValue;

fn run(fps: f64) -> Metrics {
    Metrics {
        frames: (fps * 60.0) as u64,
        seconds: 60.0,
        avg_fps: fps,
        low1_fps: fps * 0.7,
        avg_frametime_ms: 1000.0 / fps,
    }
}

#[test]
fn series_are_grouped_and_newest_first() {
    let mut j = Journal::open_in_memory().unwrap();
    for fps in [140.0, 142.0, 144.0] {
        j.record_benchmark(1_000, "cs2", Label::Before, &run(fps))
            .unwrap();
    }
    for fps in [150.0, 151.0] {
        j.record_benchmark(2_000, "cs2", Label::After, &run(fps))
            .unwrap();
    }
    j.record_benchmark(3_000, "msedge.exe", Label::Before, &run(60.0))
        .unwrap();

    let all = j.benchmark_series(None).unwrap();
    let summary: Vec<(i64, &str, Label, usize)> = all
        .iter()
        .map(|s| (s.id, s.target.as_str(), s.label, s.runs.len()))
        .collect();
    assert_eq!(
        summary,
        [
            (3_000, "msedge.exe", Label::Before, 1),
            (2_000, "cs2", Label::After, 2),
            (1_000, "cs2", Label::Before, 3),
        ]
    );
    let before = j.latest_series("cs2", Label::Before).unwrap().unwrap();
    assert_eq!(
        before.runs.iter().map(|m| m.avg_fps).collect::<Vec<_>>(),
        [140.0, 142.0, 144.0]
    );
    assert!(j.latest_series("dota2", Label::Before).unwrap().is_none());

    // A newer "before" series replaces the older one as the latest.
    j.record_benchmark(4_000, "cs2", Label::Before, &run(120.0))
        .unwrap();
    assert_eq!(
        j.latest_series("cs2", Label::Before).unwrap().unwrap().id,
        4_000
    );
}

#[test]
fn forgetting_one_target_or_everything() {
    let mut j = Journal::open_in_memory().unwrap();
    j.record_benchmark(1, "cs2", Label::Before, &run(100.0))
        .unwrap();
    j.record_benchmark(2, "dota2", Label::Before, &run(100.0))
        .unwrap();
    j.record_benchmark(2, "dota2", Label::Before, &run(101.0))
        .unwrap();
    assert_eq!(j.forget_benchmarks(Some("dota2")).unwrap(), 2);
    assert_eq!(j.benchmark_series(None).unwrap().len(), 1);
    assert_eq!(j.forget_benchmarks(None).unwrap(), 1);
    assert!(j.benchmark_series(None).unwrap().is_empty());
}

#[test]
fn recipes_applied_since_the_before_series() {
    let mut j = Journal::open_in_memory().unwrap();
    let state = RegRead::Present(RegData::from_value(&RegValue::Dword(1)));
    let apply = |j: &mut Journal, recipe: &str| {
        let s = j.begin_session(SessionKind::Apply).unwrap();
        let e = j
            .record_pending(
                s,
                &NewEntry {
                    recipe_id: recipe,
                    recipe_version: 1,
                    action_index: 0,
                    kind: EntryKind::Registry,
                    target: "x",
                    previous_state: &state,
                    new_state: &state,
                },
            )
            .unwrap();
        j.set_entry_status(e, Status::Applied, None).unwrap();
        j.finish_session(s, Status::Applied).unwrap();
    };
    apply(&mut j, "gaming.old");
    std::thread::sleep(std::time::Duration::from_millis(5));
    let before_started = j.sessions().unwrap()[0].started_at + 1;
    std::thread::sleep(std::time::Duration::from_millis(5));
    apply(&mut j, "gaming.new");
    assert_eq!(
        j.applied_recipe_ids_since(before_started).unwrap(),
        ["gaming.new"]
    );
    assert_eq!(
        j.applied_recipe_ids_since(0).unwrap(),
        ["gaming.new", "gaming.old"]
    );
}

#[test]
fn a_stage_1_journal_is_upgraded_and_keeps_its_sessions() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bench-upgrade");
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("journal.db");
    {
        let mut j = Journal::open(&path).unwrap();
        j.begin_session(SessionKind::Apply).unwrap();
    }
    // Turn it back into a schema-1 journal, as Stage 0/1 left it.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("DROP TABLE benchmarks; PRAGMA user_version = 1;")
            .unwrap();
    }
    let mut j = Journal::open(&path).unwrap();
    assert_eq!(j.schema_version().unwrap(), 2);
    assert_eq!(j.sessions().unwrap().len(), 1, "existing sessions are kept");
    j.record_benchmark(1, "cs2", Label::Before, &run(100.0))
        .unwrap();
    assert_eq!(j.benchmark_series(None).unwrap().len(), 1);
}
