//! The engine: `plan` → `apply` → `undo`, plus `recover` after an interruption.
//!
//! Rules (CLAUDE.md, safety rules 2 and 3):
//! - nothing is written before its exact previous state is in the journal as `pending`;
//! - after every write the target is read back and compared;
//! - if one action of a recipe fails, the recipe's earlier actions are undone and the other
//!   recipes continue;
//! - a session left `pending` (crash, power cut) is undone by `recover` before anything else
//!   may run.
//!
//! Journal errors are fatal: the engine stops at once, and the session stays `pending` so
//! that the next `recover` cleans up. Backend errors only fail the recipe concerned.

mod snapshot;

use std::fmt;

pub use snapshot::Snapshot;

use crate::backend::{
    BackendError, RegData, RegRead, RegistryBackend, ServiceBackend, ServiceStart, ServiceState,
};
use crate::guard::{platform_mismatch, PlatformMismatch, SystemInfo};
use crate::journal::{EntryId, Journal, JournalError, NewEntry, SessionId, SessionKind, Status};
use crate::recipe::{Action, Hive, Recipe, RegistryOp, ServiceStartup};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Journal(#[from] JournalError),

    #[error(transparent)]
    Backend(#[from] BackendError),

    /// An earlier run was interrupted; `recover` must run first.
    #[error("an earlier session was interrupted and must be undone first (sessions {sessions:?})")]
    PendingRecovery { sessions: Vec<SessionId> },
}

/// Why one action failed. Never fatal for the whole run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    Backend(BackendError),
    /// The target was read back after the change and is not in the expected state.
    Mismatch {
        target: String,
        expected: Box<Snapshot>,
        found: Box<Snapshot>,
    },
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionError::Backend(e) => e.fmt(f),
            ActionError::Mismatch { target, .. } => {
                write!(f, "{target} is not in the expected state after the change")
            }
        }
    }
}

impl From<BackendError> for ActionError {
    fn from(e: BackendError) -> Self {
        ActionError::Backend(e)
    }
}

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotApplicable {
    ServiceMissing {
        name: String,
    },
    /// The recipe's `platform` block excludes this PC.
    Platform(PlatformMismatch),
}

/// Where a recipe stands on this PC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeStatus {
    /// Not in place; the app has no changes recorded for it.
    NotApplied,
    /// Applied by this app and still in place.
    Applied,
    /// Already in place, but not by this app (Windows default or another tool).
    AlreadyInPlace,
    /// Applied by this app, then changed by something else.
    ModifiedOutside,
    NotApplicable(NotApplicable),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionPlan {
    pub index: usize,
    pub current: Snapshot,
    pub desired: Snapshot,
    pub in_place: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipePlan {
    pub recipe_id: String,
    pub status: RecipeStatus,
    /// Empty when the recipe is not applicable.
    pub actions: Vec<ActionPlan>,
}

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied {
        changed: usize,
    },
    AlreadyApplied,
    NotApplicable(NotApplicable),
    /// Nothing of the recipe remains changed if `rolled_back` is true.
    Failed {
        error: String,
        rolled_back: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoOutcome {
    Undone {
        restored: usize,
    },
    NothingToUndo,
    /// The recipe is still applied if `rolled_back` is true.
    Failed {
        error: String,
        rolled_back: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report<O> {
    /// `None` if nothing had to change.
    pub session: Option<SessionId>,
    pub results: Vec<(String, O)>,
}

pub type ApplyReport = Report<ApplyOutcome>;
pub type UndoReport = Report<UndoOutcome>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredSession {
    pub session: SessionId,
    pub kind: SessionKind,
    pub restored: usize,
    /// Empty if the session was fully undone; otherwise it stays `pending`.
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

pub struct Engine<'a> {
    registry: &'a mut dyn RegistryBackend,
    services: &'a mut dyn ServiceBackend,
    journal: &'a mut Journal,
    system: SystemInfo,
}

impl<'a> Engine<'a> {
    /// `system` is compared with each recipe's `platform` block.
    pub fn new(
        registry: &'a mut dyn RegistryBackend,
        services: &'a mut dyn ServiceBackend,
        journal: &'a mut Journal,
        system: SystemInfo,
    ) -> Self {
        Self {
            registry,
            services,
            journal,
            system,
        }
    }

    /// Whether undoing these recipes touches HKLM or services.
    pub fn undo_needs_admin(&self, recipe_ids: &[&str]) -> Result<bool, EngineError> {
        for id in recipe_ids {
            for entry in self.journal.applied_entries(id)? {
                if entry.previous::<Snapshot>()?.needs_admin() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Whether `recover` will touch HKLM or services.
    pub fn recovery_needs_admin(&self) -> Result<bool, EngineError> {
        for session in self.journal.pending_sessions()? {
            for entry in self.journal.entries(session.id)? {
                if matches!(entry.status, Status::Pending | Status::Applied)
                    && entry.previous::<Snapshot>()?.needs_admin()
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Current → desired state for every action, without changing anything.
    pub fn plan(&self, recipe: &Recipe) -> Result<RecipePlan, EngineError> {
        if let Some(mismatch) = platform_mismatch(&recipe.platform, &self.system) {
            return Ok(RecipePlan {
                recipe_id: recipe.id.clone(),
                status: RecipeStatus::NotApplicable(NotApplicable::Platform(mismatch)),
                actions: Vec::new(),
            });
        }
        let mut actions = Vec::new();
        for (index, action) in recipe.actions.iter().enumerate() {
            let current = match self.capture(action)? {
                Captured::Found(s) => s,
                Captured::NotApplicable(reason) => {
                    return Ok(RecipePlan {
                        recipe_id: recipe.id.clone(),
                        status: RecipeStatus::NotApplicable(reason),
                        actions: Vec::new(),
                    })
                }
            };
            let desired = desired_state(action, &current);
            let in_place = desired.is_satisfied_by(&current);
            actions.push(ActionPlan {
                index,
                current,
                desired,
                in_place,
            });
        }

        let all_in_place = actions.iter().all(|a| a.in_place);
        let by_app = !self.journal.applied_entries(&recipe.id)?.is_empty();
        let status = match (by_app, all_in_place) {
            (true, true) => RecipeStatus::Applied,
            (true, false) => RecipeStatus::ModifiedOutside,
            (false, true) => RecipeStatus::AlreadyInPlace,
            (false, false) => RecipeStatus::NotApplied,
        };
        Ok(RecipePlan {
            recipe_id: recipe.id.clone(),
            status,
            actions,
        })
    }

    pub fn apply(&mut self, recipes: &[&Recipe]) -> Result<ApplyReport, EngineError> {
        self.ensure_nothing_pending()?;
        let mut session = None;
        let mut results = Vec::new();
        for recipe in recipes {
            let outcome = self.apply_recipe(recipe, &mut session)?;
            results.push((recipe.id.clone(), outcome));
        }
        if let Some(s) = session {
            let any_applied = results
                .iter()
                .any(|(_, o)| matches!(o, ApplyOutcome::Applied { .. }));
            let status = if any_applied {
                Status::Applied
            } else {
                Status::Failed
            };
            self.journal.finish_session(s, status)?;
        }
        Ok(Report { session, results })
    }

    /// Restores the state saved before the app changed each recipe.
    pub fn undo(&mut self, recipe_ids: &[&str]) -> Result<UndoReport, EngineError> {
        self.ensure_nothing_pending()?;
        let mut session = None;
        let mut results = Vec::new();
        let mut originals = Vec::new();
        for id in recipe_ids {
            let outcome = self.undo_recipe(id, &mut session, &mut originals)?;
            results.push(((*id).to_owned(), outcome));
        }
        if let Some(s) = session {
            let any_undone = results
                .iter()
                .any(|(_, o)| matches!(o, UndoOutcome::Undone { .. }));
            let status = if any_undone {
                Status::Applied
            } else {
                Status::Failed
            };
            self.journal
                .finish_session_rolling_back(s, status, &originals)?;
        }
        Ok(Report { session, results })
    }

    /// Undoes every recipe the app has changed.
    pub fn undo_all(&mut self) -> Result<UndoReport, EngineError> {
        let ids = self.journal.applied_recipe_ids()?;
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        self.undo(&refs)
    }

    /// Undoes every session left `pending` by an interruption, returning the system to the
    /// state before that session started.
    pub fn recover(&mut self) -> Result<Vec<RecoveredSession>, EngineError> {
        let mut report = Vec::new();
        for session in self.journal.pending_sessions()? {
            let entries = self.journal.entries(session.id)?;
            let mut restored = 0;
            let mut errors = Vec::new();
            for entry in entries
                .iter()
                .rev()
                .filter(|e| matches!(e.status, Status::Pending | Status::Applied))
            {
                let previous: Snapshot = entry.previous()?;
                match self.restore(&previous) {
                    Ok(()) => {
                        self.journal
                            .set_entry_status(entry.id, Status::RolledBack, None)?;
                        restored += 1;
                    }
                    Err(e) => {
                        let msg = format!("{}: {e}", entry.target);
                        self.journal.set_entry_error(entry.id, &msg)?;
                        errors.push(msg);
                    }
                }
            }
            if errors.is_empty() {
                self.journal
                    .finish_session(session.id, Status::RolledBack)?;
            }
            report.push(RecoveredSession {
                session: session.id,
                kind: session.kind,
                restored,
                errors,
            });
        }
        Ok(report)
    }

    // -- apply / undo of one recipe ------------------------------------------

    fn apply_recipe(
        &mut self,
        recipe: &Recipe,
        session: &mut Option<SessionId>,
    ) -> Result<ApplyOutcome, EngineError> {
        let plan = match self.plan(recipe) {
            Ok(plan) => plan,
            Err(EngineError::Backend(e)) => {
                return Ok(ApplyOutcome::Failed {
                    error: e.to_string(),
                    rolled_back: true,
                })
            }
            Err(e) => return Err(e),
        };
        if let RecipeStatus::NotApplicable(reason) = plan.status {
            return Ok(ApplyOutcome::NotApplicable(reason));
        }
        if plan.actions.iter().all(|a| a.in_place) {
            return Ok(ApplyOutcome::AlreadyApplied);
        }

        let sid = self.session(session, SessionKind::Apply)?;
        let mut done: Vec<(EntryId, Snapshot)> = Vec::new();
        for (index, action) in recipe.actions.iter().enumerate() {
            // Capture again right before writing: the plan may be stale, and two actions may
            // touch the same target.
            let current = match self.capture(action) {
                Ok(Captured::Found(s)) => s,
                Ok(Captured::NotApplicable(_)) => {
                    let rolled_back = self.roll_back(&done)?;
                    return Ok(ApplyOutcome::Failed {
                        error: "a service disappeared while applying".into(),
                        rolled_back,
                    });
                }
                Err(e) => {
                    let rolled_back = self.roll_back(&done)?;
                    return Ok(ApplyOutcome::Failed {
                        error: e.to_string(),
                        rolled_back,
                    });
                }
            };
            let desired = desired_state(action, &current);
            if desired.is_satisfied_by(&current) {
                continue;
            }

            let entry = self.journal.record_pending(
                sid,
                &NewEntry {
                    recipe_id: &recipe.id,
                    recipe_version: recipe.version,
                    action_index: index,
                    kind: current.kind(),
                    target: &current.target(),
                    previous_state: &current,
                    new_state: &desired,
                },
            )?;

            match self.restore(&desired) {
                Ok(()) => {
                    self.journal
                        .set_entry_status(entry, Status::Applied, None)?;
                    done.push((entry, current));
                }
                Err(e) => {
                    let error = e.to_string();
                    let this_restored = self.undo_failed_action(entry, &current, &error)?;
                    let others_restored = self.roll_back(&done)?;
                    return Ok(ApplyOutcome::Failed {
                        error,
                        rolled_back: this_restored && others_restored,
                    });
                }
            }
        }
        Ok(ApplyOutcome::Applied {
            changed: done.len(),
        })
    }

    fn undo_recipe(
        &mut self,
        recipe_id: &str,
        session: &mut Option<SessionId>,
        originals: &mut Vec<EntryId>,
    ) -> Result<UndoOutcome, EngineError> {
        let entries = self.journal.applied_entries(recipe_id)?;
        if entries.is_empty() {
            return Ok(UndoOutcome::NothingToUndo);
        }
        let sid = self.session(session, SessionKind::Undo)?;
        let mut done: Vec<(EntryId, Snapshot)> = Vec::new();

        // Newest first, so a value re-applied after an outside change ends at the original.
        for (step, original) in entries.iter().rev().enumerate() {
            let target: Snapshot = original.previous()?;
            let current = match self.capture_snapshot_target(&target) {
                Ok(s) => s,
                Err(e) => {
                    let rolled_back = self.roll_back(&done)?;
                    return Ok(UndoOutcome::Failed {
                        error: e.to_string(),
                        rolled_back,
                    });
                }
            };
            let entry = self.journal.record_pending(
                sid,
                &NewEntry {
                    recipe_id,
                    recipe_version: original.recipe_version,
                    // In undo sessions this is the restore step, unique within the recipe.
                    action_index: step,
                    kind: target.kind(),
                    target: &target.target(),
                    previous_state: &current,
                    new_state: &target,
                },
            )?;
            match self.restore(&target) {
                Ok(()) => {
                    self.journal
                        .set_entry_status(entry, Status::Applied, None)?;
                    done.push((entry, current));
                }
                Err(e) => {
                    let error = e.to_string();
                    let this_restored = self.undo_failed_action(entry, &current, &error)?;
                    let others_restored = self.roll_back(&done)?;
                    return Ok(UndoOutcome::Failed {
                        error,
                        rolled_back: this_restored && others_restored,
                    });
                }
            }
        }
        // Marked `rolled_back` only together with the end of the session (atomically).
        originals.extend(entries.iter().map(|e| e.id));
        Ok(UndoOutcome::Undone {
            restored: done.len(),
        })
    }

    /// After a failed write: restore the target and record the outcome on its entry.
    /// Returns whether the target is back to its previous state.
    fn undo_failed_action(
        &mut self,
        entry: EntryId,
        previous: &Snapshot,
        error: &str,
    ) -> Result<bool, EngineError> {
        match self.restore(previous) {
            Ok(()) => {
                self.journal
                    .set_entry_status(entry, Status::Failed, Some(error))?;
                Ok(true)
            }
            Err(e) => {
                // The change may be in place: keep it undoable later.
                let note = format!("{error}; restoring the previous state also failed: {e}");
                self.journal
                    .set_entry_status(entry, Status::Applied, Some(&note))?;
                Ok(false)
            }
        }
    }

    /// Restores earlier actions of the same recipe, newest first.
    fn roll_back(&mut self, done: &[(EntryId, Snapshot)]) -> Result<bool, EngineError> {
        let mut all_restored = true;
        for (entry, previous) in done.iter().rev() {
            match self.restore(previous) {
                Ok(()) => self
                    .journal
                    .set_entry_status(*entry, Status::RolledBack, None)?,
                Err(e) => {
                    all_restored = false;
                    self.journal
                        .set_entry_error(*entry, &format!("rollback failed: {e}"))?;
                }
            }
        }
        Ok(all_restored)
    }

    fn session(
        &mut self,
        session: &mut Option<SessionId>,
        kind: SessionKind,
    ) -> Result<SessionId, EngineError> {
        if let Some(s) = *session {
            return Ok(s);
        }
        let s = self.journal.begin_session(kind)?;
        *session = Some(s);
        Ok(s)
    }

    fn ensure_nothing_pending(&self) -> Result<(), EngineError> {
        let pending = self.journal.pending_sessions()?;
        if pending.is_empty() {
            Ok(())
        } else {
            Err(EngineError::PendingRecovery {
                sessions: pending.iter().map(|s| s.id).collect(),
            })
        }
    }

    // -- reading and writing targets -------------------------------------------

    fn capture(&self, action: &Action) -> Result<Captured, BackendError> {
        match action {
            Action::Registry(r) => Ok(Captured::Found(
                self.capture_registry(r.hive, &r.key, &r.value)?,
            )),
            Action::Service(s) => Ok(match self.services.query(&s.name)? {
                Some(state) => Captured::Found(Snapshot::Service {
                    name: s.name.clone(),
                    state,
                }),
                None => Captured::NotApplicable(NotApplicable::ServiceMissing {
                    name: s.name.clone(),
                }),
            }),
        }
    }

    /// Fresh capture of the same target a snapshot describes.
    fn capture_snapshot_target(&self, target: &Snapshot) -> Result<Snapshot, BackendError> {
        match target {
            Snapshot::Registry {
                hive, key, value, ..
            } => self.capture_registry(*hive, key, value),
            Snapshot::Service { name, .. } => match self.services.query(name)? {
                Some(state) => Ok(Snapshot::Service {
                    name: name.clone(),
                    state,
                }),
                None => Err(BackendError::ServiceNotFound { name: name.clone() }),
            },
        }
    }

    fn capture_registry(
        &self,
        hive: Hive,
        key: &str,
        value: &str,
    ) -> Result<Snapshot, BackendError> {
        let state = self.registry.read(hive, key, value)?;
        let missing_keys = if state == RegRead::KeyMissing {
            self.missing_keys(hive, key)?
        } else {
            Vec::new()
        };
        Ok(Snapshot::Registry {
            hive,
            key: key.to_owned(),
            value: value.to_owned(),
            state,
            missing_keys,
        })
    }

    /// The keys of `key`'s path that do not exist yet, outermost first.
    fn missing_keys(&self, hive: Hive, key: &str) -> Result<Vec<String>, BackendError> {
        let parts: Vec<&str> = key.split('\\').collect();
        for end in 1..=parts.len() {
            // Reading the default ("") value tells whether the key itself exists.
            if self.registry.read(hive, &parts[..end].join("\\"), "")? == RegRead::KeyMissing {
                return Ok((end..=parts.len()).map(|e| parts[..e].join("\\")).collect());
            }
        }
        Ok(Vec::new())
    }

    /// Makes the target match `snapshot`, then reads it back to confirm.
    ///
    /// Writes only what differs: a value that is already exactly right is not rewritten, and
    /// a value that is already missing is not deleted again. That keeps rollback working
    /// even when the original write never happened.
    fn restore(&mut self, snapshot: &Snapshot) -> Result<(), ActionError> {
        let before = self.capture_snapshot_target(snapshot)?;
        match (snapshot, &before) {
            (
                Snapshot::Registry {
                    hive,
                    key,
                    value,
                    state,
                    missing_keys,
                },
                Snapshot::Registry { state: now, .. },
            ) => match state {
                RegRead::Present(data) => {
                    // Exact bytes, not just "means the same": undo restores the original.
                    if now != state {
                        self.registry.write(*hive, key, value, data)?;
                    }
                }
                RegRead::KeyMissing | RegRead::ValueMissing => {
                    if matches!(now, RegRead::Present(_)) {
                        self.registry.delete_value(*hive, key, value)?;
                    }
                    for k in missing_keys.iter().rev() {
                        // Keeps keys that something else has filled in the meantime.
                        self.registry.delete_key_if_empty(*hive, k)?;
                    }
                }
            },
            (Snapshot::Service { name, state }, Snapshot::Service { state: now, .. })
                if now != state =>
            {
                self.services.set(name, *state)?;
            }
            // Already right, or (never happens) a snapshot of a different kind.
            _ => {}
        }

        let found = self.capture_snapshot_target(snapshot)?;
        if snapshot.is_satisfied_by(&found) {
            Ok(())
        } else {
            Err(ActionError::Mismatch {
                target: snapshot.target(),
                expected: Box::new(snapshot.clone()),
                found: Box::new(found),
            })
        }
    }
}

enum Captured {
    Found(Snapshot),
    NotApplicable(NotApplicable),
}

/// What the target should look like after the action, given its current state.
fn desired_state(action: &Action, current: &Snapshot) -> Snapshot {
    match action {
        Action::Registry(r) => Snapshot::Registry {
            hive: r.hive,
            key: r.key.clone(),
            value: r.value.clone(),
            state: match &r.op {
                RegistryOp::Set(v) => RegRead::Present(RegData::from_value(v)),
                RegistryOp::Delete => RegRead::ValueMissing,
            },
            missing_keys: Vec::new(),
        },
        Action::Service(s) => {
            let delayed_now = match current {
                Snapshot::Service { state, .. } => state.delayed_auto_start,
                Snapshot::Registry { .. } => false,
            };
            let (start, delayed_auto_start) = match s.startup {
                // "automatic" means plain automatic, not delayed.
                ServiceStartup::Automatic => (ServiceStart::Automatic, false),
                // Windows ignores the delayed flag here; keep it as it is.
                ServiceStartup::Manual => (ServiceStart::Manual, delayed_now),
                ServiceStartup::Disabled => (ServiceStart::Disabled, delayed_now),
            };
            Snapshot::Service {
                name: s.name.clone(),
                state: ServiceState {
                    start,
                    delayed_auto_start,
                },
            }
        }
    }
}
