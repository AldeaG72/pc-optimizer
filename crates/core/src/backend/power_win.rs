//! Real power plans, through the same API `powercfg` uses (powrprof).
//!
//! Like the other real backends, tests on the development PC must use
//! [`WindowsPower::read_only`] (CLAUDE.md safety rule 1).

use windows::core::GUID;
use windows::Win32::Foundation::{LocalFree, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, HLOCAL};
use windows::Win32::System::Power::{
    PowerEnumerate, PowerGetActiveScheme, PowerReadACValueIndex, PowerReadDCValueIndex,
    PowerSetActiveScheme, PowerWriteACValueIndex, PowerWriteDCValueIndex, ACCESS_SCHEME,
};

use super::{BackendError, PowerBackend, PowerValues};

#[derive(Debug, Clone)]
pub struct WindowsPower {
    read_only: bool,
}

impl WindowsPower {
    /// Can change power plans. Only for the elevated helper and `pco apply` / `pco undo`.
    pub fn system() -> Self {
        Self { read_only: false }
    }

    /// Every change is refused. For the app and for tests on the development PC.
    pub fn read_only() -> Self {
        Self { read_only: true }
    }

    fn check_writable(&self, target: &str) -> Result<(), BackendError> {
        if self.read_only {
            Err(BackendError::ReadOnly {
                target: target.into(),
            })
        } else {
            Ok(())
        }
    }
}

/// `xxxxxxxx-xxxx-…` (lowercase, validated by the recipe loader) → GUID.
fn parse(text: &str) -> Result<GUID, BackendError> {
    let hex: String = text
        .trim_matches(['{', '}'])
        .chars()
        .filter(|c| *c != '-')
        .collect();
    match (hex.len(), u128::from_str_radix(&hex, 16)) {
        (32, Ok(v)) => Ok(GUID::from_u128(v)),
        _ => Err(BackendError::NotFound {
            target: format!("power GUID {text}"),
        }),
    }
}

fn format(guid: &GUID) -> String {
    format!("{guid:?}").to_ascii_lowercase()
}

/// powrprof returns either `WIN32_ERROR` or a plain `u32`, depending on the function.
trait Code {
    fn code(self) -> u32;
}

impl Code for u32 {
    fn code(self) -> u32 {
        self
    }
}

impl Code for windows::Win32::Foundation::WIN32_ERROR {
    fn code(self) -> u32 {
        self.0
    }
}

fn os(op: &'static str, target: &str, code: u32) -> BackendError {
    BackendError::Os {
        op,
        target: target.into(),
        code,
        message: windows::core::Error::from(
            windows::Win32::Foundation::WIN32_ERROR(code).to_hresult(),
        )
        .message(),
    }
}

impl PowerBackend for WindowsPower {
    fn active_scheme(&self) -> Result<String, BackendError> {
        let mut guid: *mut GUID = std::ptr::null_mut();
        // SAFETY: Windows allocates the GUID; freed below with LocalFree.
        let rc = unsafe { PowerGetActiveScheme(None, &mut guid) };
        if rc != ERROR_SUCCESS || guid.is_null() {
            return Err(os("read active power plan", "power plan", rc.0));
        }
        // SAFETY: non-null, set by PowerGetActiveScheme.
        let scheme = unsafe { *guid };
        // SAFETY: allocated by PowerGetActiveScheme with LocalAlloc.
        unsafe { LocalFree(Some(HLOCAL(guid.cast()))) };
        Ok(format(&scheme))
    }

    fn scheme_exists(&self, scheme: &str) -> Result<bool, BackendError> {
        let wanted = parse(scheme)?;
        for index in 0u32.. {
            let mut guid = GUID::zeroed();
            let mut size = std::mem::size_of::<GUID>() as u32;
            // SAFETY: the buffer is one GUID of `size` bytes.
            let rc = unsafe {
                PowerEnumerate(
                    None,
                    None,
                    None,
                    ACCESS_SCHEME,
                    index,
                    Some((&raw mut guid).cast()),
                    &mut size,
                )
            };
            if rc != ERROR_SUCCESS {
                // ERROR_NO_MORE_ITEMS ends the list.
                return Ok(false);
            }
            if guid == wanted {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn set_active_scheme(&mut self, scheme: &str) -> Result<(), BackendError> {
        self.check_writable("active power plan")?;
        let guid = parse(scheme)?;
        // SAFETY: `guid` outlives the call.
        let rc = unsafe { PowerSetActiveScheme(None, Some(&guid)) };
        if rc != ERROR_SUCCESS {
            return Err(os("set active power plan", scheme, rc.0));
        }
        Ok(())
    }

    fn read_setting(
        &self,
        scheme: &str,
        subgroup: &str,
        setting: &str,
    ) -> Result<Option<PowerValues>, BackendError> {
        let (s, g, p) = (parse(scheme)?, parse(subgroup)?, parse(setting)?);
        let (mut ac, mut dc) = (0u32, 0u32);
        // SAFETY: the GUIDs outlive the calls; `ac`/`dc` are valid out-pointers.
        let rc_ac = unsafe { PowerReadACValueIndex(None, Some(&s), Some(&g), Some(&p), &mut ac) };
        if rc_ac.code() == ERROR_FILE_NOT_FOUND.0 {
            return Ok(None);
        }
        if rc_ac.code() != ERROR_SUCCESS.0 {
            return Err(os("read power setting", setting, rc_ac.code()));
        }
        // SAFETY: as above.
        let rc_dc = unsafe { PowerReadDCValueIndex(None, Some(&s), Some(&g), Some(&p), &mut dc) };
        if rc_dc.code() != ERROR_SUCCESS.0 {
            return Err(os("read power setting", setting, rc_dc.code()));
        }
        Ok(Some(PowerValues { ac, dc }))
    }

    fn write_setting(
        &mut self,
        scheme: &str,
        subgroup: &str,
        setting: &str,
        values: PowerValues,
    ) -> Result<(), BackendError> {
        self.check_writable(&format!("power setting {setting}"))?;
        let (s, g, p) = (parse(scheme)?, parse(subgroup)?, parse(setting)?);
        // SAFETY: the GUIDs outlive the calls.
        let rc = unsafe { PowerWriteACValueIndex(None, &s, Some(&g), Some(&p), values.ac) };
        if rc.code() != ERROR_SUCCESS.0 {
            return Err(os("write power setting", setting, rc.code()));
        }
        // SAFETY: as above.
        let rc = unsafe { PowerWriteDCValueIndex(None, &s, Some(&g), Some(&p), values.dc) };
        if rc.code() != ERROR_SUCCESS.0 {
            return Err(os("write power setting", setting, rc.code()));
        }
        // A plan's new values take effect when it is (re)activated.
        if self.active_scheme()? == scheme.to_ascii_lowercase() {
            // SAFETY: `s` outlives the call.
            let rc = unsafe { PowerSetActiveScheme(None, Some(&s)) };
            if rc != ERROR_SUCCESS {
                return Err(os("apply power setting", setting, rc.0));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guids_round_trip() {
        let text = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
        assert_eq!(format(&parse(text).unwrap()), text);
        assert!(parse("nope").is_err());
    }

    /// Read-only: safe on the development PC.
    #[test]
    fn reads_the_active_plan_and_refuses_changes() {
        let mut power = WindowsPower::read_only();
        let active = power.active_scheme().unwrap();
        assert!(power.scheme_exists(&active).unwrap());
        assert!(matches!(
            power.set_active_scheme(&active),
            Err(BackendError::ReadOnly { .. })
        ));
        // Processor subgroup / minimum processor state exists on every PC.
        let values = power
            .read_setting(
                &active,
                "54533251-82be-4824-96c1-47b60b740d00",
                "893dee8e-2bef-41e0-89c6-b55d0929964c",
            )
            .unwrap();
        assert!(values.is_some());
    }
}
