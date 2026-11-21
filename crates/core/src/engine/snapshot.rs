//! Snapshots: the exact state of one target (a registry value, a service, a power plan or
//! setting, a preinstalled app), stored in the journal as the "previous" and "new" state of
//! every entry.
//!
//! Applying, undoing and recovering are all the same operation: make the target look like a
//! snapshot, then check that it does.

use serde::{Deserialize, Serialize};

use crate::backend::{reg_target, PowerValues, RegRead, ServiceState};
use crate::journal::EntryKind;
use crate::recipe::Hive;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Snapshot {
    Registry {
        hive: Hive,
        key: String,
        value: String,
        state: RegRead,
        /// Keys that did not exist, outermost first. Restoring a "missing" state removes
        /// them again (only if they are empty).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        missing_keys: Vec<String>,
    },
    Service {
        name: String,
        state: ServiceState,
    },
    /// Which power plan is active.
    PowerPlan {
        active: String,
    },
    /// One setting of one power plan.
    PowerSetting {
        scheme: String,
        subgroup: String,
        setting: String,
        values: PowerValues,
    },
    /// Whether a preinstalled app is installed for the current user.
    Appx {
        package: String,
        installed: bool,
    },
}

impl Snapshot {
    /// Human-readable target, e.g. `HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled`.
    pub fn target(&self) -> String {
        match self {
            Snapshot::Registry {
                hive, key, value, ..
            } => reg_target(*hive, key, value),
            Snapshot::Service { name, .. } => format!("service {name}"),
            Snapshot::PowerPlan { .. } => "active power plan".into(),
            Snapshot::PowerSetting {
                scheme,
                subgroup,
                setting,
                ..
            } => format!("power setting {scheme}/{subgroup}/{setting}"),
            Snapshot::Appx { package, .. } => format!("app {package}"),
        }
    }

    /// Changing this target needs administrator rights (HKLM, a service, power, apps).
    pub fn needs_admin(&self) -> bool {
        match self {
            Snapshot::Registry { hive, .. } => hive.needs_admin(),
            Snapshot::Service { .. }
            | Snapshot::PowerPlan { .. }
            | Snapshot::PowerSetting { .. }
            | Snapshot::Appx { .. } => true,
        }
    }

    pub fn kind(&self) -> EntryKind {
        match self {
            Snapshot::Registry { .. } => EntryKind::Registry,
            Snapshot::Service { .. } => EntryKind::Service,
            Snapshot::PowerPlan { .. } | Snapshot::PowerSetting { .. } => EntryKind::Power,
            Snapshot::Appx { .. } => EntryKind::Appx,
        }
    }

    /// True if `actual` (a fresh capture of the same target) is in the state this snapshot
    /// describes. "Key missing" and "value missing" both count as "not there"; a value
    /// counts if it is byte-identical or means the same (e.g. a string without its NUL).
    pub fn is_satisfied_by(&self, actual: &Snapshot) -> bool {
        match (self, actual) {
            (Snapshot::Registry { state: want, .. }, Snapshot::Registry { state: have, .. }) => {
                match (want, have) {
                    (RegRead::Present(w), RegRead::Present(h)) => {
                        w == h || w.to_value().is_some_and(|v| h.matches(&v))
                    }
                    (
                        RegRead::KeyMissing | RegRead::ValueMissing,
                        RegRead::KeyMissing | RegRead::ValueMissing,
                    ) => true,
                    _ => false,
                }
            }
            (Snapshot::Service { state: want, .. }, Snapshot::Service { state: have, .. }) => {
                want == have
            }
            (Snapshot::PowerPlan { active: want }, Snapshot::PowerPlan { active: have }) => {
                want.eq_ignore_ascii_case(have)
            }
            (
                Snapshot::PowerSetting { values: want, .. },
                Snapshot::PowerSetting { values: have, .. },
            ) => want == have,
            (
                Snapshot::Appx {
                    installed: want, ..
                },
                Snapshot::Appx {
                    installed: have, ..
                },
            ) => want == have,
            _ => false,
        }
    }
}
