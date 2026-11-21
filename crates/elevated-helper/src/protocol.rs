//! What the app may ask the elevated helper, and what the helper answers.
//!
//! One request per helper run, one JSON object per line. The helper accepts **only** recipe
//! IDs (CLAUDE.md: the elevated helper takes recipe IDs, nothing else): no paths, no registry
//! keys, no commands. Anything else is refused before the engine is even created.
//!
//! The one exception is a benchmark (PresentMon needs administrator rights): it takes a
//! supported game's short name or a bare executable name, used only as PresentMon's process
//! filter, and numbers within fixed limits. It changes no setting.
//!
//! Request:  `{"v":1,"op":"apply","ids":["gaming.game-mode-on"]}`
//!           `{"v":1,"op":"undo","ids":["gaming.game-mode-on"]}`
//!           `{"v":1,"op":"undo_all"}`
//!           `{"v":1,"op":"clean","ids":["cleanup.windows-temp"]}`
//!           `{"v":1,"op":"repair","action":"network_reset"}`
//!           `{"v":1,"op":"startup","changes":[{"id":"startup.user-run.1a2b3c4d","enable":false}]}`
//!           `{"v":1,"op":"bench","target":"cs2","label":"before","runs":3,"duration":60,"warmup":10}`
//! Events:   `{"event":"hello","version":1}`, then `started` / `result` per recipe, then `done`
//!           (or a single `refused` / `blocked` / `error`).

use pco_core::bench::Metrics;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bumped when a request or event changes shape; app and helper ship together.
pub const VERSION: u32 = 1;
/// Longest request line accepted, in bytes.
pub const MAX_REQUEST: usize = 64 * 1024;
/// Most recipe IDs in one request.
pub const MAX_IDS: usize = 256;
/// Longest recipe ID accepted.
pub const MAX_ID_LEN: usize = 100;
/// Benchmark limits: runs, seconds measured per run, seconds of warm-up.
pub const BENCH_RUNS: std::ops::RangeInclusive<u32> = 1..=5;
pub const BENCH_DURATION: std::ops::RangeInclusive<u32> = 10..=300;
pub const BENCH_WARMUP: std::ops::RangeInclusive<u32> = 0..=60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// `restore_point`: create a Windows restore point first (Settings, optional).
    Apply {
        ids: Vec<String>,
        restore_point: bool,
    },
    Undo {
        ids: Vec<String>,
    },
    UndoAll,
    /// Run built-in cleanup recipes (system folders only; the app cleans the user's own).
    Clean {
        ids: Vec<String>,
    },
    /// One fixed troubleshooting action (Settings › Troubleshooting).
    Repair {
        action: pco_core::repair::RepairAction,
    },
    /// Turn startup programs on or off (`startup.*` IDs from the helper's own listing).
    Startup {
        changes: Vec<StartupChange>,
    },
    /// Measure FPS: `after` false = before the tweaks.
    Bench {
        target: String,
        after: bool,
        runs: u32,
        duration: u32,
        warmup: u32,
    },
}

impl Request {
    /// The request line the app sends.
    pub fn to_line(&self) -> String {
        let value = match self {
            Request::Apply { ids, restore_point } => serde_json::json!({
                "v": VERSION, "op": "apply", "ids": ids, "restore_point": restore_point
            }),
            Request::Undo { ids } => serde_json::json!({"v": VERSION, "op": "undo", "ids": ids}),
            Request::UndoAll => serde_json::json!({"v": VERSION, "op": "undo_all"}),
            Request::Clean { ids } => serde_json::json!({"v": VERSION, "op": "clean", "ids": ids}),
            Request::Repair { action } => {
                serde_json::json!({"v": VERSION, "op": "repair", "action": action.name()})
            }
            Request::Startup { changes } => serde_json::json!({
                "v": VERSION, "op": "startup",
                "changes": changes.iter().map(|c| serde_json::json!({"id": c.id, "enable": c.enable})).collect::<Vec<_>>(),
            }),
            Request::Bench {
                target,
                after,
                runs,
                duration,
                warmup,
            } => serde_json::json!({
                "v": VERSION, "op": "bench", "target": target,
                "label": if *after { "after" } else { "before" },
                "runs": runs, "duration": duration, "warmup": warmup,
            }),
        };
        value.to_string()
    }
}

/// Why a request was refused. Nothing was changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum Refusal {
    TooLarge,
    NotJson,
    /// The app and the helper are from different versions.
    Version(Option<u64>),
    /// Missing, extra or wrongly typed fields; names the field.
    BadShape(String),
    /// Not a recipe ID (wrong characters, too long).
    BadId(String),
    /// Well formed, but not one of the recipes built into the helper.
    UnknownRecipe(String),
    /// Not a supported game or a bare executable name.
    BadTarget(String),
    /// The helper was started under another Windows account than the app (UAC with a
    /// different administrator): settings "for the current user" would reach the wrong user.
    OtherUser,
    /// The process on the other end of the channel is not the one that started the helper.
    WrongPeer,
    /// No request arrived in time.
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum BlockReason {
    NotWindows11 {
        build: u32,
    },
    NotX64 {
        arch: String,
    },
    NeedsAdmin,
    GameRunning {
        processes: Vec<String>,
    },
    /// A session left `pending` by a crash could not be undone first.
    RecoveryFailed {
        errors: Vec<String>,
    },
    /// A journal entry the helper would act on does not match the built-in recipe it names
    /// (edited by something else); nothing is changed.
    JournalUntrusted {
        recipe: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum NotApplicableReason {
    ServiceMissing {
        name: String,
    },
    /// The recipe's platform limits exclude this PC (build, edition, desktop/laptop).
    Platform {
        detail: String,
    },
    /// An `only_if` value does not match: the user chose something else.
    Precondition {
        target: String,
    },
    PowerPlanMissing,
    PowerSettingMissing,
    NoNetworkAdapter,
    AppMissing,
    /// Windows keeps no copy of the app, so it is left installed.
    AppNotRestorable,
    /// Cleanups run on their own, after a confirmation.
    Cleanup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum Outcome {
    Applied {
        changed: usize,
    },
    AlreadyApplied,
    NotApplicable {
        reason: NotApplicableReason,
    },
    /// Nothing of the recipe remains changed if `rolled_back` is true (apply), or the recipe
    /// is still applied if `rolled_back` is true (undo).
    Failed {
        error: String,
        rolled_back: bool,
    },
    Undone {
        restored: usize,
    },
    NothingToUndo,
    /// A troubleshooting action finished without errors.
    Repaired,
    /// A cleanup ran (it has no undo).
    Cleaned {
        freed_bytes: u64,
        deleted: u64,
        skipped: u64,
    },
}

/// The strongest restart any changed recipe needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Restart {
    None,
    Explorer,
    Signout,
    Reboot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Hello {
        version: u32,
    },
    Refused {
        reason: Refusal,
    },
    Blocked {
        reason: BlockReason,
    },
    /// A session interrupted earlier (crash, power cut) was undone before this request.
    Recovered {
        session: i64,
        restored: usize,
    },
    Started {
        id: String,
    },
    Result {
        id: String,
        outcome: Outcome,
    },
    /// The restore point asked for before applying: created, or why not (`message`).
    RestorePoint {
        created: bool,
        message: Option<String>,
    },
    /// Benchmark: waiting for the game to start (the app launched it).
    BenchWaitingGame {
        processes: Vec<String>,
    },
    /// Benchmark: press Ctrl+Shift+F10 in the match to start run `run`.
    BenchWaitingKey {
        run: u32,
        runs: u32,
    },
    /// Benchmark: the warm-up is over; measuring for `seconds`.
    BenchMeasuring {
        run: u32,
        seconds: u32,
    },
    BenchRun {
        run: u32,
        metrics: Metrics,
    },
    /// Benchmark: the runs measured so far are saved under `series` (then `done`).
    BenchFinished {
        series: i64,
        runs: u32,
    },
    Done {
        restart: Restart,
    },
    Error {
        message: String,
    },
}

impl Event {
    pub fn to_line(&self) -> String {
        // Serializing these plain enums cannot fail.
        serde_json::to_string(self)
            .unwrap_or_else(|_| r#"{"event":"error","message":"serialize"}"#.into())
    }

    pub fn from_line(line: &str) -> Option<Event> {
        serde_json::from_str(line).ok()
    }

    /// The last event of a run.
    pub fn is_final(&self) -> bool {
        matches!(
            self,
            Event::Refused { .. }
                | Event::Blocked { .. }
                | Event::Done { .. }
                | Event::Error { .. }
        )
    }
}

/// Recipe IDs look like `gaming.game-mode-on`: lowercase letters, digits, `-`, at least one
/// `.`, no empty part.
pub fn is_recipe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id.contains('.')
        && id.split('.').all(|part| {
            !part.is_empty()
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn parse_bench(map: &serde_json::Map<String, Value>) -> Result<Request, Refusal> {
    let number = |field: &str, range: std::ops::RangeInclusive<u32>| {
        map.get(field)
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| range.contains(n))
            .ok_or_else(|| Refusal::BadShape(field.to_owned()))
    };
    let Some(Value::String(target)) = map.get("target") else {
        return Err(Refusal::BadShape("target".into()));
    };
    if pco_core::bench::target::resolve(target).is_none() {
        return Err(Refusal::BadTarget(
            target.chars().take(MAX_ID_LEN).collect(),
        ));
    }
    let after = match map.get("label").and_then(Value::as_str) {
        Some("before") => false,
        Some("after") => true,
        _ => return Err(Refusal::BadShape("label".into())),
    };
    Ok(Request::Bench {
        target: target.trim().to_owned(),
        after,
        runs: number("runs", BENCH_RUNS)?,
        duration: number("duration", BENCH_DURATION)?,
        warmup: number("warmup", BENCH_WARMUP)?,
    })
}

/// Checks the shape of a request line. Whether the IDs are known is checked by the server,
/// against the recipes built into it.
/// One startup program to turn on or off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupChange {
    pub id: String,
    pub enable: bool,
}

/// `changes`: 1..=MAX_IDS objects `{"id": "startup.…", "enable": bool}`, nothing else.
fn parse_startup(map: &serde_json::Map<String, Value>) -> Result<Request, Refusal> {
    let Some(Value::Array(items)) = map.get("changes") else {
        return Err(Refusal::BadShape("changes".into()));
    };
    if items.is_empty() || items.len() > MAX_IDS {
        return Err(Refusal::BadShape("changes".into()));
    }
    let mut changes: Vec<StartupChange> = Vec::with_capacity(items.len());
    for item in items {
        let Value::Object(change) = item else {
            return Err(Refusal::BadShape("changes".into()));
        };
        if change.len() != 2 {
            return Err(Refusal::BadShape("changes".into()));
        }
        let (Some(Value::String(id)), Some(Value::Bool(enable))) =
            (change.get("id"), change.get("enable"))
        else {
            return Err(Refusal::BadShape("changes".into()));
        };
        if !is_recipe_id(id) || !id.starts_with("startup.") {
            return Err(Refusal::BadId(id.chars().take(MAX_ID_LEN).collect()));
        }
        if !changes.iter().any(|c| &c.id == id) {
            changes.push(StartupChange {
                id: id.clone(),
                enable: *enable,
            });
        }
    }
    Ok(Request::Startup { changes })
}

pub fn parse_request(line: &str) -> Result<Request, Refusal> {
    if line.len() > MAX_REQUEST {
        return Err(Refusal::TooLarge);
    }
    let value: Value = serde_json::from_str(line.trim()).map_err(|_| Refusal::NotJson)?;
    let Value::Object(map) = value else {
        return Err(Refusal::BadShape("request".into()));
    };
    match map.get("v") {
        Some(Value::Number(n)) if n.as_u64() == Some(u64::from(VERSION)) => {}
        Some(Value::Number(n)) => return Err(Refusal::Version(n.as_u64())),
        _ => return Err(Refusal::Version(None)),
    }
    let op = match map.get("op") {
        Some(Value::String(op)) => op.as_str(),
        _ => return Err(Refusal::BadShape("op".into())),
    };
    let allowed: &[&str] = match op {
        "apply" => &["v", "op", "ids", "restore_point"],
        "undo" | "clean" => &["v", "op", "ids"],
        "startup" => &["v", "op", "changes"],
        "repair" => &["v", "op", "action"],
        "undo_all" => &["v", "op"],
        "bench" => &["v", "op", "target", "label", "runs", "duration", "warmup"],
        _ => return Err(Refusal::BadShape("op".into())),
    };
    if let Some(extra) = map.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(Refusal::BadShape(extra.clone()));
    }
    if op == "undo_all" {
        return Ok(Request::UndoAll);
    }
    if op == "bench" {
        return parse_bench(&map);
    }
    if op == "startup" {
        return parse_startup(&map);
    }
    if op == "repair" {
        return match map.get("action") {
            Some(Value::String(name)) => pco_core::repair::RepairAction::from_name(name)
                .map(|action| Request::Repair { action })
                .ok_or_else(|| Refusal::BadShape("action".into())),
            _ => Err(Refusal::BadShape("action".into())),
        };
    }
    let Some(Value::Array(items)) = map.get("ids") else {
        return Err(Refusal::BadShape("ids".into()));
    };
    if items.is_empty() || items.len() > MAX_IDS {
        return Err(Refusal::BadShape("ids".into()));
    }
    let mut ids = Vec::with_capacity(items.len());
    for item in items {
        let Value::String(id) = item else {
            return Err(Refusal::BadShape("ids".into()));
        };
        if !is_recipe_id(id) {
            return Err(Refusal::BadId(id.chars().take(MAX_ID_LEN).collect()));
        }
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    Ok(if op == "apply" {
        let restore_point = match map.get("restore_point") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(Refusal::BadShape("restore_point".into())),
        };
        Request::Apply { ids, restore_point }
    } else if op == "clean" {
        Request::Clean { ids }
    } else {
        Request::Undo { ids }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_three_requests() {
        assert_eq!(
            parse_request(
                r#"{"v":1,"op":"apply","ids":["gaming.game-mode-on","privacy.advertising-id-off"]}"#
            ),
            Ok(Request::Apply {
                ids: vec![
                    "gaming.game-mode-on".into(),
                    "privacy.advertising-id-off".into()
                ],
                restore_point: false
            })
        );
        assert_eq!(
            parse_request(r#"{"v":1,"op":"apply","ids":["a.b"],"restore_point":true}"#),
            Ok(Request::Apply {
                ids: vec!["a.b".into()],
                restore_point: true
            })
        );
        assert_eq!(
            parse_request(r#"{"v":1,"op":"apply","ids":["a.b"],"restore_point":"yes"}"#),
            Err(Refusal::BadShape("restore_point".into()))
        );
        assert_eq!(
            parse_request(r#"{"v":1,"op":"undo","ids":["a.b"],"restore_point":true}"#),
            Err(Refusal::BadShape("restore_point".into()))
        );
        assert_eq!(
            parse_request(
                r#"{"v":1,"op":"undo","ids":["gaming.game-mode-on","gaming.game-mode-on"]}"#
            ),
            Ok(Request::Undo {
                ids: vec!["gaming.game-mode-on".into()]
            })
        );
        assert_eq!(
            parse_request(r#"{"v":1,"op":"undo_all"}"#),
            Ok(Request::UndoAll)
        );
        for r in [
            Request::Apply {
                ids: vec!["a.b".into()],
                restore_point: true,
            },
            Request::Undo {
                ids: vec!["a.b".into()],
            },
            Request::UndoAll,
        ] {
            assert_eq!(parse_request(&r.to_line()), Ok(r));
        }
    }

    #[test]
    fn refuses_everything_else() {
        let cases = [
            ("not json", Refusal::NotJson),
            ("[1,2]", Refusal::BadShape("request".into())),
            (r#"{"op":"apply","ids":["a.b"]}"#, Refusal::Version(None)),
            (
                r#"{"v":2,"op":"apply","ids":["a.b"]}"#,
                Refusal::Version(Some(2)),
            ),
            (
                r#"{"v":1,"op":"run","cmd":"cmd.exe"}"#,
                Refusal::BadShape("op".into()),
            ),
            (r#"{"v":1,"op":"apply"}"#, Refusal::BadShape("ids".into())),
            (
                r#"{"v":1,"op":"apply","ids":[]}"#,
                Refusal::BadShape("ids".into()),
            ),
            (
                r#"{"v":1,"op":"apply","ids":"a.b"}"#,
                Refusal::BadShape("ids".into()),
            ),
            (
                r#"{"v":1,"op":"apply","ids":[7]}"#,
                Refusal::BadShape("ids".into()),
            ),
            (
                r#"{"v":1,"op":"apply","ids":["a.b"],"key":"HKLM\\SYSTEM"}"#,
                Refusal::BadShape("key".into()),
            ),
            (
                r#"{"v":1,"op":"undo_all","ids":["a.b"]}"#,
                Refusal::BadShape("ids".into()),
            ),
            (
                r#"{"v":1,"op":"apply","ids":["..\\..\\evil.yaml"]}"#,
                Refusal::BadId("..\\..\\evil.yaml".into()),
            ),
            (
                r#"{"v":1,"op":"apply","ids":["Gaming.X"]}"#,
                Refusal::BadId("Gaming.X".into()),
            ),
        ];
        for (line, expected) in cases {
            assert_eq!(parse_request(line), Err(expected), "{line}");
        }
        let huge = format!(
            r#"{{"v":1,"op":"apply","ids":["{}"]}}"#,
            "a".repeat(MAX_REQUEST)
        );
        assert_eq!(parse_request(&huge), Err(Refusal::TooLarge));
        let many: Vec<String> = (0..=MAX_IDS).map(|i| format!("a.b{i}")).collect();
        assert_eq!(
            parse_request(
                &Request::Apply {
                    ids: many,
                    restore_point: false
                }
                .to_line()
            ),
            Err(Refusal::BadShape("ids".into()))
        );
    }

    #[test]
    fn recipe_id_syntax() {
        for ok in ["gaming.game-mode-on", "a.b", "power.plan-2", "a.b.c"] {
            assert!(is_recipe_id(ok), "{ok}");
        }
        for bad in [
            "", "nodot", ".a", "a.", "a..b", "a.-b", "a.b-", "A.b", "a b.c", "a/b.c", "a.b\n",
        ] {
            assert!(!is_recipe_id(bad), "{bad:?}");
        }
        assert!(!is_recipe_id(&format!("a.{}", "b".repeat(MAX_ID_LEN))));
    }

    #[test]
    fn events_round_trip() {
        let events = [
            Event::Hello { version: VERSION },
            Event::Refused {
                reason: Refusal::UnknownRecipe("x.y".into()),
            },
            Event::Refused {
                reason: Refusal::OtherUser,
            },
            Event::Blocked {
                reason: BlockReason::GameRunning {
                    processes: vec!["cs2.exe".into()],
                },
            },
            Event::Recovered {
                session: 3,
                restored: 1,
            },
            Event::Started { id: "a.b".into() },
            Event::Result {
                id: "a.b".into(),
                outcome: Outcome::NotApplicable {
                    reason: NotApplicableReason::ServiceMissing { name: "Fax".into() },
                },
            },
            Event::Result {
                id: "a.b".into(),
                outcome: Outcome::Applied { changed: 2 },
            },
            Event::Done {
                restart: Restart::Reboot,
            },
            Event::Error {
                message: "x".into(),
            },
        ];
        for e in events {
            let line = e.to_line();
            assert!(!line.contains('\n'));
            assert_eq!(Event::from_line(&line), Some(e));
        }
        assert!(Event::Done {
            restart: Restart::None
        }
        .is_final());
        assert!(!Event::Started { id: "a.b".into() }.is_final());
    }
}
