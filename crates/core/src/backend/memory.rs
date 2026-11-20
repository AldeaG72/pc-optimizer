//! In-memory backends for tests. They follow the same rules as Windows: names are
//! case-insensitive, writing creates missing parent keys, and a key is only removed when it
//! is empty.

use std::collections::{HashMap, HashSet};

use super::{
    reg_target, BackendError, RegData, RegRead, RegistryBackend, ServiceBackend, ServiceState,
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
