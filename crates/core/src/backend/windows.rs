//! Real Windows backends (Win32 registry and Service Control Manager).
//!
//! Tests must use [`WindowsRegistry::test_sandbox`] and [`WindowsServices::read_only`]:
//! CLAUDE.md safety rule 1 forbids real tweaks on the development PC, and these
//! constructors make that a hard error instead of a convention.

use windows::core::{Error as WinError, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER, ERROR_MORE_DATA,
    ERROR_NO_MORE_ITEMS, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SUCCESS, WIN32_ERROR,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyExW, RegDeleteValueW, RegEnumKeyExW, RegOpenKeyExW,
    RegQueryInfoKeyW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_ENUMERATE_SUB_KEYS, KEY_QUERY_VALUE, KEY_SET_VALUE, KEY_WOW64_64KEY,
    REG_CREATED_NEW_KEY, REG_CREATE_KEY_DISPOSITION, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS,
    REG_VALUE_TYPE,
};
use windows::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, CloseServiceHandle, OpenSCManagerW, OpenServiceW,
    QueryServiceConfig2W, QueryServiceConfigW, ENUM_SERVICE_TYPE, QUERY_SERVICE_CONFIGW, SC_HANDLE,
    SC_MANAGER_CONNECT, SERVICE_AUTO_START, SERVICE_BOOT_START, SERVICE_CHANGE_CONFIG,
    SERVICE_CONFIG_DELAYED_AUTO_START_INFO, SERVICE_DELAYED_AUTO_START_INFO, SERVICE_DEMAND_START,
    SERVICE_DISABLED, SERVICE_ERROR, SERVICE_NO_CHANGE, SERVICE_QUERY_CONFIG, SERVICE_START_TYPE,
    SERVICE_SYSTEM_START,
};

use super::{
    reg_target, BackendError, RegData, RegRead, RegistryBackend, ServiceBackend, ServiceStart,
    ServiceState,
};
use crate::recipe::Hive;

/// The only key tests may change (CLAUDE.md, safety rule 1).
pub const TEST_KEY: &str = r"Software\PcOptimizerTest";

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WindowsRegistry {
    sandboxed: bool,
}

impl WindowsRegistry {
    /// Full access to HKCU and HKLM. Only for the real `pco apply` / `pco undo`.
    pub fn system() -> Self {
        Self { sandboxed: false }
    }

    /// Writes and deletes are refused unless they are below `HKCU\Software\PcOptimizerTest`.
    /// Reads are allowed anywhere.
    pub fn test_sandbox() -> Self {
        Self { sandboxed: true }
    }

    fn check_writable(&self, hive: Hive, key: &str, value: &str) -> Result<(), BackendError> {
        if self.sandboxed && !in_test_key(hive, key) {
            return Err(BackendError::OutsideSandbox {
                target: reg_target(hive, key, value),
            });
        }
        Ok(())
    }
}

fn in_test_key(hive: Hive, key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    let test = TEST_KEY.to_ascii_lowercase();
    hive == Hive::CurrentUser && (key == test || key.starts_with(&format!("{test}\\")))
}

fn root(hive: Hive) -> HKEY {
    match hive {
        Hive::CurrentUser => HKEY_CURRENT_USER,
        Hive::LocalMachine => HKEY_LOCAL_MACHINE,
    }
}

/// Closes the key handle when dropped.
struct OpenKey(HKEY);

impl Drop for OpenKey {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful RegOpenKeyExW/RegCreateKeyExW and is
        // closed exactly once, here.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

fn os_error(op: &'static str, target: String, rc: WIN32_ERROR) -> BackendError {
    if rc == ERROR_ACCESS_DENIED {
        return BackendError::AccessDenied { target };
    }
    BackendError::Os {
        op,
        target,
        code: rc.0,
        message: WinError::from(rc.to_hresult()).message(),
    }
}

/// Opens an existing key. `Ok(None)` if it does not exist.
fn open_key(hive: Hive, key: &str, access: REG_SAM_FLAGS) -> Result<Option<OpenKey>, BackendError> {
    let mut handle = HKEY::default();
    let path = HSTRING::from(key);
    // SAFETY: `path` outlives the call and `handle` is a valid out-pointer.
    let rc = unsafe {
        RegOpenKeyExW(
            root(hive),
            &path,
            Some(0),
            access | KEY_WOW64_64KEY,
            &mut handle,
        )
    };
    match rc {
        ERROR_SUCCESS => Ok(Some(OpenKey(handle))),
        ERROR_FILE_NOT_FOUND => Ok(None),
        rc => Err(os_error("open key", reg_target(hive, key, ""), rc)),
    }
}

/// Creates (or opens) one key path. Returns the handle and whether it was newly created.
fn create_key(
    hive: Hive,
    key: &str,
    access: REG_SAM_FLAGS,
) -> Result<(OpenKey, bool), BackendError> {
    let mut handle = HKEY::default();
    let mut disposition = REG_CREATE_KEY_DISPOSITION::default();
    let path = HSTRING::from(key);
    // SAFETY: `path` outlives the call; `handle` and `disposition` are valid out-pointers.
    let rc = unsafe {
        RegCreateKeyExW(
            root(hive),
            &path,
            Some(0),
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            access | KEY_WOW64_64KEY,
            None,
            &mut handle,
            Some(&mut disposition),
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(os_error("create key", reg_target(hive, key, ""), rc));
    }
    Ok((OpenKey(handle), disposition == REG_CREATED_NEW_KEY))
}

impl RegistryBackend for WindowsRegistry {
    fn read(&self, hive: Hive, key: &str, value: &str) -> Result<RegRead, BackendError> {
        let Some(k) = open_key(hive, key, KEY_QUERY_VALUE)? else {
            return Ok(RegRead::KeyMissing);
        };
        let name = HSTRING::from(value);
        let target = || reg_target(hive, key, value);
        let mut size: u32 = 0;
        let mut reg_type = REG_VALUE_TYPE::default();

        // SAFETY: valid out-pointers; no data buffer, so Windows only reports the size.
        let rc = unsafe {
            RegQueryValueExW(k.0, &name, None, Some(&mut reg_type), None, Some(&mut size))
        };
        match rc {
            ERROR_SUCCESS => {}
            ERROR_FILE_NOT_FOUND => return Ok(RegRead::ValueMissing),
            rc => return Err(os_error("read value", target(), rc)),
        }

        // The value can change size between calls; retry a few times.
        for _ in 0..4 {
            let mut bytes = vec![0u8; size as usize];
            let mut len = size;
            // SAFETY: `bytes` has exactly `len` writable bytes.
            let rc = unsafe {
                RegQueryValueExW(
                    k.0,
                    &name,
                    None,
                    Some(&mut reg_type),
                    Some(bytes.as_mut_ptr()),
                    Some(&mut len),
                )
            };
            match rc {
                ERROR_SUCCESS => {
                    bytes.truncate(len as usize);
                    return Ok(RegRead::Present(RegData {
                        reg_type: reg_type.0,
                        bytes,
                    }));
                }
                ERROR_MORE_DATA => size = len,
                ERROR_FILE_NOT_FOUND => return Ok(RegRead::ValueMissing),
                rc => return Err(os_error("read value", target(), rc)),
            }
        }
        Err(os_error("read value", target(), ERROR_MORE_DATA))
    }

    fn write(
        &mut self,
        hive: Hive,
        key: &str,
        value: &str,
        data: &RegData,
    ) -> Result<Vec<String>, BackendError> {
        self.check_writable(hive, key, value)?;

        // Create the path one level at a time to know exactly which keys are new.
        let parts: Vec<&str> = key.split('\\').collect();
        let mut created = Vec::new();
        let mut last = None;
        for end in 1..=parts.len() {
            let path = parts[..end].join("\\");
            let access = if end == parts.len() {
                KEY_SET_VALUE
            } else {
                KEY_QUERY_VALUE
            };
            let (handle, is_new) = create_key(hive, &path, access)?;
            if is_new {
                created.push(path);
            }
            last = Some(handle);
        }
        let Some(k) = last else {
            return Err(os_error(
                "create key",
                reg_target(hive, key, value),
                ERROR_FILE_NOT_FOUND,
            ));
        };

        let name = HSTRING::from(value);
        // SAFETY: `name` outlives the call; the slice is passed with its own length.
        let rc = unsafe {
            RegSetValueExW(
                k.0,
                &name,
                Some(0),
                REG_VALUE_TYPE(data.reg_type),
                Some(&data.bytes),
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(os_error("write value", reg_target(hive, key, value), rc));
        }
        Ok(created)
    }

    fn delete_value(&mut self, hive: Hive, key: &str, value: &str) -> Result<bool, BackendError> {
        self.check_writable(hive, key, value)?;
        let Some(k) = open_key(hive, key, KEY_SET_VALUE)? else {
            return Ok(false);
        };
        let name = HSTRING::from(value);
        // SAFETY: `name` outlives the call.
        match unsafe { RegDeleteValueW(k.0, &name) } {
            ERROR_SUCCESS => Ok(true),
            ERROR_FILE_NOT_FOUND => Ok(false),
            rc => Err(os_error("delete value", reg_target(hive, key, value), rc)),
        }
    }

    fn delete_key_if_empty(&mut self, hive: Hive, key: &str) -> Result<bool, BackendError> {
        self.check_writable(hive, key, "")?;
        if key.is_empty() {
            return Ok(false);
        }
        let Some(k) = open_key(hive, key, KEY_QUERY_VALUE)? else {
            return Ok(false);
        };
        let mut subkeys: u32 = 0;
        let mut values: u32 = 0;
        // SAFETY: only the two requested counters are written.
        let rc = unsafe {
            RegQueryInfoKeyW(
                k.0,
                None,
                None,
                None,
                Some(&mut subkeys),
                None,
                None,
                Some(&mut values),
                None,
                None,
                None,
                None,
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(os_error("inspect key", reg_target(hive, key, ""), rc));
        }
        drop(k);
        if subkeys > 0 || values > 0 {
            return Ok(false);
        }
        let path = HSTRING::from(key);
        // SAFETY: `path` outlives the call.
        match unsafe { RegDeleteKeyExW(root(hive), &path, KEY_WOW64_64KEY.0, Some(0)) } {
            ERROR_SUCCESS => Ok(true),
            ERROR_FILE_NOT_FOUND => Ok(false),
            rc => Err(os_error("delete key", reg_target(hive, key, ""), rc)),
        }
    }

    fn subkeys(&self, hive: Hive, key: &str) -> Result<Vec<String>, BackendError> {
        let Some(k) = open_key(hive, key, KEY_ENUMERATE_SUB_KEYS)? else {
            return Ok(Vec::new());
        };
        let mut names = Vec::new();
        for index in 0u32.. {
            // Key names are at most 255 characters.
            let mut buf = [0u16; 256];
            let mut len = buf.len() as u32;
            // SAFETY: `buf` has `len` writable UTF-16 units; the optional outputs are unused.
            let rc = unsafe {
                RegEnumKeyExW(
                    k.0,
                    index,
                    Some(PWSTR(buf.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            match rc {
                ERROR_SUCCESS => {
                    names.push(String::from_utf16_lossy(&buf[..len as usize]));
                }
                ERROR_NO_MORE_ITEMS => break,
                rc => return Err(os_error("list subkeys", reg_target(hive, key, ""), rc)),
            }
        }
        Ok(names)
    }
}

// ---------------------------------------------------------------------------
// Services
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WindowsServices {
    read_only: bool,
}

impl WindowsServices {
    /// Can change services. Only for the real `pco apply` / `pco undo`.
    pub fn system() -> Self {
        Self { read_only: false }
    }

    /// Every change is refused. For tests on the development PC.
    pub fn read_only() -> Self {
        Self { read_only: true }
    }
}

/// Closes the service handle when dropped.
struct ScHandle(SC_HANDLE);

impl Drop for ScHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful OpenSCManagerW/OpenServiceW and is
        // closed exactly once, here.
        let _ = unsafe { CloseServiceHandle(self.0) };
    }
}

fn service_error(op: &'static str, name: &str, e: &WinError) -> BackendError {
    let target = format!("service {name}");
    if e.code() == ERROR_SERVICE_DOES_NOT_EXIST.to_hresult() {
        return BackendError::ServiceNotFound { name: name.into() };
    }
    if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
        return BackendError::AccessDenied { target };
    }
    BackendError::Os {
        op,
        target,
        // HRESULT bits, shown as hex in messages.
        code: e.code().0 as u32,
        message: e.message(),
    }
}

/// Opens a service. `Ok(None)` if it does not exist.
fn open_service(name: &str, access: u32) -> Result<Option<ScHandle>, BackendError> {
    // SAFETY: null machine/database names mean "this PC, active database".
    let scm = unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }
        .map_err(|e| service_error("open service manager", name, &e))?;
    let scm = ScHandle(scm);
    let wide = HSTRING::from(name);
    // SAFETY: `scm` is open and `wide` outlives the call.
    match unsafe { OpenServiceW(scm.0, &wide, access) } {
        Ok(h) => Ok(Some(ScHandle(h))),
        Err(e) if e.code() == ERROR_SERVICE_DOES_NOT_EXIST.to_hresult() => Ok(None),
        Err(e) => Err(service_error("open service", name, &e)),
    }
}

fn start_type(start: SERVICE_START_TYPE) -> Option<ServiceStart> {
    Some(match start {
        SERVICE_BOOT_START => ServiceStart::Boot,
        SERVICE_SYSTEM_START => ServiceStart::System,
        SERVICE_AUTO_START => ServiceStart::Automatic,
        SERVICE_DEMAND_START => ServiceStart::Manual,
        SERVICE_DISABLED => ServiceStart::Disabled,
        _ => return None,
    })
}

fn win_start_type(start: ServiceStart) -> SERVICE_START_TYPE {
    match start {
        ServiceStart::Boot => SERVICE_BOOT_START,
        ServiceStart::System => SERVICE_SYSTEM_START,
        ServiceStart::Automatic => SERVICE_AUTO_START,
        ServiceStart::Manual => SERVICE_DEMAND_START,
        ServiceStart::Disabled => SERVICE_DISABLED,
    }
}

impl ServiceBackend for WindowsServices {
    fn query(&self, name: &str) -> Result<Option<ServiceState>, BackendError> {
        let Some(svc) = open_service(name, SERVICE_QUERY_CONFIG)? else {
            return Ok(None);
        };

        let mut needed: u32 = 0;
        // SAFETY: no buffer; Windows reports the size it needs.
        match unsafe { QueryServiceConfigW(svc.0, None, 0, &mut needed) } {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_INSUFFICIENT_BUFFER.to_hresult() => {}
            Err(e) => return Err(service_error("query service", name, &e)),
        }
        // u64 storage keeps the struct correctly aligned.
        let words = (needed as usize).div_ceil(8).max(1);
        let mut buf = vec![0u64; words];
        let size = u32::try_from(words * 8).unwrap_or(u32::MAX);
        let config = buf.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
        // SAFETY: `buf` is aligned for QUERY_SERVICE_CONFIGW and has `size` bytes.
        unsafe { QueryServiceConfigW(svc.0, Some(config), size, &mut needed) }
            .map_err(|e| service_error("query service", name, &e))?;
        // SAFETY: the call succeeded, so `buf` now holds a valid QUERY_SERVICE_CONFIGW.
        let raw_start = unsafe { (*config).dwStartType };
        let start = start_type(raw_start).ok_or_else(|| BackendError::Os {
            op: "query service",
            target: format!("service {name}"),
            code: raw_start.0,
            message: "unknown start type".into(),
        })?;

        let mut info = [0u8; std::mem::size_of::<SERVICE_DELAYED_AUTO_START_INFO>()];
        // SAFETY: `info` is exactly the size of SERVICE_DELAYED_AUTO_START_INFO.
        let delayed = unsafe {
            QueryServiceConfig2W(
                svc.0,
                SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
                Some(&mut info),
                &mut needed,
            )
        }
        // Drivers have no delayed-start setting; treat as "not delayed".
        .map(|()| i32::from_ne_bytes(info) != 0)
        .unwrap_or(false);

        Ok(Some(ServiceState {
            start,
            delayed_auto_start: delayed,
        }))
    }

    fn set(&mut self, name: &str, state: ServiceState) -> Result<(), BackendError> {
        if self.read_only {
            return Err(BackendError::ReadOnly {
                target: format!("service {name}"),
            });
        }
        let Some(svc) = open_service(name, SERVICE_CHANGE_CONFIG | SERVICE_QUERY_CONFIG)? else {
            return Err(BackendError::ServiceNotFound { name: name.into() });
        };
        // SAFETY: `svc` is open with SERVICE_CHANGE_CONFIG; null strings mean "no change".
        unsafe {
            ChangeServiceConfigW(
                svc.0,
                ENUM_SERVICE_TYPE(SERVICE_NO_CHANGE),
                win_start_type(state.start),
                SERVICE_ERROR(SERVICE_NO_CHANGE),
                PCWSTR::null(),
                PCWSTR::null(),
                None,
                PCWSTR::null(),
                PCWSTR::null(),
                PCWSTR::null(),
                PCWSTR::null(),
            )
        }
        .map_err(|e| service_error("change service", name, &e))?;

        let info = SERVICE_DELAYED_AUTO_START_INFO {
            fDelayedAutostart: state.delayed_auto_start.into(),
        };
        // SAFETY: `info` lives until the call returns and matches the info level.
        unsafe {
            ChangeServiceConfig2W(
                svc.0,
                SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
                Some((&raw const info).cast()),
            )
        }
        .map_err(|e| service_error("change service delayed start", name, &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_covers_only_the_test_key() {
        assert!(in_test_key(Hive::CurrentUser, TEST_KEY));
        assert!(in_test_key(
            Hive::CurrentUser,
            r"software\pcoptimizertest\a\b"
        ));
        assert!(!in_test_key(Hive::LocalMachine, TEST_KEY));
        assert!(!in_test_key(
            Hive::CurrentUser,
            r"Software\PcOptimizerTestX"
        ));
        assert!(!in_test_key(
            Hive::CurrentUser,
            r"Software\Microsoft\GameBar"
        ));
        assert!(!in_test_key(Hive::CurrentUser, "Software"));
    }
}
