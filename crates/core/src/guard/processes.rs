//! The list of game and anti-cheat processes that block apply/undo
//! (`config/blocked-processes.yaml`).

use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

/// The list shipped with the app, used when no override file exists.
const BUILTIN: &str = include_str!("../../../../config/blocked-processes.yaml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProcessKind {
    AntiCheat,
    Game,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockedProcess {
    /// Executable name, e.g. `cs2.exe`.
    pub name: String,
    /// What it is, for the "close the game first" message.
    pub what: String,
    pub kind: ProcessKind,
    pub verified: bool,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    processes: Vec<BlockedProcess>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessListError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{origin}: {message}")]
    Invalid { origin: String, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedProcesses(Vec<BlockedProcess>);

impl BlockedProcesses {
    /// The list built into the app.
    pub fn builtin() -> Result<Self, ProcessListError> {
        Self::parse(BUILTIN, "built-in blocked-processes.yaml")
    }

    pub fn load(path: &Path) -> Result<Self, ProcessListError> {
        let text = std::fs::read_to_string(path).map_err(|source| ProcessListError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::parse(&text, &path.display().to_string())
    }

    /// `origin` names the source in error messages.
    pub fn parse(text: &str, origin: &str) -> Result<Self, ProcessListError> {
        let invalid = |message: String| ProcessListError::Invalid {
            origin: origin.to_owned(),
            message,
        };
        let file: File = serde_yaml_ng::from_str(text).map_err(|e| invalid(e.to_string()))?;
        if file.processes.is_empty() {
            return Err(invalid("the list is empty".into()));
        }
        let mut seen = HashSet::new();
        for p in &file.processes {
            let lower = p.name.to_lowercase();
            if p.name.trim() != p.name
                || !lower.ends_with(".exe")
                || lower.len() <= 4
                || p.name.contains(['\\', '/'])
            {
                return Err(invalid(format!(
                    "`{}` must be a file name like `cs2.exe`",
                    p.name
                )));
            }
            if p.what.trim().is_empty() {
                return Err(invalid(format!("`{}` needs a `what`", p.name)));
            }
            if p.verified && p.source.is_none() {
                return Err(invalid(format!(
                    "`{}` is marked verified but has no `source`",
                    p.name
                )));
            }
            if !seen.insert(lower) {
                return Err(invalid(format!("`{}` is listed twice", p.name)));
            }
        }
        Ok(Self(file.processes))
    }

    pub fn entries(&self) -> &[BlockedProcess] {
        &self.0
    }

    /// The listed processes among `running` (executable names), in list order.
    pub fn find_running(&self, running: &[String]) -> Vec<&BlockedProcess> {
        let running: HashSet<String> = running.iter().map(|n| n.to_lowercase()).collect();
        self.0
            .iter()
            .filter(|p| running.contains(&p.name.to_lowercase()))
            .collect()
    }
}
