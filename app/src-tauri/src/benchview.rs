//! What the benchmark card shows: the games to pick from, and the before → after verdict.

use pco_core::bench::target::{game_key, precise, resolve, GAMES};
use pco_core::bench::{compare, summarize, Comparison, Label};
use pco_core::hw::Game;
use pco_core::journal::Journal;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchGame {
    pub key: &'static str,
    pub name: &'static str,
    pub installed: bool,
    /// The app can start it (not through the EA app).
    pub can_launch: bool,
    /// Has a fixed test map; Valorant and LoL are "less precise" (SPEC 4.4).
    pub precise: bool,
}

pub fn games(installed: &[Game]) -> Vec<BenchGame> {
    GAMES
        .iter()
        .map(|&id| {
            let found = installed.iter().find(|g| g.id == id);
            BenchGame {
                key: game_key(id),
                name: id.name(),
                installed: found.is_some(),
                can_launch: found.is_some_and(|g| g.launcher != pco_core::hw::Launcher::Ea),
                precise: precise(id),
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub target: String,
    pub comparison: Comparison,
    /// "better", "same" or "drop" (a real drop: over 3% and over the spread of the runs).
    pub kind: &'static str,
    /// The recipes applied since the "before" series: what "Undo" would undo.
    pub undo_ids: Vec<String>,
}

/// The latest before/after comparison for `target`, if both series exist.
pub fn verdict(journal: &Journal, target: &str) -> Result<Option<Verdict>, String> {
    let key = resolve(target).map(|t| t.key).ok_or("unknown target")?;
    let err = |e: pco_core::journal::JournalError| e.to_string();
    let (Some(before), Some(after)) = (
        journal.latest_series(&key, Label::Before).map_err(err)?,
        journal.latest_series(&key, Label::After).map_err(err)?,
    ) else {
        return Ok(None);
    };
    let (Some(b), Some(a)) = (summarize(&before.runs), summarize(&after.runs)) else {
        return Ok(None);
    };
    let c = compare(&b, &a);
    let kind = if c.significant_drop {
        "drop"
    } else if c.fps_change_pct >= 1.0 && a.avg_fps - b.avg_fps > b.fps_spread.max(a.fps_spread) {
        "better"
    } else {
        "same"
    };
    let undo_ids = if c.significant_drop {
        journal.applied_recipe_ids_since(before.id).map_err(err)?
    } else {
        Vec::new()
    };
    Ok(Some(Verdict {
        target: key,
        comparison: c,
        kind,
        undo_ids,
    }))
}

/// The newest "before" series without an "after" yet: what the "Measure after" card offers.
pub fn open_before(journal: &Journal) -> Result<Option<String>, String> {
    let all = journal.benchmark_series(None).map_err(|e| e.to_string())?;
    let Some(newest) = all.first() else {
        return Ok(None);
    };
    Ok((newest.label == Label::Before).then(|| newest.target.clone()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use pco_core::bench::Metrics;

    fn m(fps: f64) -> Metrics {
        Metrics {
            frames: 6000,
            seconds: 60.0,
            avg_fps: fps,
            low1_fps: fps * 0.6,
            avg_frametime_ms: 1000.0 / fps,
        }
    }

    #[test]
    fn verdicts() {
        let mut j = Journal::open_in_memory().unwrap();
        assert_eq!(verdict(&j, "cs2").unwrap(), None);
        for fps in [140.0, 141.0, 142.0] {
            j.record_benchmark(1, "cs2", Label::Before, &m(fps))
                .unwrap();
        }
        assert_eq!(open_before(&j).unwrap().as_deref(), Some("cs2"));
        for fps in [150.0, 151.0, 152.0] {
            j.record_benchmark(2, "cs2", Label::After, &m(fps)).unwrap();
        }
        let v = verdict(&j, "cs2").unwrap().unwrap();
        assert_eq!(v.kind, "better");
        assert!(v.undo_ids.is_empty());
        assert_eq!(open_before(&j).unwrap(), None);

        for fps in [120.0, 121.0, 122.0] {
            j.record_benchmark(3, "cs2", Label::After, &m(fps)).unwrap();
        }
        assert_eq!(verdict(&j, "cs2").unwrap().unwrap().kind, "drop");
        assert!(verdict(&j, "notagame").is_err());
    }

    #[test]
    fn game_list() {
        let list = games(&[]);
        assert_eq!(list.len(), 6);
        assert_eq!(list[0].key, "cs2");
        assert!(list.iter().all(|g| !g.installed && !g.can_launch));
        assert!(!list.iter().find(|g| g.key == "valorant").unwrap().precise);
    }
}
