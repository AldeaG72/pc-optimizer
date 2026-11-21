//! Troubleshooting (SPEC 4.8, Stage 3: 3.14): fixed repair actions the elevated helper runs.
//! They are repairs, not tweaks: nothing to undo and nothing in the journal. Each action is a
//! fixed list of steps written here, the documented Microsoft procedure; the app can only
//! name the action.
//!
//! - network reset: `netsh int ip reset` and `netsh winsock reset` (a restart finishes it);
//! - Windows Update repair: stop the Windows Update and BITS services, delete the download
//!   cache (`SoftwareDistribution`), start them again. The services' startup settings are
//!   never changed, and they are started again even if a step fails;
//! - system scan: `DISM /Online /Cleanup-Image /RestoreHealth`, then `sfc /scannow`. This can
//!   take a long time; DISM may download repair files through Windows Update.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairAction {
    NetworkReset,
    UpdateRepair,
    SystemScan,
}

impl RepairAction {
    pub fn id(self) -> &'static str {
        match self {
            RepairAction::NetworkReset => "repair.network-reset",
            RepairAction::UpdateRepair => "repair.update",
            RepairAction::SystemScan => "repair.system-scan",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "network_reset" => Some(RepairAction::NetworkReset),
            "update_repair" => Some(RepairAction::UpdateRepair),
            "system_scan" => Some(RepairAction::SystemScan),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RepairAction::NetworkReset => "network_reset",
            RepairAction::UpdateRepair => "update_repair",
            RepairAction::SystemScan => "system_scan",
        }
    }

    /// The network reset needs a restart to finish.
    pub fn needs_reboot(self) -> bool {
        matches!(self, RepairAction::NetworkReset)
    }
}

/// One step. Programs are in `System32`; folders are below the Windows folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Run `System32\<exe>` with these arguments; a non-zero exit fails the action.
    Run {
        exe: &'static str,
        args: &'static [&'static str],
    },
    /// Delete a folder below the Windows folder, if it exists (links are not followed).
    RemoveDir { below_windows: &'static str },
    /// Like `Run`, but always runs, even after a failed step (services started again).
    Always {
        exe: &'static str,
        args: &'static [&'static str],
    },
}

pub fn steps(action: RepairAction) -> Vec<Step> {
    match action {
        RepairAction::NetworkReset => vec![
            Step::Run {
                exe: "netsh.exe",
                args: &["int", "ip", "reset"],
            },
            Step::Run {
                exe: "netsh.exe",
                args: &["winsock", "reset"],
            },
        ],
        RepairAction::UpdateRepair => vec![
            Step::Run {
                exe: "net.exe",
                args: &["stop", "wuauserv"],
            },
            Step::Run {
                exe: "net.exe",
                args: &["stop", "bits"],
            },
            Step::RemoveDir {
                below_windows: "SoftwareDistribution",
            },
            Step::Always {
                exe: "net.exe",
                args: &["start", "bits"],
            },
            Step::Always {
                exe: "net.exe",
                args: &["start", "wuauserv"],
            },
        ],
        RepairAction::SystemScan => vec![
            Step::Run {
                exe: "DISM.exe",
                args: &["/Online", "/Cleanup-Image", "/RestoreHealth"],
            },
            Step::Run {
                exe: "sfc.exe",
                args: &["/scannow"],
            },
        ],
    }
}

/// Carries out steps. The real one is in the helper; tests use a fake.
pub trait StepRunner {
    /// Exit code of `System32\<exe> <args>`.
    fn run(&mut self, exe: &str, args: &[&str]) -> Result<i32, String>;
    fn remove_dir(&mut self, below_windows: &str) -> Result<(), String>;
}

/// Runs an action's steps; `Err` names the first step that failed. `Always` steps run in any
/// case. "Already stopped" (exit 2 from `net stop`) is not a failure.
pub fn run(action: RepairAction, runner: &mut dyn StepRunner) -> Result<(), String> {
    let mut failure: Option<String> = None;
    for step in steps(action) {
        match step {
            Step::Run { exe, args } if failure.is_none() => match runner.run(exe, args) {
                Ok(0) => {}
                Ok(2) if exe == "net.exe" && args.first() == Some(&"stop") => {}
                Ok(code) => failure = Some(format!("{exe} {} → {code}", args.join(" "))),
                Err(e) => failure = Some(format!("{exe}: {e}")),
            },
            Step::RemoveDir { below_windows } if failure.is_none() => {
                if let Err(e) = runner.remove_dir(below_windows) {
                    failure = Some(format!("{below_windows}: {e}"));
                }
            }
            Step::Always { exe, args } => {
                if let Err(e) = runner.run(exe, args) {
                    failure.get_or_insert(format!("{exe}: {e}"));
                }
            }
            _ => {}
        }
    }
    failure.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        log: Vec<String>,
        fail_remove: bool,
    }

    impl StepRunner for Fake {
        fn run(&mut self, exe: &str, args: &[&str]) -> Result<i32, String> {
            self.log.push(format!("{exe} {}", args.join(" ")));
            Ok(0)
        }
        fn remove_dir(&mut self, below_windows: &str) -> Result<(), String> {
            self.log.push(format!("rmdir {below_windows}"));
            if self.fail_remove {
                Err("in use".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn update_repair_starts_the_services_again_even_after_a_failure() {
        let mut fake = Fake {
            fail_remove: true,
            ..Fake::default()
        };
        let result = run(RepairAction::UpdateRepair, &mut fake);
        assert!(result.is_err());
        assert_eq!(
            fake.log,
            [
                "net.exe stop wuauserv",
                "net.exe stop bits",
                "rmdir SoftwareDistribution",
                "net.exe start bits",
                "net.exe start wuauserv",
            ]
        );
    }

    #[test]
    fn names_round_trip_and_only_fixed_actions_exist() {
        for a in [
            RepairAction::NetworkReset,
            RepairAction::UpdateRepair,
            RepairAction::SystemScan,
        ] {
            assert_eq!(RepairAction::from_name(a.name()), Some(a));
        }
        assert_eq!(RepairAction::from_name("format_c"), None);
        assert!(RepairAction::NetworkReset.needs_reboot());
        let mut fake = Fake::default();
        run(RepairAction::SystemScan, &mut fake).unwrap();
        assert_eq!(
            fake.log[0],
            "DISM.exe /Online /Cleanup-Image /RestoreHealth"
        );
    }
}
