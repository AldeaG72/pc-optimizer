//! File cleanup (SPEC 4.2 "Curățare"). This is the one thing the app does that cannot be
//! undone, so:
//! - the places are fixed in code ([`CleanupTarget`]); a recipe only names one;
//! - the interface asks first and shows the size; nothing is in a "Recommended" profile;
//! - links (symbolic links, junctions) are never followed: a link is removed itself, never
//!   what it points to;
//! - where other users can create files (`C:\Windows\Temp`), the elevated helper only
//!   deletes the files directly inside, never inside subfolders (a subfolder could be
//!   swapped for a link to a system folder while it is being emptied);
//! - the user's own places (temporary files, launcher caches, Recycle Bin) are cleaned by
//!   the app itself, without administrator rights, so nothing elevated ever works in a
//!   folder the user controls;
//! - temporary files changed in the last 24 hours are kept (an installer may still need
//!   them); files in use are skipped and counted.

#[cfg(windows)]
pub mod windows;

use std::fs;
use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime};

use serde::Serialize;

use crate::recipe::CleanupTarget;

/// What one cleanup did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct CleanReport {
    pub freed_bytes: u64,
    pub deleted: u64,
    /// Files in use or protected: left in place.
    pub skipped: u64,
}

impl CleanReport {
    pub fn add(&mut self, other: CleanReport) {
        self.freed_bytes += other.freed_bytes;
        self.deleted += other.deleted;
        self.skipped += other.skipped;
    }
}

/// Only the elevated helper can clean these (system folders).
pub fn needs_admin(target: CleanupTarget) -> bool {
    matches!(target, CleanupTarget::WindowsTemp)
}

#[derive(Debug, thiserror::Error)]
pub enum CleanupError {
    #[error("{what} cannot be cleaned from here")]
    WrongSide { what: &'static str },
    #[error("cannot find the folder for {what}")]
    NoFolder { what: &'static str },
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Os(String),
}

/// Measures and cleans the fixed places. The real one is in [`windows`].
pub trait CleanupBackend {
    /// Bytes a cleanup would free now; `None` if this side cannot see it (e.g. a system
    /// folder from the app without administrator rights).
    fn measure(&self, target: CleanupTarget) -> Result<Option<u64>, CleanupError>;

    fn clean(&mut self, target: CleanupTarget) -> Result<CleanReport, CleanupError>;
}

/// How a folder is emptied.
#[derive(Debug, Clone, Copy)]
pub struct Rules {
    /// Empty subfolders too (only where no other user can create files).
    pub recurse: bool,
    /// Keep files changed more recently than this.
    pub min_age: Option<Duration>,
}

/// Temporary files: keep the last day's.
pub const TEMP_AGE: Duration = Duration::from_secs(24 * 60 * 60);

fn is_link(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT: junctions and other links.
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn old_enough(meta: &fs::Metadata, rules: Rules, now: SystemTime) -> bool {
    let Some(min_age) = rules.min_age else {
        return true;
    };
    meta.modified()
        .ok()
        .and_then(|m| now.duration_since(m).ok())
        .is_some_and(|age| age >= min_age)
}

/// Bytes `clean_dir` would free, without changing anything.
pub fn measure_dir(dir: &Path, rules: Rules) -> io::Result<u64> {
    measure_inner(dir, rules, SystemTime::now())
}

fn measure_inner(dir: &Path, rules: Rules, now: SystemTime) -> io::Result<u64> {
    let mut total = 0;
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if is_link(&meta) {
            continue;
        }
        if meta.is_dir() {
            if rules.recurse {
                total += measure_inner(&entry.path(), rules, now).unwrap_or(0);
            }
        } else if old_enough(&meta, rules, now) {
            total += meta.len();
        }
    }
    Ok(total)
}

/// Deletes what is inside `dir` (never `dir` itself), following the rules above.
pub fn clean_dir(dir: &Path, rules: Rules) -> io::Result<CleanReport> {
    // The folder itself must be a real folder, not a link to somewhere else.
    let meta = fs::symlink_metadata(dir)?;
    if is_link(&meta) || !meta.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a plain folder", dir.display()),
        ));
    }
    Ok(clean_inner(dir, rules, SystemTime::now()))
}

fn clean_inner(dir: &Path, rules: Rules, now: SystemTime) -> CleanReport {
    let mut report = CleanReport::default();
    let Ok(entries) = fs::read_dir(dir) else {
        report.skipped += 1;
        return report;
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if is_link(&meta) {
            // The link itself goes; what it points to is never touched.
            if old_enough(&meta, rules, now) {
                let removed = if meta.is_dir() {
                    fs::remove_dir(&path)
                } else {
                    fs::remove_file(&path)
                };
                match removed {
                    Ok(()) => report.deleted += 1,
                    Err(_) => report.skipped += 1,
                }
            }
            continue;
        }
        if meta.is_dir() {
            if rules.recurse {
                report.add(clean_inner(&path, rules, now));
                // Only if it is empty now.
                let _ = fs::remove_dir(&path);
            }
            continue;
        }
        if !old_enough(&meta, rules, now) {
            continue;
        }
        match fs::remove_file(&path) {
            Ok(()) => {
                report.freed_bytes += meta.len();
                report.deleted += 1;
            }
            Err(_) => report.skipped += 1,
        }
    }
    report
}

/// In-memory cleanup for tests: sizes per target, emptied by `clean`.
#[derive(Debug, Clone, Default)]
pub struct MemoryCleanup {
    pub sizes: std::collections::HashMap<CleanupTarget, u64>,
    /// Targets this side may not clean (like the app for system folders).
    pub admin: bool,
}

impl CleanupBackend for MemoryCleanup {
    fn measure(&self, target: CleanupTarget) -> Result<Option<u64>, CleanupError> {
        Ok(self.sizes.get(&target).copied())
    }

    fn clean(&mut self, target: CleanupTarget) -> Result<CleanReport, CleanupError> {
        if needs_admin(target) && !self.admin {
            return Err(CleanupError::WrongSide {
                what: "system folders",
            });
        }
        let freed = self.sizes.insert(target, 0).unwrap_or(0);
        Ok(CleanReport {
            freed_bytes: freed,
            deleted: u64::from(freed > 0),
            skipped: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pco-cleanup-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const ALL: Rules = Rules {
        recurse: true,
        min_age: None,
    };

    #[test]
    fn empties_the_folder_but_keeps_it() {
        let dir = temp("empty");
        fs::write(dir.join("a.tmp"), b"12345").unwrap();
        fs::create_dir_all(dir.join("sub/deeper")).unwrap();
        fs::write(dir.join("sub/deeper/b.tmp"), b"123").unwrap();
        assert_eq!(measure_dir(&dir, ALL).unwrap(), 8);
        let report = clean_dir(&dir, ALL).unwrap();
        assert_eq!(report.freed_bytes, 8);
        assert_eq!(report.deleted, 2);
        assert!(dir.exists());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn top_level_only_keeps_subfolders() {
        let dir = temp("top");
        fs::write(dir.join("a.tmp"), b"1").unwrap();
        fs::create_dir(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/b.tmp"), b"1").unwrap();
        let rules = Rules {
            recurse: false,
            min_age: None,
        };
        assert_eq!(measure_dir(&dir, rules).unwrap(), 1);
        clean_dir(&dir, rules).unwrap();
        assert!(!dir.join("a.tmp").exists());
        assert!(dir.join("sub/b.tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn recent_files_are_kept() {
        let dir = temp("recent");
        fs::write(dir.join("new.tmp"), b"1").unwrap();
        let rules = Rules {
            recurse: true,
            min_age: Some(TEMP_AGE),
        };
        assert_eq!(measure_dir(&dir, rules).unwrap(), 0);
        assert_eq!(clean_dir(&dir, rules).unwrap().deleted, 0);
        assert!(dir.join("new.tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A link inside the folder is removed itself; the folder it points to is untouched.
    #[test]
    fn links_are_never_followed() {
        let dir = temp("links");
        let outside = temp("links-outside");
        fs::write(outside.join("precious.txt"), b"keep me").unwrap();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&outside, dir.join("link")).is_ok();
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&outside, dir.join("link")).is_ok();
        if made {
            assert_eq!(measure_dir(&dir, ALL).unwrap(), 0);
            clean_dir(&dir, ALL).unwrap();
            assert!(outside.join("precious.txt").exists(), "followed a link");
            // And a link given as the folder itself is refused.
            let link = dir.join("link2");
            #[cfg(unix)]
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            #[cfg(windows)]
            std::os::windows::fs::symlink_dir(&outside, &link).unwrap();
            assert!(clean_dir(&link, ALL).is_err());
            assert!(outside.join("precious.txt").exists());
        }
        // (Creating links can need Developer Mode on Windows; the test still passes.)
        let _ = fs::remove_dir_all(&dir);
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn memory_backend_refuses_system_folders_without_admin() {
        let mut m = MemoryCleanup::default();
        m.sizes.insert(CleanupTarget::UserTemp, 10);
        m.sizes.insert(CleanupTarget::WindowsTemp, 20);
        assert_eq!(m.clean(CleanupTarget::UserTemp).unwrap().freed_bytes, 10);
        assert!(m.clean(CleanupTarget::WindowsTemp).is_err());
        assert_eq!(m.measure(CleanupTarget::UserTemp).unwrap(), Some(0));
    }
}
