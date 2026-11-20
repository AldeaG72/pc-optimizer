//! Undo journal (SQLite).
//!
//! Write-ahead rule (CLAUDE.md, rules 2 and 3): the engine records an entry as `pending`
//! with the exact previous state *before* changing anything, then marks it `applied`.
//! Every call commits immediately with `synchronous = FULL`, so a power cut right after
//! `record_pending` still leaves the previous state on disk.
//!
//! The journal stores states as JSON and does not interpret them; the engine decides what
//! they mean.

mod migrations;

use std::fmt;
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use serde::Serialize;

use migrations::MIGRATIONS;

use crate::bench::{Label, Metrics, Series};

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("journal database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("journal data error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("cannot create the journal folder {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Written by a newer app version; refusing to touch it rather than risk losing undo data.
    #[error("the journal was created by a newer version (schema {found}, this version knows {supported}); update the app")]
    NewerSchema { found: u32, supported: u32 },

    #[error("{what} {id} does not exist in the journal")]
    NotFound { what: &'static str, id: i64 },

    #[error("{what} {id}: cannot change status from `{from}` to `{to}`")]
    InvalidTransition {
        what: &'static str,
        id: i64,
        from: Status,
        to: Status,
    },

    #[error("unexpected value `{0}` in the journal")]
    Corrupt(String),
}

pub type Result<T> = std::result::Result<T, JournalError>;

/// Status of a session or an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    /// Started but not finished. After a crash, recovery undoes it.
    Pending,
    /// The change is in place.
    Applied,
    /// The change was reverted (by a later undo, or by the engine after a failure).
    RolledBack,
    /// The change could not be made; nothing remains to undo.
    Failed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Applied => "applied",
            Status::RolledBack => "rolled_back",
            Status::Failed => "failed",
        }
    }

    /// Allowed status changes. Anything else is a bug and is refused.
    fn can_become(self, to: Status) -> bool {
        matches!(
            (self, to),
            (
                Status::Pending,
                Status::Applied | Status::RolledBack | Status::Failed
            ) | (Status::Applied, Status::RolledBack)
        )
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Status {
    type Err = JournalError;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "pending" => Status::Pending,
            "applied" => Status::Applied,
            "rolled_back" => Status::RolledBack,
            "failed" => Status::Failed,
            other => return Err(JournalError::Corrupt(other.into())),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionKind {
    Apply,
    Undo,
}

impl SessionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionKind::Apply => "apply",
            SessionKind::Undo => "undo",
        }
    }
}

impl FromStr for SessionKind {
    type Err = JournalError;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "apply" => SessionKind::Apply,
            "undo" => SessionKind::Undo,
            other => return Err(JournalError::Corrupt(other.into())),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    Registry,
    Service,
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryKind::Registry => "registry",
            EntryKind::Service => "service",
        }
    }
}

impl FromStr for EntryKind {
    type Err = JournalError;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "registry" => EntryKind::Registry,
            "service" => EntryKind::Service,
            other => return Err(JournalError::Corrupt(other.into())),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryId(pub i64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub kind: SessionKind,
    pub status: Status,
    /// Unix time in milliseconds.
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub app_version: String,
}

/// One action of one recipe, as recorded before it was carried out.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: EntryId,
    pub session_id: SessionId,
    pub recipe_id: String,
    pub recipe_version: u32,
    pub action_index: usize,
    pub kind: EntryKind,
    /// Human-readable target, e.g. `HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled`.
    pub target: String,
    pub previous_state: serde_json::Value,
    pub new_state: serde_json::Value,
    pub status: Status,
    pub error: Option<String>,
}

impl Entry {
    pub fn previous<T: DeserializeOwned>(&self) -> Result<T> {
        Ok(serde_json::from_value(self.previous_state.clone())?)
    }

    pub fn target_state<T: DeserializeOwned>(&self) -> Result<T> {
        Ok(serde_json::from_value(self.new_state.clone())?)
    }
}

/// What to record before an action is carried out.
#[derive(Debug, Clone)]
pub struct NewEntry<'a, P: Serialize, N: Serialize> {
    pub recipe_id: &'a str,
    pub recipe_version: u32,
    pub action_index: usize,
    pub kind: EntryKind,
    pub target: &'a str,
    pub previous_state: &'a P,
    pub new_state: &'a N,
}

pub struct Journal {
    conn: Connection,
}

impl Journal {
    /// Opens (or creates) the journal file and brings its schema up to date.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|source| JournalError::Io {
                path: dir.display().to_string(),
                source,
            })?;
        }
        Self::init(Connection::open(path)?)
    }

    /// A journal that lives only in memory (tests).
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "synchronous", "FULL")?;

        let version: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let supported = MIGRATIONS.len() as u32;
        if version > supported {
            return Err(JournalError::NewerSchema {
                found: version,
                supported,
            });
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", i as u32 + 1)?;
            tx.commit()?;
        }
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    pub fn begin_session(&mut self, kind: SessionKind) -> Result<SessionId> {
        self.conn.execute(
            "INSERT INTO sessions (kind, status, started_at, app_version) VALUES (?1, 'pending', ?2, ?3)",
            params![kind.as_str(), now_ms(), crate::VERSION],
        )?;
        Ok(SessionId(self.conn.last_insert_rowid()))
    }

    pub fn finish_session(&mut self, id: SessionId, status: Status) -> Result<()> {
        self.finish_session_rolling_back(id, status, &[])
    }

    /// Finishes a session and marks `rolled_back` entries (from earlier sessions) as
    /// `rolled_back`, all in one transaction. An undo session uses this so that a crash can
    /// never leave "the original change is undone" recorded without "the undo finished".
    pub fn finish_session_rolling_back(
        &mut self,
        id: SessionId,
        status: Status,
        rolled_back: &[EntryId],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        for entry in rolled_back {
            let current = entry_status(&tx, *entry)?;
            if !current.can_become(Status::RolledBack) {
                return Err(JournalError::InvalidTransition {
                    what: "entry",
                    id: entry.0,
                    from: current,
                    to: Status::RolledBack,
                });
            }
            tx.execute(
                "UPDATE entries SET status = 'rolled_back', updated_at = ?1 WHERE id = ?2",
                params![now_ms(), entry.0],
            )?;
        }
        let current: Option<String> = tx
            .query_row("SELECT status FROM sessions WHERE id = ?1", [id.0], |r| {
                r.get(0)
            })
            .optional()?;
        let current: Status = current
            .ok_or(JournalError::NotFound {
                what: "session",
                id: id.0,
            })?
            .parse()?;
        if !current.can_become(status) {
            return Err(JournalError::InvalidTransition {
                what: "session",
                id: id.0,
                from: current,
                to: status,
            });
        }
        tx.execute(
            "UPDATE sessions SET status = ?1, finished_at = ?2 WHERE id = ?3",
            params![status.as_str(), now_ms(), id.0],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Adds an error note to an entry without changing its status.
    pub fn set_entry_error(&mut self, id: EntryId, error: &str) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE entries SET error = ?1, updated_at = ?2 WHERE id = ?3",
            params![error, now_ms(), id.0],
        )?;
        if changed == 0 {
            return Err(JournalError::NotFound {
                what: "entry",
                id: id.0,
            });
        }
        Ok(())
    }

    /// Records an action as `pending`, with the state to restore. Call this *before* the
    /// action changes anything.
    pub fn record_pending<P: Serialize, N: Serialize>(
        &mut self,
        session: SessionId,
        entry: &NewEntry<'_, P, N>,
    ) -> Result<EntryId> {
        let status = self.session(session)?.status;
        if status != Status::Pending {
            return Err(JournalError::InvalidTransition {
                what: "session",
                id: session.0,
                from: status,
                to: Status::Pending,
            });
        }
        self.conn.execute(
            "INSERT INTO entries (session_id, recipe_id, recipe_version, action_index, kind, target,
                                  previous_state, new_state, status, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9)",
            params![
                session.0,
                entry.recipe_id,
                entry.recipe_version,
                entry.action_index as i64,
                entry.kind.as_str(),
                entry.target,
                serde_json::to_string(entry.previous_state)?,
                serde_json::to_string(entry.new_state)?,
                now_ms(),
            ],
        )?;
        Ok(EntryId(self.conn.last_insert_rowid()))
    }

    /// Changes an entry's status (e.g. `pending` → `applied`). `error` explains a failure.
    pub fn set_entry_status(
        &mut self,
        id: EntryId,
        status: Status,
        error: Option<&str>,
    ) -> Result<()> {
        let current = self.entry(id)?.status;
        if !current.can_become(status) {
            return Err(JournalError::InvalidTransition {
                what: "entry",
                id: id.0,
                from: current,
                to: status,
            });
        }
        self.conn.execute(
            "UPDATE entries SET status = ?1, error = COALESCE(?2, error), updated_at = ?3 WHERE id = ?4",
            params![status.as_str(), error, now_ms(), id.0],
        )?;
        Ok(())
    }

    pub fn session(&self, id: SessionId) -> Result<Session> {
        self.conn
            .query_row(
                &format!("{SESSION_COLUMNS} WHERE id = ?1"),
                [id.0],
                session_from_row,
            )
            .optional()?
            .ok_or(JournalError::NotFound {
                what: "session",
                id: id.0,
            })?
    }

    /// All sessions, newest first.
    pub fn sessions(&self) -> Result<Vec<Session>> {
        let mut stmt = self
            .conn
            .prepare(&format!("{SESSION_COLUMNS} ORDER BY id DESC"))?;
        let rows = stmt.query_map([], session_from_row)?;
        rows.map(|r| r?).collect()
    }

    /// Sessions left `pending`, oldest first — what recovery must undo after a crash.
    pub fn pending_sessions(&self) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SESSION_COLUMNS} WHERE status = 'pending' ORDER BY id"
        ))?;
        let rows = stmt.query_map([], session_from_row)?;
        rows.map(|r| r?).collect()
    }

    pub fn entry(&self, id: EntryId) -> Result<Entry> {
        self.conn
            .query_row(
                &format!("{ENTRY_COLUMNS} WHERE id = ?1"),
                [id.0],
                entry_from_row,
            )
            .optional()?
            .ok_or(JournalError::NotFound {
                what: "entry",
                id: id.0,
            })?
    }

    /// Entries of a session in the order they were recorded.
    pub fn entries(&self, session: SessionId) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(&format!(
            "{ENTRY_COLUMNS} WHERE session_id = ?1 ORDER BY id"
        ))?;
        let rows = stmt.query_map([session.0], entry_from_row)?;
        rows.map(|r| r?).collect()
    }

    /// Changes from `apply` sessions that are still in place for a recipe, oldest first.
    /// Undo restores them newest first.
    pub fn applied_entries(&self, recipe_id: &str) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(&format!(
            "{ENTRY_COLUMNS} WHERE recipe_id = ?1 AND status = 'applied'
               AND session_id IN (SELECT id FROM sessions WHERE kind = 'apply')
             ORDER BY id"
        ))?;
        let rows = stmt.query_map([recipe_id], entry_from_row)?;
        rows.map(|r| r?).collect()
    }

    /// Every change from `apply` sessions that is still in place, oldest first.
    pub fn all_applied_entries(&self) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(&format!(
            "{ENTRY_COLUMNS} WHERE status = 'applied'
               AND session_id IN (SELECT id FROM sessions WHERE kind = 'apply')
             ORDER BY id"
        ))?;
        let rows = stmt.query_map([], entry_from_row)?;
        rows.map(|r| r?).collect()
    }

    // -- benchmarks (Stage 2) -------------------------------------------------------------

    /// Saves one run of a series (`series` = the series' start time in Unix ms).
    pub fn record_benchmark(
        &mut self,
        series: i64,
        target: &str,
        label: Label,
        m: &Metrics,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO benchmarks (series, target, label, created_at, frames, seconds, avg_fps,
                                     low1_fps, avg_frametime_ms, app_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                series,
                target,
                label.as_str(),
                now_ms(),
                i64::try_from(m.frames).unwrap_or(i64::MAX),
                m.seconds,
                m.avg_fps,
                m.low1_fps,
                m.avg_frametime_ms,
                crate::VERSION,
            ],
        )?;
        Ok(())
    }

    /// Every series, newest first; only `target`'s if given.
    pub fn benchmark_series(&self, target: Option<&str>) -> Result<Vec<Series>> {
        let mut stmt = self.conn.prepare(
            "SELECT series, target, label, frames, seconds, avg_fps, low1_fps, avg_frametime_ms
             FROM benchmarks WHERE (?1 IS NULL OR target = ?1) ORDER BY series DESC, id",
        )?;
        let rows = stmt.query_map([target], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                Metrics {
                    frames: u64::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
                    seconds: r.get(4)?,
                    avg_fps: r.get(5)?,
                    low1_fps: r.get(6)?,
                    avg_frametime_ms: r.get(7)?,
                },
            ))
        })?;
        let mut out: Vec<Series> = Vec::new();
        for row in rows {
            let (id, target, label, metrics) = row?;
            let label = Label::parse(&label).ok_or(JournalError::Corrupt(label))?;
            match out.last_mut() {
                Some(s) if s.id == id && s.target == target && s.label == label => {
                    s.runs.push(metrics)
                }
                _ => out.push(Series {
                    id,
                    target,
                    label,
                    runs: vec![metrics],
                }),
            }
        }
        Ok(out)
    }

    /// The newest series for `target` with `label`.
    pub fn latest_series(&self, target: &str, label: Label) -> Result<Option<Series>> {
        Ok(self
            .benchmark_series(Some(target))?
            .into_iter()
            .find(|s| s.label == label))
    }

    /// Deletes benchmark results (all of them, or one target's). Returns how many runs.
    pub fn forget_benchmarks(&mut self, target: Option<&str>) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM benchmarks WHERE (?1 IS NULL OR target = ?1)",
            [target],
        )?)
    }

    /// Recipes changed by `apply` sessions started at or after `since_ms` and still in place:
    /// what to undo when the benchmark shows a drop.
    pub fn applied_recipe_ids_since(&self, since_ms: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT recipe_id FROM entries WHERE status = 'applied'
               AND session_id IN (SELECT id FROM sessions WHERE kind = 'apply' AND started_at >= ?1)
             ORDER BY recipe_id",
        )?;
        let rows = stmt.query_map([since_ms], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Ids of every recipe with changes still in place, sorted.
    pub fn applied_recipe_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT recipe_id FROM entries WHERE status = 'applied'
               AND session_id IN (SELECT id FROM sessions WHERE kind = 'apply')
             ORDER BY recipe_id",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

const SESSION_COLUMNS: &str =
    "SELECT id, kind, status, started_at, finished_at, app_version FROM sessions";

const ENTRY_COLUMNS: &str =
    "SELECT id, session_id, recipe_id, recipe_version, action_index, kind, \
     target, previous_state, new_state, status, error FROM entries";

/// Row mappers return a nested `Result` so enum/JSON parse errors keep their own type.
fn session_from_row(row: &Row<'_>) -> rusqlite::Result<Result<Session>> {
    let kind: String = row.get(1)?;
    let status: String = row.get(2)?;
    let (id, started_at, finished_at, app_version) =
        (row.get(0)?, row.get(3)?, row.get(4)?, row.get(5)?);
    Ok((|| {
        Ok(Session {
            id: SessionId(id),
            kind: kind.parse()?,
            status: status.parse()?,
            started_at,
            finished_at,
            app_version,
        })
    })())
}

fn entry_from_row(row: &Row<'_>) -> rusqlite::Result<Result<Entry>> {
    let id: i64 = row.get(0)?;
    let session_id: i64 = row.get(1)?;
    let recipe_id: String = row.get(2)?;
    let recipe_version: u32 = row.get(3)?;
    let action_index: i64 = row.get(4)?;
    let kind: String = row.get(5)?;
    let target: String = row.get(6)?;
    let previous: String = row.get(7)?;
    let new: String = row.get(8)?;
    let status: String = row.get(9)?;
    let error: Option<String> = row.get(10)?;
    Ok((|| {
        Ok(Entry {
            id: EntryId(id),
            session_id: SessionId(session_id),
            recipe_id,
            recipe_version,
            action_index: usize::try_from(action_index)
                .map_err(|_| JournalError::Corrupt(action_index.to_string()))?,
            kind: kind.parse()?,
            target,
            previous_state: serde_json::from_str(&previous)?,
            new_state: serde_json::from_str(&new)?,
            status: status.parse()?,
            error,
        })
    })())
}

fn entry_status(conn: &Connection, id: EntryId) -> Result<Status> {
    let status: Option<String> = conn
        .query_row("SELECT status FROM entries WHERE id = ?1", [id.0], |r| {
            r.get(0)
        })
        .optional()?;
    status
        .ok_or(JournalError::NotFound {
            what: "entry",
            id: id.0,
        })?
        .parse()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_transitions() {
        use Status::*;
        let all = [Pending, Applied, RolledBack, Failed];
        let allowed = [
            (Pending, Applied),
            (Pending, RolledBack),
            (Pending, Failed),
            (Applied, RolledBack),
        ];
        for from in all {
            for to in all {
                assert_eq!(
                    from.can_become(to),
                    allowed.contains(&(from, to)),
                    "{from} -> {to}"
                );
            }
        }
    }

    #[test]
    fn status_text_round_trips() {
        for s in [
            Status::Pending,
            Status::Applied,
            Status::RolledBack,
            Status::Failed,
        ] {
            assert_eq!(s.as_str().parse::<Status>().unwrap(), s);
        }
        assert!("done".parse::<Status>().is_err());
    }
}
