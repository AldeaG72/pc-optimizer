//! Guards that run before anything changes (CLAUDE.md, safety rules 6 and 7):
//! - only Windows 11 x64 (build ≥ 22000); Windows 10 and ARM are refused;
//! - administrator rights only when the work touches HKLM or services;
//! - nothing runs while a game or a known anti-cheat is running;
//! - each recipe's own platform limits (build range, edition, desktop/laptop).
//!
//! The checks are pure functions over [`Host`] so they can be tested anywhere; the
//! `windows` module reads the real values.

mod processes;
#[cfg(windows)]
pub mod windows;

use std::fmt;

pub use processes::{BlockedProcess, BlockedProcesses, ProcessKind, ProcessListError};

use crate::recipe::{Edition, FormFactor, Platform};

/// Lowest Windows 11 build (21H2).
pub const MIN_BUILD: u32 = 22000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arch {
    X64,
    Arm64,
    X86,
    Other(u16),
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Arch::X64 => f.write_str("x64"),
            Arch::Arm64 => f.write_str("ARM64"),
            Arch::X86 => f.write_str("x86 (32-bit)"),
            Arch::Other(m) => write!(f, "unknown (machine {m:#06x})"),
        }
    }
}

/// Facts about the PC the app runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub build: u32,
    /// The processor's real architecture, even when the app runs under emulation.
    pub arch: Arch,
    pub elevated: bool,
    pub edition: Option<Edition>,
    /// `None` until hardware detection exists (Stage 1).
    pub form_factor: Option<FormFactor>,
}

/// What a recipe's `platform` block is compared with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemInfo {
    pub build: u32,
    pub edition: Option<Edition>,
    pub form_factor: Option<FormFactor>,
}

impl From<&Host> for SystemInfo {
    fn from(h: &Host) -> Self {
        Self {
            build: h.build,
            edition: h.edition,
            form_factor: h.form_factor,
        }
    }
}

/// A reason to stop before changing anything.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Blocked {
    #[error(
        "this app runs only on Windows 11 (build {MIN_BUILD} or newer); this PC has build {build}"
    )]
    NotWindows11 { build: u32 },

    #[error("this app runs only on 64-bit Intel/AMD (x64) Windows; this PC is {arch}")]
    NotX64 { arch: Arch },

    #[error("administrator rights are needed for changes under HKLM or to services")]
    NeedsAdmin,

    #[error("close the game first: {}", names(.running))]
    GameRunning { running: Vec<BlockedProcess> },
}

fn names(list: &[BlockedProcess]) -> String {
    list.iter()
        .map(|p| format!("{} ({})", p.what, p.name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Windows 11 on x64 only.
pub fn check_host(host: &Host) -> Result<(), Blocked> {
    if host.arch != Arch::X64 {
        return Err(Blocked::NotX64 { arch: host.arch });
    }
    if host.build < MIN_BUILD {
        return Err(Blocked::NotWindows11 { build: host.build });
    }
    Ok(())
}

/// `needs_admin`: whether the planned work touches HKLM or services.
pub fn check_admin(host: &Host, needs_admin: bool) -> Result<(), Blocked> {
    if needs_admin && !host.elevated {
        return Err(Blocked::NeedsAdmin);
    }
    Ok(())
}

/// `running`: executable names of the running processes.
pub fn check_no_games(running: &[String], list: &BlockedProcesses) -> Result<(), Blocked> {
    let found = list.find_running(running);
    if found.is_empty() {
        Ok(())
    } else {
        Err(Blocked::GameRunning {
            running: found.into_iter().cloned().collect(),
        })
    }
}

/// Why a recipe's platform limits exclude this PC, if they do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformMismatch {
    BuildTooOld {
        required: u32,
        current: u32,
    },
    BuildTooNew {
        max: u32,
        current: u32,
    },
    Edition {
        allowed: Vec<Edition>,
        current: Option<Edition>,
    },
    FormFactor {
        required: FormFactor,
        current: Option<FormFactor>,
    },
}

/// `None` if the recipe may run on this system. An unknown edition or form factor counts
/// as "does not match", so a recipe limited to, say, laptops never runs on a guess.
pub fn platform_mismatch(platform: &Platform, sys: &SystemInfo) -> Option<PlatformMismatch> {
    if sys.build < platform.min_build {
        return Some(PlatformMismatch::BuildTooOld {
            required: platform.min_build,
            current: sys.build,
        });
    }
    if let Some(max) = platform.max_build {
        if sys.build > max {
            return Some(PlatformMismatch::BuildTooNew {
                max,
                current: sys.build,
            });
        }
    }
    if !platform.editions.is_empty() && !sys.edition.is_some_and(|e| platform.editions.contains(&e))
    {
        return Some(PlatformMismatch::Edition {
            allowed: platform.editions.clone(),
            current: sys.edition,
        });
    }
    if platform.form_factor != FormFactor::Any && sys.form_factor != Some(platform.form_factor) {
        return Some(PlatformMismatch::FormFactor {
            required: platform.form_factor,
            current: sys.form_factor,
        });
    }
    None
}

/// Maps the registry `EditionID` (e.g. `Professional`, `CoreSingleLanguage`) to an edition.
pub fn edition_from_id(id: &str) -> Option<Edition> {
    let id = id.to_ascii_lowercase();
    if id.starts_with("server") {
        None
    } else if id.starts_with("core") {
        Some(Edition::Home)
    } else if id.contains("education") {
        // Includes "ProfessionalEducation".
        Some(Edition::Education)
    } else if id.starts_with("professional") {
        Some(Edition::Pro)
    } else if id.contains("enterprise") {
        Some(Edition::Enterprise)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(build: u32, arch: Arch, elevated: bool) -> Host {
        Host {
            build,
            arch,
            elevated,
            edition: Some(Edition::Pro),
            form_factor: None,
        }
    }

    #[test]
    fn only_windows_11_x64() {
        assert_eq!(check_host(&host(22000, Arch::X64, false)), Ok(()));
        assert_eq!(check_host(&host(26100, Arch::X64, false)), Ok(()));
        assert_eq!(
            check_host(&host(19045, Arch::X64, false)),
            Err(Blocked::NotWindows11 { build: 19045 })
        );
        assert_eq!(
            check_host(&host(26100, Arch::Arm64, false)),
            Err(Blocked::NotX64 { arch: Arch::Arm64 })
        );
        assert!(check_host(&host(26100, Arch::X86, false)).is_err());
    }

    #[test]
    fn admin_only_when_needed() {
        assert_eq!(check_admin(&host(26100, Arch::X64, false), false), Ok(()));
        assert_eq!(check_admin(&host(26100, Arch::X64, true), true), Ok(()));
        assert_eq!(
            check_admin(&host(26100, Arch::X64, false), true),
            Err(Blocked::NeedsAdmin)
        );
    }

    #[test]
    fn editions() {
        for (id, want) in [
            ("Core", Some(Edition::Home)),
            ("CoreSingleLanguage", Some(Edition::Home)),
            ("Professional", Some(Edition::Pro)),
            ("ProfessionalWorkstation", Some(Edition::Pro)),
            ("ProfessionalEducation", Some(Edition::Education)),
            ("Education", Some(Edition::Education)),
            ("Enterprise", Some(Edition::Enterprise)),
            ("IoTEnterprise", Some(Edition::Enterprise)),
            ("ServerDatacenter", None),
            ("", None),
        ] {
            assert_eq!(edition_from_id(id), want, "{id}");
        }
    }

    #[test]
    fn platform_limits() {
        let sys = SystemInfo {
            build: 22631,
            edition: Some(Edition::Home),
            form_factor: None,
        };
        let base = Platform {
            min_build: 22000,
            max_build: None,
            editions: vec![],
            form_factor: FormFactor::Any,
        };
        assert_eq!(platform_mismatch(&base, &sys), None);

        let newer = Platform {
            min_build: 26100,
            ..base.clone()
        };
        assert!(matches!(
            platform_mismatch(&newer, &sys),
            Some(PlatformMismatch::BuildTooOld {
                required: 26100,
                current: 22631
            })
        ));

        let capped = Platform {
            max_build: Some(22621),
            ..base.clone()
        };
        assert!(matches!(
            platform_mismatch(&capped, &sys),
            Some(PlatformMismatch::BuildTooNew { .. })
        ));

        let pro_only = Platform {
            editions: vec![Edition::Pro],
            ..base.clone()
        };
        assert!(matches!(
            platform_mismatch(&pro_only, &sys),
            Some(PlatformMismatch::Edition { .. })
        ));
        let unknown_edition = SystemInfo {
            edition: None,
            ..sys
        };
        assert!(platform_mismatch(&pro_only, &unknown_edition).is_some());

        // Form factor is not detected yet, so a desktop-only recipe never runs on a guess.
        let desktop = Platform {
            form_factor: FormFactor::Desktop,
            ..base.clone()
        };
        assert!(matches!(
            platform_mismatch(&desktop, &sys),
            Some(PlatformMismatch::FormFactor { current: None, .. })
        ));
    }
}
