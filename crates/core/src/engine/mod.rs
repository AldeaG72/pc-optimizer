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
    AppxBackend, AppxState, BackendError, PowerBackend, PowerValues, RegData, RegRead,
    RegistryBackend, ServiceBackend, ServiceStart, ServiceState,
};
use crate::guard::{platform_mismatch, PlatformMismatch, SystemInfo};
use crate::journal::{EntryId, Journal, JournalError, NewEntry, SessionId, SessionKind, Status};
use crate::recipe::{
    Action, Hive, NetAdapterAction, Recipe, RegValue, RegistryAction, RegistryOp, ServiceStartup,
};

/// Driver keys of network adapters (the network device class), one numbered subkey each.
pub const NET_ADAPTER_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}";

/// `Characteristics` bit of a physical adapter (not virtual, not software).
pub const NCF_PHYSICAL: u32 = 0x4;

/// True if `key` is one adapter's driver key: `NET_ADAPTER_CLASS\0001`.
pub fn is_net_adapter_key(key: &str) -> bool {
    let Some((parent, child)) = key.rsplit_once('\\') else {
        return false;
    };
    parent.eq_ignore_ascii_case(NET_ADAPTER_CLASS)
        && child.len() == 4
        && child.bytes().all(|b| b.is_ascii_digit())
}

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
    /// An `only_if` value does not match: the user (or another tool) chose something else.
    Precondition {
        target: String,
        expected: RegValue,
    },
    /// The power plan the recipe switches to does not exist on this PC.
    PowerPlanMissing {
        scheme: String,
    },
    /// The active plan has no such setting on this PC.
    PowerSettingMissing {
        setting: String,
    },
    /// No physical network adapter offers this setting.
    NoNetworkAdapter {
        value: String,
    },
    /// The app is not on this PC.
    AppMissing {
        package: String,
    },
    /// Windows keeps no copy of the app, so removing it could not be undone offline.
    AppNotRestorable {
        package: String,
    },
    /// A cleanup deletes files: it runs on its own, never through apply/undo.
    Cleanup,
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
    /// Optional: without them, recipes of that kind fail with a clear error.
    power: Option<&'a mut dyn PowerBackend>,
    apps: Option<&'a mut dyn AppxBackend>,
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
            power: None,
            apps: None,
        }
    }

    /// Power plans and settings.
    pub fn with_power(mut self, power: &'a mut dyn PowerBackend) -> Self {
        self.power = Some(power);
        self
    }

    /// Preinstalled apps.
    pub fn with_apps(mut self, apps: &'a mut dyn AppxBackend) -> Self {
        self.apps = Some(apps);
        self
    }

    fn power(&self) -> Result<&dyn PowerBackend, BackendError> {
        match self.power.as_deref() {
            Some(p) => Ok(p),
            None => Err(BackendError::Unsupported {
                what: "power plans".into(),
            }),
        }
    }

    fn power_mut(&mut self) -> Result<&mut dyn PowerBackend, BackendError> {
        match self.power.as_deref_mut() {
            Some(p) => Ok(p),
            None => Err(BackendError::Unsupported {
                what: "power plans".into(),
            }),
        }
    }

    fn apps(&self) -> Result<&dyn AppxBackend, BackendError> {
        match self.apps.as_deref() {
            Some(a) => Ok(a),
            None => Err(BackendError::Unsupported {
                what: "preinstalled apps".into(),
            }),
        }
    }

    fn apps_mut(&mut self) -> Result<&mut dyn AppxBackend, BackendError> {
        match self.apps.as_deref_mut() {
            Some(a) => Ok(a),
            None => Err(BackendError::Unsupported {
                what: "preinstalled apps".into(),
            }),
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
        let not_applicable = |reason| RecipePlan {
            recipe_id: recipe.id.clone(),
            status: RecipeStatus::NotApplicable(reason),
            actions: Vec::new(),
        };
        let mut actions = Vec::new();
        for (index, action) in recipe.actions.iter().enumerate() {
            let currents = match self.capture(action)? {
                Captured::Found(s) => s,
                Captured::NotApplicable(reason) => return Ok(not_applicable(reason)),
            };
            // Usually one target; one per adapter for network adapter settings.
            for current in currents {
                let desired = desired_state(action, &current);
                let in_place = desired.is_satisfied_by(&current);
                if !in_place {
                    if let Some(unmet) = unmet_precondition(action, &current) {
                        return Ok(not_applicable(unmet));
                    }
                }
                actions.push(ActionPlan {
                    index,
                    current,
                    desired,
                    in_place,
                });
            }
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
            let currents = match self.capture(action) {
                Ok(Captured::Found(s)) => s,
                Ok(Captured::NotApplicable(_)) => {
                    let rolled_back = self.roll_back(&done)?;
                    return Ok(ApplyOutcome::Failed {
                        error: "a target disappeared while applying".into(),
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
            for mut current in currents {
                let desired = desired_state(action, &current);
                if desired.is_satisfied_by(&current) {
                    continue;
                }
                if unmet_precondition(action, &current).is_some() {
                    // The value changed between the plan and now: leave it alone.
                    let rolled_back = self.roll_back(&done)?;
                    return Ok(ApplyOutcome::Failed {
                        error: format!("{} changed while applying", current.target()),
                        rolled_back,
                    });
                }
                self.inherit_created_keys(&mut current)?;

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
        let one = |s| Ok(Captured::Found(vec![s]));
        let not_applicable = |r| Ok(Captured::NotApplicable(r));
        match action {
            Action::Registry(r) => one(self.capture_registry(r.hive, &r.key, &r.value)?),
            Action::Service(s) => match self.services.query(&s.name)? {
                Some(state) => one(Snapshot::Service {
                    name: s.name.clone(),
                    state,
                }),
                None => not_applicable(NotApplicable::ServiceMissing {
                    name: s.name.clone(),
                }),
            },
            Action::PowerPlan(p) => {
                let power = self.power()?;
                if !power.scheme_exists(&p.scheme)? {
                    return not_applicable(NotApplicable::PowerPlanMissing {
                        scheme: p.scheme.clone(),
                    });
                }
                one(Snapshot::PowerPlan {
                    active: power.active_scheme()?,
                })
            }
            Action::PowerSetting(p) => {
                let power = self.power()?;
                let scheme = power.active_scheme()?;
                match power.read_setting(&scheme, &p.subgroup, &p.setting)? {
                    Some(values) => one(Snapshot::PowerSetting {
                        scheme,
                        subgroup: p.subgroup.clone(),
                        setting: p.setting.clone(),
                        values,
                    }),
                    None => not_applicable(NotApplicable::PowerSettingMissing {
                        setting: p.setting.clone(),
                    }),
                }
            }
            Action::NetAdapter(n) => {
                let found = self.capture_adapters(n)?;
                if found.is_empty() {
                    not_applicable(NotApplicable::NoNetworkAdapter {
                        value: n.value.clone(),
                    })
                } else {
                    Ok(Captured::Found(found))
                }
            }
            Action::Appx(a) => match self.apps()?.query(&a.package)? {
                AppxState::Installed => one(Snapshot::Appx {
                    package: a.package.clone(),
                    installed: true,
                }),
                AppxState::Removed => one(Snapshot::Appx {
                    package: a.package.clone(),
                    installed: false,
                }),
                AppxState::NotRestorable => not_applicable(NotApplicable::AppNotRestorable {
                    package: a.package.clone(),
                }),
                AppxState::Missing => not_applicable(NotApplicable::AppMissing {
                    package: a.package.clone(),
                }),
            },
            Action::Cleanup(_) => not_applicable(NotApplicable::Cleanup),
        }
    }

    /// The value on every physical adapter whose driver offers it (or on every physical
    /// adapter, for values that are not advertised).
    fn capture_adapters(&self, n: &NetAdapterAction) -> Result<Vec<Snapshot>, BackendError> {
        let hive = Hive::LocalMachine;
        let mut found = Vec::new();
        for sub in self.registry.subkeys(hive, NET_ADAPTER_CLASS)? {
            let key = format!("{NET_ADAPTER_CLASS}\\{sub}");
            if !is_net_adapter_key(&key) {
                continue;
            }
            let physical = match self.registry.read(hive, &key, "Characteristics")? {
                RegRead::Present(data) => {
                    matches!(data.to_value(), Some(RegValue::Dword(c)) if c & NCF_PHYSICAL != 0)
                }
                _ => false,
            };
            if !physical {
                continue;
            }
            if n.advertised {
                let params = format!("{key}\\Ndi\\Params\\{}", n.value);
                if self.registry.read(hive, &params, "")? == RegRead::KeyMissing {
                    continue;
                }
            }
            found.push(self.capture_registry(hive, &key, &n.value)?);
        }
        Ok(found)
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
            Snapshot::PowerPlan { .. } => Ok(Snapshot::PowerPlan {
                active: self.power()?.active_scheme()?,
            }),
            Snapshot::PowerSetting {
                scheme,
                subgroup,
                setting,
                ..
            } => match self.power()?.read_setting(scheme, subgroup, setting)? {
                Some(values) => Ok(Snapshot::PowerSetting {
                    scheme: scheme.clone(),
                    subgroup: subgroup.clone(),
                    setting: setting.clone(),
                    values,
                }),
                None => Err(BackendError::NotFound {
                    target: target.target(),
                }),
            },
            Snapshot::Appx { package, .. } => Ok(Snapshot::Appx {
                package: package.clone(),
                installed: matches!(
                    self.apps()?.query(package)?,
                    AppxState::Installed | AppxState::NotRestorable
                ),
            }),
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

    /// Adds to a "not there yet" registry snapshot the keys on its path that the app created
    /// for changes still in place. Whichever recipe is undone last then removes them, so the
    /// order of undos does not matter. (Keys are removed only when empty, so a key still used
    /// by another recipe stays.)
    fn inherit_created_keys(&self, snapshot: &mut Snapshot) -> Result<(), EngineError> {
        let Snapshot::Registry {
            hive,
            key,
            state: RegRead::KeyMissing | RegRead::ValueMissing,
            missing_keys,
            ..
        } = snapshot
        else {
            return Ok(());
        };
        let target = key.to_lowercase();
        for entry in self.journal.all_applied_entries()? {
            let Snapshot::Registry {
                hive: their_hive,
                missing_keys: theirs,
                ..
            } = entry.previous::<Snapshot>()?
            else {
                continue;
            };
            if their_hive != *hive {
                continue;
            }
            for k in theirs {
                let lower = k.to_lowercase();
                let on_path = target == lower || target.starts_with(&format!("{lower}\\"));
                if on_path && !missing_keys.iter().any(|m| m.eq_ignore_ascii_case(&k)) {
                    missing_keys.push(k);
                }
            }
        }
        // Outermost first, as everywhere else.
        missing_keys.sort_by_key(|k| k.split('\\').count());
        Ok(())
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
            (Snapshot::PowerPlan { active }, Snapshot::PowerPlan { active: now })
                if !now.eq_ignore_ascii_case(active) =>
            {
                self.power_mut()?.set_active_scheme(active)?;
            }
            (
                Snapshot::PowerSetting {
                    scheme,
                    subgroup,
                    setting,
                    values,
                },
                Snapshot::PowerSetting { values: now, .. },
            ) if now != values => {
                self.power_mut()?
                    .write_setting(scheme, subgroup, setting, *values)?;
            }
            (Snapshot::Appx { package, installed }, Snapshot::Appx { installed: now, .. })
                if now != installed =>
            {
                self.apps_mut()?.set_installed(package, *installed)?;
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
    /// One snapshot per target (several for network adapter settings).
    Found(Vec<Snapshot>),
    NotApplicable(NotApplicable),
}

/// The action's `only_if` value, if the target does not currently hold it.
fn unmet_precondition(action: &Action, current: &Snapshot) -> Option<NotApplicable> {
    let Action::Registry(RegistryAction {
        only_if: Some(expected),
        ..
    }) = action
    else {
        return None;
    };
    let holds = matches!(
        current,
        Snapshot::Registry { state: RegRead::Present(data), .. } if data.matches(expected)
    );
    (!holds).then(|| NotApplicable::Precondition {
        target: current.target(),
        expected: expected.clone(),
    })
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
                _ => false,
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
        Action::PowerPlan(p) => Snapshot::PowerPlan {
            active: p.scheme.clone(),
        },
        Action::PowerSetting(p) => match current {
            Snapshot::PowerSetting {
                scheme,
                subgroup,
                setting,
                values,
            } => Snapshot::PowerSetting {
                scheme: scheme.clone(),
                subgroup: subgroup.clone(),
                setting: setting.clone(),
                values: PowerValues {
                    ac: p.ac.unwrap_or(values.ac),
                    dc: p.dc.unwrap_or(values.dc),
                },
            },
            // Never happens: the capture of a power setting is a power setting.
            other => other.clone(),
        },
        Action::NetAdapter(n) => match current {
            Snapshot::Registry {
                hive, key, value, ..
            } => Snapshot::Registry {
                hive: *hive,
                key: key.clone(),
                value: value.clone(),
                state: RegRead::Present(RegData::from_value(&n.set)),
                missing_keys: Vec::new(),
            },
            other => other.clone(),
        },
        Action::Appx(a) => Snapshot::Appx {
            package: a.package.clone(),
            installed: false,
        },
        // Never planned (see `NotApplicable::Cleanup`).
        Action::Cleanup(_) => current.clone(),
    }
}
