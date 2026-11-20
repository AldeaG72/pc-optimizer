use std::fmt;
use std::path::PathBuf;

/// A problem with one recipe file.
#[derive(Debug, thiserror::Error)]
pub enum RecipeError {
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The file is not a valid recipe. `field` is a path like `actions[0].set`.
    #[error("{}: field `{field}`: {message}", path.display())]
    Invalid {
        path: PathBuf,
        field: String,
        message: String,
    },

    /// The recipe touches something the project never changes (CLAUDE.md, safety rule 4).
    #[error("{}: field `{field}`: blocked by safety rules: {reason}", path.display())]
    Forbidden {
        path: PathBuf,
        field: String,
        reason: String,
    },

    #[error("duplicate recipe id `{id}` in {} and {}", first.display(), second.display())]
    DuplicateId {
        id: String,
        first: PathBuf,
        second: PathBuf,
    },

    #[error("no recipe files (*.yaml) found in {}", dir.display())]
    NoRecipes { dir: PathBuf },
}

/// Every problem found while loading a recipe folder.
///
/// Loading never stops at the first bad file, so the author sees all problems at once.
#[derive(Debug)]
pub struct LoadErrors(pub Vec<RecipeError>);

impl fmt::Display for LoadErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} invalid recipe(s):", self.0.len())?;
        for e in &self.0 {
            write!(f, "\n  - {e}")?;
        }
        Ok(())
    }
}

impl std::error::Error for LoadErrors {}
