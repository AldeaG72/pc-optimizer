//! Known optimizers and overlay apps among the installed programs.

use super::{reg_string, subkeys};
use crate::backend::windows::WindowsRegistry;
use crate::hw::{apps, HwInfo, InstalledApp};
use crate::recipe::Hive;

const UNINSTALL_ROOTS: &[(Hive, &str)] = &[
    (
        Hive::LocalMachine,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
    (
        Hive::LocalMachine,
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
    (
        Hive::CurrentUser,
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
];

pub(super) fn installed(reg: &WindowsRegistry, info: &mut HwInfo, errors: &mut Vec<String>) {
    for (hive, root) in UNINSTALL_ROOTS {
        let keys = match subkeys(*hive, root) {
            Ok(k) => k,
            Err(e) => {
                errors.push(format!("installed programs: {e}"));
                continue;
            }
        };
        for key in keys {
            let Some(name) = reg_string(reg, *hive, &format!(r"{root}\{key}"), "DisplayName")
            else {
                continue;
            };
            if let Some((id, kind)) = apps::identify(&name) {
                if !info.software.iter().any(|a| a.id == id) {
                    info.software.push(InstalledApp {
                        id: id.to_owned(),
                        name: name.trim().to_owned(),
                        kind,
                    });
                }
            }
        }
    }
    info.software.sort_by(|a, b| a.id.cmp(&b.id));
}
