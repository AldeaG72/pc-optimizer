//! The real places to clean. Folder paths come from Windows APIs (known folders, the
//! Windows directory), never from environment variables.

use std::path::PathBuf;

use windows::core::PCWSTR;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, SHEmptyRecycleBinW, SHGetKnownFolderPath, SHQueryRecycleBinW,
    KF_FLAG_DEFAULT, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHQUERYRBINFO,
};

use super::{
    clean_dir, measure_dir, needs_admin, CleanReport, CleanupBackend, CleanupError, Rules, TEMP_AGE,
};
use crate::recipe::CleanupTarget;

/// Launcher web caches below `%LOCALAPPDATA%`; the launchers rebuild them on demand.
/// Only the folders the launcher's own support pages say to delete (Epic Games).
const LAUNCHER_CACHES: &[&str] = &[
    r"EpicGamesLauncher\Saved\webcache",
    r"EpicGamesLauncher\Saved\webcache_4147",
    r"EpicGamesLauncher\Saved\webcache_4430",
];

/// One side of the cleanup: the app (the user's own places) or the elevated helper
/// (system folders). Each refuses the other's targets.
#[derive(Debug, Clone, Copy)]
pub struct WindowsCleanup {
    elevated: bool,
}

impl WindowsCleanup {
    /// For the app, without administrator rights: temporary files, launcher caches,
    /// Recycle Bin.
    pub fn user() -> Self {
        Self { elevated: false }
    }

    /// For the elevated helper: `C:\Windows\Temp` (files directly inside only).
    pub fn system() -> Self {
        Self { elevated: true }
    }

    fn check_side(&self, target: CleanupTarget) -> Result<(), CleanupError> {
        if needs_admin(target) == self.elevated {
            Ok(())
        } else if self.elevated {
            Err(CleanupError::WrongSide {
                what: "the user's own files",
            })
        } else {
            Err(CleanupError::WrongSide {
                what: "system folders",
            })
        }
    }

    /// The folders of a target and how each is emptied.
    fn folders(target: CleanupTarget) -> Result<Vec<(PathBuf, Rules)>, CleanupError> {
        let temp_rules = |recurse| Rules {
            recurse,
            min_age: Some(TEMP_AGE),
        };
        Ok(match target {
            CleanupTarget::UserTemp => vec![(local_app_data()?.join("Temp"), temp_rules(true))],
            CleanupTarget::LauncherCaches => {
                let base = local_app_data()?;
                LAUNCHER_CACHES
                    .iter()
                    .map(|p| base.join(p))
                    .filter(|p| p.is_dir())
                    .map(|p| {
                        (
                            p,
                            Rules {
                                recurse: true,
                                min_age: None,
                            },
                        )
                    })
                    .collect()
            }
            // Other users can create files here: only the files directly inside.
            CleanupTarget::WindowsTemp => vec![(windows_dir()?.join("Temp"), temp_rules(false))],
            CleanupTarget::RecycleBin => Vec::new(),
        })
    }
}

fn local_app_data() -> Result<PathBuf, CleanupError> {
    // SAFETY: FOLDERID_LocalAppData is a static GUID; the returned string is freed below.
    let path = unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, KF_FLAG_DEFAULT, None) }
        .map_err(|e| CleanupError::Os(e.message()))?;
    // SAFETY: a valid NUL-terminated string from SHGetKnownFolderPath.
    let text = unsafe { path.to_string() }.map_err(|e| CleanupError::Os(e.to_string()));
    // SAFETY: allocated by SHGetKnownFolderPath with CoTaskMemAlloc.
    unsafe { CoTaskMemFree(Some(path.0 as *const _)) };
    let text = text?;
    if text.is_empty() {
        return Err(CleanupError::NoFolder {
            what: "local app data",
        });
    }
    Ok(PathBuf::from(text))
}

fn windows_dir() -> Result<PathBuf, CleanupError> {
    let mut buf = [0u16; 260];
    // SAFETY: the buffer is writable for its whole length.
    let len = unsafe { GetSystemWindowsDirectoryW(Some(&mut buf)) } as usize;
    if len == 0 || len >= buf.len() {
        return Err(CleanupError::NoFolder { what: "Windows" });
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..len])))
}

fn recycle_bin_size() -> Result<u64, CleanupError> {
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a valid out-structure with its size set; null root = all drives.
    unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) }
        .map_err(|e| CleanupError::Os(e.message()))?;
    Ok(u64::try_from(info.i64Size).unwrap_or(0))
}

impl CleanupBackend for WindowsCleanup {
    fn measure(&self, target: CleanupTarget) -> Result<Option<u64>, CleanupError> {
        if target == CleanupTarget::RecycleBin {
            return recycle_bin_size().map(Some);
        }
        let mut total = 0;
        for (dir, rules) in Self::folders(target)? {
            match measure_dir(&dir, rules) {
                Ok(n) => total += n,
                // A system folder seen without administrator rights.
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Ok(None),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(Some(total))
    }

    fn clean(&mut self, target: CleanupTarget) -> Result<CleanReport, CleanupError> {
        self.check_side(target)?;
        if target == CleanupTarget::RecycleBin {
            let size = recycle_bin_size()?;
            if size == 0 {
                return Ok(CleanReport::default());
            }
            let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
            // SAFETY: no window, all drives.
            unsafe { SHEmptyRecycleBinW(None, PCWSTR::null(), flags) }
                .map_err(|e| CleanupError::Os(e.message()))?;
            return Ok(CleanReport {
                freed_bytes: size,
                deleted: 1,
                skipped: 0,
            });
        }
        let mut report = CleanReport::default();
        for (dir, rules) in Self::folders(target)? {
            match clean_dir(&dir, rules) {
                Ok(r) => report.add(r),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Measuring changes nothing: safe on the development PC.
    #[test]
    fn measures_without_deleting() {
        let side = WindowsCleanup::user();
        assert!(side.measure(CleanupTarget::UserTemp).unwrap().is_some());
        assert!(side.measure(CleanupTarget::RecycleBin).unwrap().is_some());
        side.measure(CleanupTarget::LauncherCaches).unwrap();
    }

    #[test]
    fn each_side_refuses_the_other_targets() {
        assert!(WindowsCleanup::user()
            .check_side(CleanupTarget::WindowsTemp)
            .is_err());
        assert!(WindowsCleanup::system()
            .check_side(CleanupTarget::UserTemp)
            .is_err());
        assert!(WindowsCleanup::system()
            .check_side(CleanupTarget::WindowsTemp)
            .is_ok());
    }

    #[test]
    fn folders_come_from_windows_not_the_environment() {
        let dirs = WindowsCleanup::folders(CleanupTarget::WindowsTemp).unwrap();
        assert!(dirs[0].0.ends_with("Temp"));
        assert!(!dirs[0].1.recurse, "Windows Temp: never inside subfolders");
        let user = WindowsCleanup::folders(CleanupTarget::UserTemp).unwrap();
        assert!(user[0].0.ends_with(r"AppData\Local\Temp") || user[0].0.ends_with("Temp"));
    }
}
