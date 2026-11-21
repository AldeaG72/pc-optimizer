//! Access to Windows settings, behind traits.
//!
//! Each backend has a real implementation (`windows` module, Windows only) and an in-memory
//! one (`memory` module) used by tests on any OS. The engine only sees the traits.
//!
//! Previous states are captured *raw* (registry type + bytes, full service start state) so
//! that undo restores exactly what was there, even values this app would never write.

pub mod memory;
#[cfg(windows)]
pub mod windows;

use serde::{Deserialize, Serialize};

use crate::recipe::{Hive, RegValue};

/// Registry value types this crate encodes or decodes (winnt.h).
pub const REG_SZ: u32 = 1;
pub const REG_DWORD: u32 = 4;
pub const REG_QWORD: u32 = 11;

/// A registry value exactly as stored: its type and raw bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegData {
    pub reg_type: u32,
    pub bytes: Vec<u8>,
}

impl RegData {
    /// Encodes a recipe value the way Windows stores it (strings are UTF-16LE + NUL).
    pub fn from_value(value: &RegValue) -> Self {
        match value {
            RegValue::Dword(v) => Self {
                reg_type: REG_DWORD,
                bytes: v.to_le_bytes().to_vec(),
            },
            RegValue::Qword(v) => Self {
                reg_type: REG_QWORD,
                bytes: v.to_le_bytes().to_vec(),
            },
            RegValue::String(s) => Self {
                reg_type: REG_SZ,
                bytes: s
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            },
        }
    }

    /// Decodes into a recipe value, or `None` for types/sizes a recipe cannot express.
    pub fn to_value(&self) -> Option<RegValue> {
        match self.reg_type {
            REG_DWORD => Some(RegValue::Dword(u32::from_le_bytes(
                self.bytes.as_slice().try_into().ok()?,
            ))),
            REG_QWORD => Some(RegValue::Qword(u64::from_le_bytes(
                self.bytes.as_slice().try_into().ok()?,
            ))),
            REG_SZ => {
                let (pairs, rest) = self.bytes.as_chunks::<2>();
                if !rest.is_empty() {
                    return None;
                }
                let mut units: Vec<u16> = pairs.iter().map(|p| u16::from_le_bytes(*p)).collect();
                while units.last() == Some(&0) {
                    units.pop();
                }
                String::from_utf16(&units).ok().map(RegValue::String)
            }
            _ => None,
        }
    }

    /// True if this stored value means the same as `value` (same type and content).
    pub fn matches(&self, value: &RegValue) -> bool {
        self.to_value().as_ref() == Some(value)
    }
}

/// Result of reading one registry value. "Missing" is never confused with 0.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "data")]
pub enum RegRead {
    /// The key itself does not exist.
    KeyMissing,
    /// The key exists but has no value with this name.
    ValueMissing,
    Present(RegData),
}

/// How Windows starts a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStart {
    Boot,
    System,
    Automatic,
    Manual,
    Disabled,
}

/// The full start configuration of a service, enough to restore it exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServiceState {
    pub start: ServiceStart,
    /// "Automatic (Delayed Start)". Windows ignores it unless `start` is `Automatic`.
    pub delayed_auto_start: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BackendError {
    #[error("administrator rights are needed to change {target}")]
    AccessDenied { target: String },

    #[error("{target} is outside the test key; refusing to change it")]
    OutsideSandbox { target: String },

    #[error("this backend is read-only; refusing to change {target}")]
    ReadOnly { target: String },

    #[error("service `{name}` does not exist")]
    ServiceNotFound { name: String },

    #[error("{op} failed for {target}: {message} (code {code:#010x})")]
    Os {
        op: &'static str,
        target: String,
        code: u32,
        message: String,
    },

    /// Raised by in-memory backends to test failure handling.
    #[error("simulated failure for {target}")]
    Injected { target: String },
}

/// Reads and writes registry values.
pub trait RegistryBackend {
    fn read(&self, hive: Hive, key: &str, value: &str) -> Result<RegRead, BackendError>;

    /// Writes a value, creating missing keys. Returns the keys it created, outermost first,
    /// so undo can remove exactly those.
    fn write(
        &mut self,
        hive: Hive,
        key: &str,
        value: &str,
        data: &RegData,
    ) -> Result<Vec<String>, BackendError>;

    /// Deletes a value. Returns `false` if it (or its key) did not exist.
    fn delete_value(&mut self, hive: Hive, key: &str, value: &str) -> Result<bool, BackendError>;

    /// Deletes a key only if it has no values and no subkeys. Returns whether it was deleted.
    fn delete_key_if_empty(&mut self, hive: Hive, key: &str) -> Result<bool, BackendError>;
}

/// Reads and changes how services start.
pub trait ServiceBackend {
    /// `None` means the service does not exist on this PC.
    fn query(&self, name: &str) -> Result<Option<ServiceState>, BackendError>;

    fn set(&mut self, name: &str, state: ServiceState) -> Result<(), BackendError>;
}

/// Any registry backend with every change refused: the app reads the real registry with
/// this, so only the elevated helper can ever write.
pub struct ReadOnlyRegistry<R>(pub R);

impl<R: RegistryBackend> RegistryBackend for ReadOnlyRegistry<R> {
    fn read(&self, hive: Hive, key: &str, value: &str) -> Result<RegRead, BackendError> {
        self.0.read(hive, key, value)
    }

    fn write(
        &mut self,
        hive: Hive,
        key: &str,
        value: &str,
        _data: &RegData,
    ) -> Result<Vec<String>, BackendError> {
        Err(BackendError::ReadOnly {
            target: reg_target(hive, key, value),
        })
    }

    fn delete_value(&mut self, hive: Hive, key: &str, value: &str) -> Result<bool, BackendError> {
        Err(BackendError::ReadOnly {
            target: reg_target(hive, key, value),
        })
    }

    fn delete_key_if_empty(&mut self, hive: Hive, key: &str) -> Result<bool, BackendError> {
        Err(BackendError::ReadOnly {
            target: reg_target(hive, key, ""),
        })
    }
}

/// `HKCU\Some\Key\Value`, for messages.
pub fn reg_target(hive: Hive, key: &str, value: &str) -> String {
    if value.is_empty() {
        format!("{hive}\\{key}")
    } else {
        format!("{hive}\\{key}\\{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip_through_raw_bytes() {
        for v in [
            RegValue::Dword(0),
            RegValue::Dword(u32::MAX),
            RegValue::Qword(u64::MAX),
            RegValue::String(String::new()),
            RegValue::String("506".into()),
            RegValue::String("ăîșțâ €".into()),
        ] {
            let raw = RegData::from_value(&v);
            assert_eq!(raw.to_value(), Some(v.clone()));
            assert!(raw.matches(&v));
        }
    }

    #[test]
    fn strings_are_nul_terminated_utf16() {
        let raw = RegData::from_value(&RegValue::String("A".into()));
        assert_eq!(raw.reg_type, REG_SZ);
        assert_eq!(raw.bytes, [0x41, 0, 0, 0]);
        // Windows may also store a string without the terminator.
        let unterminated = RegData {
            reg_type: REG_SZ,
            bytes: vec![0x41, 0],
        };
        assert!(unterminated.matches(&RegValue::String("A".into())));
    }

    #[test]
    fn type_and_size_must_match() {
        let dword_zero = RegData::from_value(&RegValue::Dword(0));
        assert!(!dword_zero.matches(&RegValue::Qword(0)));
        assert!(!dword_zero.matches(&RegValue::String("0".into())));
        let short = RegData {
            reg_type: REG_DWORD,
            bytes: vec![0, 0],
        };
        assert_eq!(short.to_value(), None);
        let binary = RegData {
            reg_type: 3,
            bytes: vec![0, 0, 0, 0],
        };
        assert_eq!(binary.to_value(), None);
    }

    #[test]
    fn read_only_registry_reads_but_never_writes() {
        let inner = memory::MemoryRegistry::new().with_value(
            Hive::CurrentUser,
            "K",
            "V",
            RegData::from_value(&RegValue::Dword(1)),
        );
        let mut ro = ReadOnlyRegistry(inner);
        assert!(matches!(
            ro.read(Hive::CurrentUser, "K", "V"),
            Ok(RegRead::Present(_))
        ));
        let data = RegData::from_value(&RegValue::Dword(2));
        assert!(matches!(
            ro.write(Hive::CurrentUser, "K", "V", &data),
            Err(BackendError::ReadOnly { .. })
        ));
        assert!(ro.delete_value(Hive::CurrentUser, "K", "V").is_err());
        assert!(ro.delete_key_if_empty(Hive::CurrentUser, "K").is_err());
        assert_eq!(
            ro.0.read(Hive::CurrentUser, "K", "V").unwrap(),
            RegRead::Present(RegData::from_value(&RegValue::Dword(1)))
        );
    }
}
