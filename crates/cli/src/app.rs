//! What each `pco` command does. Works on trait objects so tests can drive it with
//! in-memory backends; `main.rs` plugs in the real Windows ones.

use std::io::Write;

use anyhow::Result;
use pco_core::backend::{RegistryBackend, ServiceBackend};
use pco_core::engine::{ApplyOutcome, Engine, RecipePlan, RecipeStatus, UndoOutcome};
use pco_core::guard::{
    check_admin, check_host, check_no_games, Blocked, BlockedProcesses, Host, SystemInfo,
};
use pco_core::journal::{Journal, SessionKind, Status};
use pco_core::recipe::{Recipe, RequiresRestart};

use crate::args::Cmd;
use crate::format;
use crate::i18n::Texts;

/// Everything a command works with.
pub struct Env<'a> {
    pub registry: &'a mut dyn RegistryBackend,
    pub services: &'a mut dyn ServiceBackend,
    pub journal: &'a mut Journal,
    pub host: Host,
    /// Executable names of the running processes.
    pub running: Vec<String>,
    pub blocked: BlockedProcesses,
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

/// Process exit codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Ok = 0,
    Error = 1,
    /// A guard stopped the command; nothing was changed.
    Blocked = 2,
    /// Some recipes failed; the rest were done.
    Partial = 3,
}

pub fn run(env: &mut Env<'_>, t: &Texts, cmd: &Cmd, out: &mut dyn Write) -> Exit {
    match run_inner(env, t, cmd, out) {
        Ok(exit) => exit,
        Err(e) => {
            let _ = writeln!(
                out,
                "{}",
                t.f("error.generic", &[("details", &format!("{e:#}"))])
            );
            Exit::Error
        }
    }
}

fn run_inner(env: &mut Env<'_>, t: &Texts, cmd: &Cmd, out: &mut dyn Write) -> Result<Exit> {
    if let Err(b) = check_host(&env.host) {
        writeln!(out, "{}", format::blocked(t, &b))?;
        return Ok(Exit::Blocked);
    }

    // Unknown ids are an error before anything else happens. `undo` accepts any id the
    // journal knows, so it is checked there instead.
    if let Cmd::Plan { ids } | Cmd::Apply { ids, .. } = cmd {
        for id in ids {
            if env.recipe(id).is_none() {
                writeln!(out, "{}", t.f("error.unknown_recipe", &[("id", id)]))?;
                return Ok(Exit::Error);
            }
        }
    }

    match recover(env, t, out)? {
        Recovery::Clean => {}
        Recovery::Blocked | Recovery::Failed if !cmd.writes() => {
            writeln!(out, "{}", t.t("recovery.pending_warning"))?;
        }
        Recovery::Blocked => return Ok(Exit::Blocked),
        Recovery::Failed => return Ok(Exit::Error),
    }

    match cmd {
        Cmd::List => list(env, t, out),
        Cmd::Plan { ids } => {
            print_plans(env, t, ids, out)?;
            Ok(Exit::Ok)
        }
        Cmd::Apply { ids, dry_run: true } => {
            print_plans(env, t, ids, out)?;
            writeln!(out, "{}", t.t("plan.dry_run"))?;
            Ok(Exit::Ok)
        }
        Cmd::Apply {
            ids,
            dry_run: false,
        } => apply(env, t, ids, out),
        Cmd::Undo { ids, all } => undo(env, t, ids, *all, out),
        Cmd::Status => status(env, t, out),
    }
}

enum Recovery {
    Clean,
    Blocked,
    Failed,
}

/// CLAUDE.md rule 3 / ETAPA-0 0.6: a session left `pending` is undone at startup — under
/// the same guards as any other change.
fn recover(env: &mut Env<'_>, t: &Texts, out: &mut dyn Write) -> Result<Recovery> {
    if env.journal.pending_sessions()?.is_empty() {
        return Ok(Recovery::Clean);
    }
    let games = env.blocked.find_running(&env.running);
    if !games.is_empty() {
        let list = format::process_list(&games.into_iter().cloned().collect::<Vec<_>>());
        writeln!(out, "{}", t.f("recovery.game_running", &[("list", &list)]))?;
        return Ok(Recovery::Blocked);
    }
    let elevated = env.host.elevated;
    let mut engine = env.engine();
    if engine.recovery_needs_admin()? && !elevated {
        writeln!(out, "{}", t.t("recovery.needs_admin"))?;
        return Ok(Recovery::Blocked);
    }
    let mut failed = false;
    for s in engine.recover()? {
        if s.errors.is_empty() {
            writeln!(
                out,
                "{}",
                t.f(
                    "recovery.done",
                    &[("session", &s.session.0), ("count", &s.restored)]
                )
            )?;
        } else {
            failed = true;
            writeln!(
                out,
                "{}",
                t.f(
                    "recovery.failed",
                    &[("session", &s.session.0), ("errors", &s.errors.join("; "))]
                )
            )?;
        }
    }
    Ok(if failed {
        Recovery::Failed
    } else {
        Recovery::Clean
    })
}

fn list(env: &mut Env<'_>, t: &Texts, out: &mut dyn Write) -> Result<Exit> {
    let rows: Vec<[String; 4]> = {
        let recipes = env.recipes.clone();
        let engine = env.engine();
        recipes
            .iter()
            .map(|r| {
                let status = match engine.plan(r) {
                    Ok(p) => format::recipe_status(t, &p.status),
                    Err(e) => t.f("status.error", &[("details", &e)]),
                };
                [
                    r.id.clone(),
                    format::category(r.category).to_owned(),
                    format::risk(t, r.risk).to_owned(),
                    status,
                ]
            })
            .collect()
    };
    let header = [
        t.t("list.col.id").to_owned(),
        t.t("list.col.category").to_owned(),
        t.t("list.col.risk").to_owned(),
        t.t("list.col.status").to_owned(),
    ];
    let width = |col: usize| {
        rows.iter()
            .chain(std::iter::once(&header))
            .map(|r| r[col].chars().count())
            .max()
            .unwrap_or(0)
    };
    let (w0, w1, w2) = (width(0), width(1), width(2));
    for row in std::iter::once(&header).chain(rows.iter()) {
        writeln!(
            out,
            "{:<w0$}  {:<w1$}  {:<w2$}  {}",
            row[0], row[1], row[2], row[3]
        )?;
    }
    Ok(Exit::Ok)
}

fn print_plans(env: &mut Env<'_>, t: &Texts, ids: &[String], out: &mut dyn Write) -> Result<()> {
    let recipes: Vec<Recipe> = ids
        .iter()
        .filter_map(|id| env.recipe(id).cloned())
        .collect();
    let engine = env.engine();
    for (i, r) in recipes.iter().enumerate() {
        if i > 0 {
            writeln!(out)?;
        }
        let plan = engine.plan(r)?;
        writeln!(
            out,
            "{}",
            t.f(
                "plan.title",
                &[("id", &r.id), ("name", &r.name.get(t.lang()))]
            )
        )?;
        writeln!(
            out,
            "{}",
            t.f(
                "plan.status",
                &[("status", &format::recipe_status(t, &plan.status))]
            )
        )?;
        for a in &plan.actions {
            let target = a.current.target();
            let old = format::state(t, &a.current);
            if a.in_place {
                writeln!(
                    out,
                    "{}",
                    t.f("plan.in_place", &[("target", &target), ("old", &old)])
                )?;
            } else {
                let new = format::state(t, &a.desired);
                writeln!(
                    out,
                    "{}",
                    t.f(
                        "plan.line",
                        &[("target", &target), ("old", &old), ("new", &new)]
                    )
                )?;
            }
        }
    }
    Ok(())
}

/// Stops with a message if a game or anti-cheat is running.
fn games_guard(env: &Env<'_>, t: &Texts, out: &mut dyn Write) -> Result<bool> {
    if let Err(b) = check_no_games(&env.running, &env.blocked) {
        writeln!(out, "{}", format::blocked(t, &b))?;
        return Ok(false);
    }
    Ok(true)
}

fn admin_guard(env: &Env<'_>, t: &Texts, needs: bool, out: &mut dyn Write) -> Result<bool> {
    if let Err(b @ Blocked::NeedsAdmin) = check_admin(&env.host, needs) {
        writeln!(out, "{}", format::blocked(t, &b))?;
        return Ok(false);
    }
    Ok(true)
}

/// Admin is needed only if an action that will actually change something needs it.
fn plan_needs_admin(plan: &RecipePlan) -> bool {
    !matches!(plan.status, RecipeStatus::NotApplicable(_))
        && plan
            .actions
            .iter()
            .any(|a| !a.in_place && a.desired.needs_admin())
}

fn apply(env: &mut Env<'_>, t: &Texts, ids: &[String], out: &mut dyn Write) -> Result<Exit> {
    if !games_guard(env, t, out)? {
        return Ok(Exit::Blocked);
    }
    let recipes: Vec<Recipe> = ids
        .iter()
        .filter_map(|id| env.recipe(id).cloned())
        .collect();
    let needs_admin = {
        let engine = env.engine();
        let mut needs = false;
        for r in &recipes {
            needs |= plan_needs_admin(&engine.plan(r)?);
        }
        needs
    };
    if !admin_guard(env, t, needs_admin, out)? {
        return Ok(Exit::Blocked);
    }

    let refs: Vec<&Recipe> = recipes.iter().collect();
    let report = env.engine().apply(&refs)?;
    let mut exit = Exit::Ok;
    let mut restart = RequiresRestart::None;
    for (id, outcome) in &report.results {
        let line = match outcome {
            ApplyOutcome::Applied { changed } => {
                if let Some(r) = env.recipe(id) {
                    if format::restart_rank(r.requires_restart) > format::restart_rank(restart) {
                        restart = r.requires_restart;
                    }
                }
                t.f("apply.applied", &[("id", id), ("count", changed)])
            }
            ApplyOutcome::AlreadyApplied => t.f("apply.already", &[("id", id)]),
            ApplyOutcome::NotApplicable(n) => t.f(
                "apply.not_applicable",
                &[("id", id), ("reason", &format::not_applicable(t, n))],
            ),
            ApplyOutcome::Failed { error, rolled_back } => {
                exit = Exit::Partial;
                let key = if *rolled_back {
                    "apply.failed"
                } else {
                    "apply.failed_dirty"
                };
                t.f(key, &[("id", id), ("error", error)])
            }
        };
        writeln!(out, "{line}")?;
    }
    if let Some(what) = format::restart(t, restart) {
        writeln!(out, "{}", t.f("apply.restart", &[("what", &what)]))?;
    }
    Ok(exit)
}

fn undo(
    env: &mut Env<'_>,
    t: &Texts,
    ids: &[String],
    all: bool,
    out: &mut dyn Write,
) -> Result<Exit> {
    let ids: Vec<String> = if all {
        env.journal.applied_recipe_ids()?
    } else {
        ids.to_vec()
    };
    if ids.is_empty() {
        writeln!(out, "{}", t.t("undo.none"))?;
        return Ok(Exit::Ok);
    }
    if !games_guard(env, t, out)? {
        return Ok(Exit::Blocked);
    }
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let needs_admin = env.engine().undo_needs_admin(&refs)?;
    if !admin_guard(env, t, needs_admin, out)? {
        return Ok(Exit::Blocked);
    }

    let report = env.engine().undo(&refs)?;
    let mut exit = Exit::Ok;
    let mut restart = RequiresRestart::None;
    for (id, outcome) in &report.results {
        let line = match outcome {
            UndoOutcome::Undone { restored } => {
                if let Some(r) = env.recipe(id) {
                    if format::restart_rank(r.requires_restart) > format::restart_rank(restart) {
                        restart = r.requires_restart;
                    }
                }
                t.f("undo.undone", &[("id", id), ("count", restored)])
            }
            UndoOutcome::NothingToUndo => t.f("undo.nothing", &[("id", id)]),
            UndoOutcome::Failed { error, rolled_back } => {
                exit = Exit::Partial;
                let key = if *rolled_back {
                    "undo.failed"
                } else {
                    "undo.failed_dirty"
                };
                t.f(key, &[("id", id), ("error", error)])
            }
        };
        writeln!(out, "{line}")?;
    }
    if let Some(what) = format::restart(t, restart) {
        writeln!(out, "{}", t.f("apply.restart", &[("what", &what)]))?;
    }
    Ok(exit)
}

fn status(env: &mut Env<'_>, t: &Texts, out: &mut dyn Write) -> Result<Exit> {
    let sessions = env.journal.sessions()?;
    if sessions.is_empty() {
        writeln!(out, "{}", t.t("status.no_sessions"))?;
    } else {
        writeln!(out, "{}", t.t("status.sessions"))?;
        for s in &sessions {
            let kind = t.t(match s.kind {
                SessionKind::Apply => "kind.apply",
                SessionKind::Undo => "kind.undo",
            });
            let state = t.t(match s.status {
                Status::Pending => "session.pending",
                Status::Applied => "session.applied",
                Status::RolledBack => "session.rolled_back",
                Status::Failed => "session.failed",
            });
            let count = env.journal.entries(s.id)?.len();
            writeln!(
                out,
                "{}",
                t.f(
                    "status.session",
                    &[
                        ("id", &s.id.0),
                        ("kind", &kind),
                        ("status", &state),
                        ("time", &format::time(s.started_at)),
                        ("count", &count),
                    ]
                )
            )?;
        }
    }
    let applied = env.journal.applied_recipe_ids()?;
    if applied.is_empty() {
        writeln!(out, "{}", t.t("status.no_applied"))?;
    } else {
        writeln!(
            out,
            "{}",
            t.f("status.applied_recipes", &[("list", &applied.join(", "))])
        )?;
    }
    Ok(Exit::Ok)
}
