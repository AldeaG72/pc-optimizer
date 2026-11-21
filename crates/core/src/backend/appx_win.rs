//! Real preinstalled (Store) apps, through the built-in Appx PowerShell module.
//!
//! An app is only offered for removal when Windows keeps a copy of it (it is "provisioned"
//! in the Windows image): then undo can reinstall it without the internet. Listing those
//! copies needs administrator rights, so the read-only backend the app uses can only tell
//! "installed" from "not installed".
//!
//! Package names reach PowerShell, so they are checked again here (letters, digits, `.`,
//! `-`), on top of the recipe loader's check.

use std::cell::RefCell;
use std::collections::HashSet;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;

use super::{AppxBackend, AppxState, BackendError};
use crate::recipe::is_protected_package;

/// No console window for the PowerShell child.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Default)]
struct Lists {
    /// Lowercase names installed for the current user.
    installed: HashSet<String>,
    /// Lowercase names Windows keeps a copy of; `None` in read-only mode.
    provisioned: Option<HashSet<String>>,
}

#[derive(Debug)]
pub struct WindowsAppx {
    read_only: bool,
    lists: RefCell<Option<Lists>>,
}

impl WindowsAppx {
    /// Can remove and reinstall apps. Only for the elevated helper and `pco`.
    pub fn system() -> Self {
        Self {
            read_only: false,
            lists: RefCell::new(None),
        }
    }

    /// Every change is refused. For the app and for tests on the development PC.
    pub fn read_only() -> Self {
        Self {
            read_only: true,
            lists: RefCell::new(None),
        }
    }

    fn lists(&self) -> Result<std::cell::Ref<'_, Lists>, BackendError> {
        if self.lists.borrow().is_none() {
            let installed = names("Get-AppxPackage | ForEach-Object { $_.Name }")?;
            let provisioned = if self.read_only {
                None
            } else {
                Some(names(
                    "Get-AppxProvisionedPackage -Online | ForEach-Object { $_.DisplayName }",
                )?)
            };
            *self.lists.borrow_mut() = Some(Lists {
                installed,
                provisioned,
            });
        }
        let lists = self.lists.borrow();
        if lists.is_none() {
            return Err(BackendError::NotFound {
                target: "app list".into(),
            });
        }
        Ok(std::cell::Ref::map(lists, |l| {
            l.as_ref().unwrap_or_else(|| unreachable!())
        }))
    }
}

fn safe_name(package: &str) -> Result<(), BackendError> {
    let ok = (3..=100).contains(&package.len())
        && package
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    if ok && is_protected_package(package).is_none() {
        Ok(())
    } else {
        Err(BackendError::NotFound {
            target: format!("app {package}"),
        })
    }
}

/// `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe`, from the API rather than
/// the environment (this also runs elevated).
fn powershell_path() -> PathBuf {
    let mut buf = [0u16; 260];
    // SAFETY: the buffer is writable for its whole length.
    let len = unsafe { GetSystemWindowsDirectoryW(Some(&mut buf)) } as usize;
    let windows = if len > 0 && len < buf.len() {
        PathBuf::from(String::from_utf16_lossy(&buf[..len]))
    } else {
        PathBuf::from(r"C:\Windows")
    };
    windows.join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
}

/// Runs a fixed script; returns its standard output.
fn run(script: &str, target: &str) -> Result<String, BackendError> {
    let full =
        format!("$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; {script}");
    let out = Command::new(powershell_path())
        .args(["-NoProfile", "-NonInteractive", "-Command", &full])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| BackendError::Os {
            op: "start PowerShell",
            target: target.into(),
            code: e.raw_os_error().map_or(0, |c| c as u32),
            message: e.to_string(),
        })?;
    let text = |b: &[u8]| String::from_utf8_lossy(b).trim().to_owned();
    if out.status.success() {
        Ok(text(&out.stdout))
    } else {
        let err = text(&out.stderr);
        Err(BackendError::Os {
            op: "PowerShell",
            target: target.into(),
            code: out.status.code().map_or(0, |c| c as u32),
            message: if err.is_empty() {
                text(&out.stdout)
            } else {
                err
            },
        })
    }
}

fn names(script: &str) -> Result<HashSet<String>, BackendError> {
    Ok(run(script, "app list")?
        .lines()
        .map(|l| l.trim().to_ascii_lowercase())
        .filter(|l| !l.is_empty())
        .collect())
}

impl AppxBackend for WindowsAppx {
    fn query(&self, package: &str) -> Result<AppxState, BackendError> {
        safe_name(package)?;
        let lists = self.lists()?;
        let name = package.to_ascii_lowercase();
        let installed = lists.installed.contains(&name);
        Ok(match (&lists.provisioned, installed) {
            // Read-only: whether a copy is kept is checked by the helper before removing.
            (None, true) => AppxState::Installed,
            (None, false) => AppxState::Removed,
            (Some(kept), true) if kept.contains(&name) => AppxState::Installed,
            (Some(_), true) => AppxState::NotRestorable,
            (Some(kept), false) if kept.contains(&name) => AppxState::Removed,
            (Some(_), false) => AppxState::Missing,
        })
    }

    fn set_installed(&mut self, package: &str, installed: bool) -> Result<(), BackendError> {
        let target = format!("app {package}");
        if self.read_only {
            return Err(BackendError::ReadOnly { target });
        }
        safe_name(package)?;
        let script = if installed {
            // Reinstall from the copy Windows keeps: register the provisioned package for
            // this user by its family name (Name_PublisherId). No download.
            format!(
                "$p = Get-AppxProvisionedPackage -Online | Where-Object DisplayName -eq '{package}' | Select-Object -First 1; \
                 if (-not $p) {{ throw 'Windows keeps no copy of {package}' }}; \
                 Add-AppxPackage -RegisterByFamilyName -MainPackage ('{package}_' + $p.PublisherId)"
            )
        } else {
            format!("Get-AppxPackage -Name '{package}' | Remove-AppxPackage")
        };
        let result = run(&script, &target);
        // Whatever happened, read the lists again next time.
        *self.lists.borrow_mut() = None;
        result.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_unsafe_or_protected_names() {
        for bad in ["a'b", "x; Remove-Item C:\\", "Microsoft.SecHealthUI", "ab"] {
            assert!(safe_name(bad).is_err(), "{bad}");
        }
        assert!(safe_name("Clipchamp.Clipchamp").is_ok());
    }

    /// Read-only: lists the apps, changes nothing. Safe on the development PC.
    #[test]
    fn read_only_lists_but_never_changes() {
        let mut apps = WindowsAppx::read_only();
        let state = apps.query("Microsoft.WindowsCalculator").unwrap();
        assert!(matches!(state, AppxState::Installed | AppxState::Removed));
        assert!(matches!(
            apps.set_installed("Microsoft.WindowsCalculator", false),
            Err(BackendError::ReadOnly { .. })
        ));
    }
}
