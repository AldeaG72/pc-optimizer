//! In-memory backends for tests. They follow the same rules as Windows: names are
//! case-insensitive, writing creates missing parent keys, and a key is only removed when it
//! is empty.

use std::collections::{HashMap, HashSet};

use super::{
    reg_target, AppxBackend, AppxState, BackendError, PowerBackend, PowerValues, RegData, RegRead,
    RegistryBackend, ServiceBackend, ServiceState,
};
use crate::recipe::Hive;

type KeyId = (Hive, String);

/// A simulated registry. Compare two snapshots with `==` to check that undo restored
/// everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryRegistry {
    /// Key (lowercase path) → values (lowercase name → data).
    keys: HashMap<KeyId, HashMap<String, RegData>>,
    fail_on: HashSet<(Hive, String, String)>,
}

impl MemoryRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: starts with `value` already present.
    pub fn with_value(mut self, hive: Hive, key: &str, value: &str, data: RegData) -> Self {
        self.insert(hive, key, value, data);
        self
    }

    /// Builder: starts with an empty key (and its parents).
    pub fn with_key(mut self, hive: Hive, key: &str) -> Self {
        self.create_keys(hive, key);
        self
    }

    /// Makes every later write or delete of this value fail.
    pub fn fail_on(&mut self, hive: Hive, key: &str, value: &str) {
        self.fail_on
            .insert((hive, key.to_lowercase(), value.to_lowercase()));
    }

    pub fn key_exists(&self, hive: Hive, key: &str) -> bool {
        self.keys.contains_key(&(hive, key.to_lowercase()))
    }

    /// Equality of contents only, ignoring injected failures.
    pub fn same_contents(&self, other: &Self) -> bool {
        self.keys == other.keys
    }

    fn insert(&mut self, hive: Hive, key: &str, value: &str, data: RegData) -> Vec<String> {
        let created = self.create_keys(hive, key);
        self.keys
            .entry((hive, key.to_lowercase()))
            .or_default()
            .insert(value.to_lowercase(), data);
        created
    }

    fn create_keys(&mut self, hive: Hive, key: &str) -> Vec<String> {
        let parts: Vec<&str> = key.split('\\').collect();
        let mut created = Vec::new();
        for end in 1..=parts.len() {
            let path = parts[..end].join("\\");
            let id = (hive, path.to_lowercase());
            if let std::collections::hash_map::Entry::Vacant(e) = self.keys.entry(id) {
                e.insert(HashMap::new());
                created.push(path);
            }
        }
        created
    }

    fn check_failure(&self, hive: Hive, key: &str, value: &str) -> Result<(), BackendError> {
        if self
            .fail_on
            .contains(&(hive, key.to_lowercase(), value.to_lowercase()))
        {
            return Err(BackendError::Injected {
                target: reg_target(hive, key, value),
            });
        }
        Ok(())
    }
}

impl RegistryBackend for MemoryRegistry {
    fn read(&self, hive: Hive, key: &str, value: &str) -> Result<RegRead, BackendError> {
        Ok(match self.keys.get(&(hive, key.to_lowercase())) {
            None => RegRead::KeyMissing,
            Some(values) => match values.get(&value.to_lowercase()) {
                None => RegRead::ValueMissing,
                Some(data) => RegRead::Present(data.clone()),
            },
        })
    }

    fn write(
        &mut self,
        hive: Hive,
        key: &str,
        value: &str,
        data: &RegData,
    ) -> Result<Vec<String>, BackendError> {
        self.check_failure(hive, key, value)?;
        Ok(self.insert(hive, key, value, data.clone()))
    }

    fn delete_value(&mut self, hive: Hive, key: &str, value: &str) -> Result<bool, BackendError> {
        self.check_failure(hive, key, value)?;
        Ok(self
            .keys
            .get_mut(&(hive, key.to_lowercase()))
            .is_some_and(|values| values.remove(&value.to_lowercase()).is_some()))
    }

    fn delete_key_if_empty(&mut self, hive: Hive, key: &str) -> Result<bool, BackendError> {
        let lower = key.to_lowercase();
        if lower.is_empty() {
            return Ok(false);
        }
        let Some(values) = self.keys.get(&(hive, lower.clone())) else {
            return Ok(false);
        };
        let child_prefix = format!("{lower}\\");
        let has_children = self
            .keys
            .keys()
            .any(|(h, k)| *h == hive && k.starts_with(&child_prefix));
        if !values.is_empty() || has_children {
            return Ok(false);
        }
        self.keys.remove(&(hive, lower));
        Ok(true)
    }

    fn subkeys(&self, hive: Hive, key: &str) -> Result<Vec<String>, BackendError> {
        let prefix = format!("{}\\", key.to_lowercase());
        let mut names: Vec<String> = self
            .keys
            .keys()
            .filter(|(h, k)| *h == hive && k.starts_with(&prefix))
            .filter_map(|(_, k)| {
                let rest = &k[prefix.len()..];
                (!rest.contains('\\')).then(|| rest.to_owned())
            })
            .collect();
        names.sort();
        Ok(names)
    }

    fn values(&self, hive: Hive, key: &str) -> Result<Vec<String>, BackendError> {
        let mut names: Vec<String> = self
            .keys
            .get(&(hive, key.to_lowercase()))
            .map(|values| values.keys().filter(|n| !n.is_empty()).cloned().collect())
            .unwrap_or_default();
        names.sort();
        Ok(names)
    }
}

/// Simulated services.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryServices {
    services: HashMap<String, ServiceState>,
    fail_on: HashSet<String>,
}

impl MemoryServices {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_service(mut self, name: &str, state: ServiceState) -> Self {
        self.services.insert(name.to_lowercase(), state);
        self
    }

    pub fn fail_on(&mut self, name: &str) {
        self.fail_on.insert(name.to_lowercase());
    }

    pub fn same_contents(&self, other: &Self) -> bool {
        self.services == other.services
    }
}

impl ServiceBackend for MemoryServices {
    fn query(&self, name: &str) -> Result<Option<ServiceState>, BackendError> {
        Ok(self.services.get(&name.to_lowercase()).copied())
    }

    fn set(&mut self, name: &str, state: ServiceState) -> Result<(), BackendError> {
        if self.fail_on.contains(&name.to_lowercase()) {
            return Err(BackendError::Injected {
                target: format!("service {name}"),
            });
        }
        match self.services.get_mut(&name.to_lowercase()) {
            Some(current) => {
                *current = state;
                Ok(())
            }
            None => Err(BackendError::ServiceNotFound { name: name.into() }),
        }
    }
}

type SettingId = (String, String, String);

fn setting_id(scheme: &str, subgroup: &str, setting: &str) -> SettingId {
    (
        scheme.to_lowercase(),
        subgroup.to_lowercase(),
        setting.to_lowercase(),
    )
}

/// Simulated power plans.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryPower {
    active: String,
    schemes: HashSet<String>,
    settings: HashMap<SettingId, PowerValues>,
    fail: bool,
}

impl MemoryPower {
    /// One plan, active.
    pub fn new(active: &str) -> Self {
        Self {
            active: active.to_lowercase(),
            schemes: HashSet::from([active.to_lowercase()]),
            ..Self::default()
        }
    }

    pub fn with_scheme(mut self, scheme: &str) -> Self {
        self.schemes.insert(scheme.to_lowercase());
        self
    }

    pub fn with_setting(
        mut self,
        scheme: &str,
        subgroup: &str,
        setting: &str,
        values: PowerValues,
    ) -> Self {
        self.settings
            .insert(setting_id(scheme, subgroup, setting), values);
        self
    }

    /// Makes every later change fail.
    pub fn fail_writes(&mut self) {
        self.fail = true;
    }

    pub fn same_contents(&self, other: &Self) -> bool {
        self.active == other.active
            && self.schemes == other.schemes
            && self.settings == other.settings
    }

    fn check(&self, target: &str) -> Result<(), BackendError> {
        if self.fail {
            Err(BackendError::Injected {
                target: target.into(),
            })
        } else {
            Ok(())
        }
    }
}

impl PowerBackend for MemoryPower {
    fn active_scheme(&self) -> Result<String, BackendError> {
        Ok(self.active.clone())
    }

    fn scheme_exists(&self, scheme: &str) -> Result<bool, BackendError> {
        Ok(self.schemes.contains(&scheme.to_lowercase()))
    }

    fn set_active_scheme(&mut self, scheme: &str) -> Result<(), BackendError> {
        self.check("power plan")?;
        if !self.schemes.contains(&scheme.to_lowercase()) {
            return Err(BackendError::NotFound {
                target: format!("power plan {scheme}"),
            });
        }
        self.active = scheme.to_lowercase();
        Ok(())
    }

    fn read_setting(
        &self,
        scheme: &str,
        subgroup: &str,
        setting: &str,
    ) -> Result<Option<PowerValues>, BackendError> {
        Ok(self
            .settings
            .get(&setting_id(scheme, subgroup, setting))
            .copied())
    }

    fn write_setting(
        &mut self,
        scheme: &str,
        subgroup: &str,
        setting: &str,
        values: PowerValues,
    ) -> Result<(), BackendError> {
        self.check(&format!("power setting {setting}"))?;
        match self
            .settings
            .get_mut(&setting_id(scheme, subgroup, setting))
        {
            Some(v) => {
                *v = values;
                Ok(())
            }
            None => Err(BackendError::NotFound {
                target: format!("power setting {setting}"),
            }),
        }
    }
}

/// Simulated preinstalled apps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryAppx {
    apps: HashMap<String, AppxState>,
    fail_on: HashSet<String>,
}

impl MemoryAppx {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_app(mut self, package: &str, state: AppxState) -> Self {
        self.apps.insert(package.to_lowercase(), state);
        self
    }

    pub fn fail_on(&mut self, package: &str) {
        self.fail_on.insert(package.to_lowercase());
    }

    pub fn same_contents(&self, other: &Self) -> bool {
        self.apps == other.apps
    }
}

impl AppxBackend for MemoryAppx {
    fn query(&self, package: &str) -> Result<AppxState, BackendError> {
        Ok(self
            .apps
            .get(&package.to_lowercase())
            .copied()
            .unwrap_or(AppxState::Missing))
    }

    fn set_installed(&mut self, package: &str, installed: bool) -> Result<(), BackendError> {
        let key = package.to_lowercase();
        if self.fail_on.contains(&key) {
            return Err(BackendError::Injected {
                target: format!("app {package}"),
            });
        }
        match self.apps.get(&key) {
            Some(AppxState::Installed | AppxState::Removed) => {
                let state = if installed {
                    AppxState::Installed
                } else {
                    AppxState::Removed
                };
                self.apps.insert(key, state);
                Ok(())
            }
            _ => Err(BackendError::NotFound {
                target: format!("app {package}"),
            }),
        }
    }
}
