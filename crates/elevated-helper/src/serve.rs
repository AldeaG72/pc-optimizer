//! Runs one request with the same guards as `pco`, one recipe at a time, reporting each
//! result as it happens (the app shows a live list). Works on trait objects, so the tests
//! drive it with in-memory backends on any OS.

use pco_core::backend::{RegistryBackend, ServiceBackend};
use pco_core::engine::{
    self, ApplyOutcome, Engine, RecipePlan, RecipeStatus, Snapshot, UndoOutcome,
};
use pco_core::guard::{
    check_host, check_no_games, Blocked, BlockedProcesses, Host, PlatformMismatch, SystemInfo,
};
use pco_core::journal::{Entry, Journal};
use pco_core::recipe::{Action, Recipe, RequiresRestart};

use crate::protocol::{
    BlockReason, Event, NotApplicableReason, Outcome, Refusal, Request, Restart, MAX_ID_LEN,
};

/// Everything a request works with.
pub struct Env<'a> {
    pub registry: &'a mut dyn RegistryBackend,
    pub services: &'a mut dyn ServiceBackend,
    pub journal: &'a mut Journal,
    pub host: Host,
    /// Executable names of the running processes.
    pub running: Vec<String>,
    pub blocked: BlockedProcesses,
    /// The recipes built into the helper; the only ones it can apply.
    pub recipes: Vec<Recipe>,
}

impl Env<'_> {
    fn engine(&mut self) -> Engine<'_> {
        let system = SystemInfo::from(&self.host);
        Engine::new(
            &mut *self.registry,
            &mut *self.services,
            &mut *self.journal,
            system,
        )
    }

    fn recipe(&self, id: &str) -> Option<&Recipe> {
        self.recipes.iter().find(|r| r.id == id)
    }
}

/// Runs `request`; every step is reported through `emit`, the last event is final.
pub fn serve(env: &mut Env<'_>, request: &Request, emit: &mut dyn FnMut(Event)) {
    if let Err(e) = run(env, request, emit) {
        emit(Event::Error {
            message: e.to_string(),
        });
    }
}

fn run(
    env: &mut Env<'_>,
    request: &Request,
    emit: &mut dyn FnMut(Event),
) -> Result<(), engine::EngineError> {
    if let Err(b) = check_host(&env.host) {
        emit(Event::Blocked { reason: block(&b) });
        return Ok(());
    }

    // Only recipes built into the helper, for apply and for undo.
    let ids: Vec<String> = match request {
        Request::Apply { ids } | Request::Undo { ids } => {
            if let Some(unknown) = ids.iter().find(|id| env.recipe(id).is_none()) {
                emit(Event::Refused {
                    reason: Refusal::UnknownRecipe(unknown.clone()),
                });
                return Ok(());
            }
            ids.clone()
        }
        Request::UndoAll => env.journal.applied_recipe_ids()?,
    };

    if let Err(b) = check_no_games(&env.running, &env.blocked) {
        emit(Event::Blocked { reason: block(&b) });
        return Ok(());
    }

    // The journal lives in the user's own folder, so a program running as the user could
    // edit it. Undo and recovery write what the journal says, with administrator rights:
    // every entry they would use must point at a target of the built-in recipe it names.
    let pending = env.journal.pending_sessions()?;
    let mut to_check = Vec::new();
    for s in &pending {
        to_check.extend(env.journal.entries(s.id)?);
    }
    if !matches!(request, Request::Apply { .. }) {
        for id in &ids {
            to_check.extend(env.journal.applied_entries(id)?);
        }
    }
    if let Some(bad) = to_check.iter().find(|e| !trusted(e, &env.recipes)) {
        emit(Event::Blocked {
            reason: BlockReason::JournalUntrusted {
                recipe: bad.recipe_id.chars().take(MAX_ID_LEN).collect(),
            },
        });
        return Ok(());
    }

    // CLAUDE.md rule 3: whatever a crash left `pending` is undone before anything new.
    if !pending.is_empty() {
        let elevated = env.host.elevated;
        let mut engine = env.engine();
        if engine.recovery_needs_admin()? && !elevated {
            emit(Event::Blocked {
                reason: BlockReason::NeedsAdmin,
            });
            return Ok(());
        }
        let mut errors = Vec::new();
        for s in engine.recover()? {
            if s.errors.is_empty() {
                emit(Event::Recovered {
                    session: s.session.0,
                    restored: s.restored,
                });
            } else {
                errors.extend(s.errors);
            }
        }
        if !errors.is_empty() {
            emit(Event::Blocked {
                reason: BlockReason::RecoveryFailed { errors },
            });
            return Ok(());
        }
    }

    let undo = !matches!(request, Request::Apply { .. });
    let needs_admin = {
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let recipes: Vec<Recipe> = ids
            .iter()
            .filter_map(|id| env.recipe(id).cloned())
            .collect();
        let engine = env.engine();
        if undo {
            engine.undo_needs_admin(&refs)?
        } else {
            let mut needs = false;
            for r in &recipes {
                needs |= plan_needs_admin(&engine.plan(r)?);
            }
            needs
        }
    };
    if needs_admin && !env.host.elevated {
        emit(Event::Blocked {
            reason: BlockReason::NeedsAdmin,
        });
        return Ok(());
    }

    let mut restart = RequiresRestart::None;
    for id in &ids {
        emit(Event::Started { id: id.clone() });
        let outcome = if undo {
            let report = env.engine().undo(&[id.as_str()])?;
            report
                .results
                .into_iter()
                .next()
                .map_or(Outcome::NothingToUndo, |(_, o)| undo_outcome(o))
        } else {
            let Some(recipe) = env.recipe(id).cloned() else {
                continue;
            };
            let report = env.engine().apply(&[&recipe])?;
            report
                .results
                .into_iter()
                .next()
                .map_or(Outcome::AlreadyApplied, |(_, o)| apply_outcome(o))
        };
        if matches!(outcome, Outcome::Applied { .. } | Outcome::Undone { .. }) {
            if let Some(r) = env.recipe(id) {
                if rank(r.requires_restart) > rank(restart) {
                    restart = r.requires_restart;
                }
            }
        }
        emit(Event::Result {
            id: id.clone(),
            outcome,
        });
    }
    emit(Event::Done {
        restart: match restart {
            RequiresRestart::None => Restart::None,
            RequiresRestart::Explorer => Restart::Explorer,
            RequiresRestart::Signout => Restart::Signout,
            RequiresRestart::Reboot => Restart::Reboot,
        },
    });
    Ok(())
}

/// The entry's recipe is built in, its action index exists, and both snapshots point at
/// exactly that action's target (same hive, key and value, or the same service). Keys the
/// engine created must be the action's key or one of its parents.
fn trusted(entry: &Entry, recipes: &[Recipe]) -> bool {
    let Some(action) = recipes
        .iter()
        .find(|r| r.id == entry.recipe_id)
        .and_then(|r| r.actions.get(entry.action_index))
    else {
        return false;
    };
    let (Ok(previous), Ok(new)) = (
        entry.previous::<Snapshot>(),
        entry.target_state::<Snapshot>(),
    ) else {
        return false;
    };
    [previous, new].iter().all(|s| matches_action(s, action))
}

fn matches_action(snapshot: &Snapshot, action: &Action) -> bool {
    match (snapshot, action) {
        (
            Snapshot::Registry {
                hive,
                key,
                value,
                missing_keys,
                ..
            },
            Action::Registry(a),
        ) => {
            *hive == a.hive
                && key.eq_ignore_ascii_case(&a.key)
                && value.eq_ignore_ascii_case(&a.value)
                && missing_keys.iter().all(|k| is_key_or_parent(k, &a.key))
        }
        (Snapshot::Service { name, .. }, Action::Service(a)) => name.eq_ignore_ascii_case(&a.name),
        _ => false,
    }
}

fn is_key_or_parent(candidate: &str, key: &str) -> bool {
    let (c, k) = (candidate.to_ascii_lowercase(), key.to_ascii_lowercase());
    !c.is_empty() && (c == k || k.starts_with(&format!("{c}\\")))
}

/// Admin is needed only if an action that will actually change something needs it.
fn plan_needs_admin(plan: &RecipePlan) -> bool {
    !matches!(plan.status, RecipeStatus::NotApplicable(_))
        && plan
            .actions
            .iter()
            .any(|a| !a.in_place && a.desired.needs_admin())
}

fn rank(r: RequiresRestart) -> u8 {
    match r {
        RequiresRestart::None => 0,
        RequiresRestart::Explorer => 1,
        RequiresRestart::Signout => 2,
        RequiresRestart::Reboot => 3,
    }
}

fn block(b: &Blocked) -> BlockReason {
    match b {
        Blocked::NotWindows11 { build } => BlockReason::NotWindows11 { build: *build },
        Blocked::NotX64 { arch } => BlockReason::NotX64 {
            arch: arch.to_string(),
        },
        Blocked::NeedsAdmin => BlockReason::NeedsAdmin,
        Blocked::GameRunning { running } => BlockReason::GameRunning {
            processes: running.iter().map(|p| p.name.clone()).collect(),
        },
    }
}

fn not_applicable(n: engine::NotApplicable) -> NotApplicableReason {
    match n {
        engine::NotApplicable::ServiceMissing { name } => {
            NotApplicableReason::ServiceMissing { name }
        }
        engine::NotApplicable::Platform(m) => NotApplicableReason::Platform {
            detail: match m {
                PlatformMismatch::BuildTooOld { .. } => "build_too_old",
                PlatformMismatch::BuildTooNew { .. } => "build_too_new",
                PlatformMismatch::Edition { .. } => "edition",
                PlatformMismatch::FormFactor { .. } => "form_factor",
            }
            .into(),
        },
        engine::NotApplicable::Precondition { target, .. } => {
            NotApplicableReason::Precondition { target }
        }
    }
}

fn apply_outcome(o: ApplyOutcome) -> Outcome {
    match o {
        ApplyOutcome::Applied { changed } => Outcome::Applied { changed },
        ApplyOutcome::AlreadyApplied => Outcome::AlreadyApplied,
        ApplyOutcome::NotApplicable(n) => Outcome::NotApplicable {
            reason: not_applicable(n),
        },
        ApplyOutcome::Failed { error, rolled_back } => Outcome::Failed { error, rolled_back },
    }
}

fn undo_outcome(o: UndoOutcome) -> Outcome {
    match o {
        UndoOutcome::Undone { restored } => Outcome::Undone { restored },
        UndoOutcome::NothingToUndo => Outcome::NothingToUndo,
        UndoOutcome::Failed { error, rolled_back } => Outcome::Failed { error, rolled_back },
    }
}
