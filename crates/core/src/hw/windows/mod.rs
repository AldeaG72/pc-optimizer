//! Reads [`HwInfo`] from the real system. **Read-only**: registry access goes through the
//! sandboxed backend (which refuses every write) or read-only Win32 calls.

mod display;
mod games;
mod launch;
mod power;
mod software;
mod storage;
mod system;

use windows::core::HSTRING;
use windows::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
    KEY_ENUMERATE_SUB_KEYS, KEY_WOW64_64KEY,
};

pub use launch::launch_game;

use super::HwInfo;
use crate::backend::windows::WindowsRegistry;
use crate::backend::{RegRead, RegistryBackend};
use crate::recipe::{Hive, RegValue};

/// Reads everything. Never fails: what cannot be read stays `None`/empty and is listed in
/// [`HwInfo::probe_errors`].
pub fn detect() -> HwInfo {
    let mut info = HwInfo::default();
    let mut errors = Vec::new();
    // Reads only; this backend refuses any write anyway.
    let reg = WindowsRegistry::test_sandbox();
    let wmi = match wmi::WMIConnection::new() {
        Ok(c) => Some(c),
        Err(e) => {
            errors.push(format!("wmi: {e}"));
            None
        }
    };

    system::windows(&reg, wmi.as_ref(), &mut info, &mut errors);
    system::cpu_and_memory(wmi.as_ref(), &mut info, &mut errors);
    system::gpus(wmi.as_ref(), &mut info, &mut errors);
    display::displays(&reg, &mut info, &mut errors);
    storage::storage(&reg, wmi.as_ref(), &mut info, &mut errors);
    power::power(wmi.as_ref(), &mut info, &mut errors);
    power::security(&mut info, &mut errors);
    software::installed(&reg, &mut info, &mut errors);
    games::games(&reg, &mut info, &mut errors);

    info.probe_errors = errors;
    info
}

/// Only the installed launch games (for `pco bench`).
pub fn installed_games() -> Vec<super::Game> {
    let mut info = HwInfo::default();
    let mut errors = Vec::new();
    games::games(&WindowsRegistry::test_sandbox(), &mut info, &mut errors);
    info.games
}

/// Desktop or laptop only (chassis type + battery), for the recipe guards.
pub fn form_factor() -> Option<crate::recipe::FormFactor> {
    let mut info = HwInfo::default();
    let mut errors = Vec::new();
    let wmi = wmi::WMIConnection::new().ok();
    power::power(wmi.as_ref(), &mut info, &mut errors);
    info.power.form_factor
}

/// Text from a fixed-size UTF-16 buffer, up to the first NUL.
pub(crate) fn wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

pub(crate) fn reg_string(
    reg: &WindowsRegistry,
    hive: Hive,
    key: &str,
    value: &str,
) -> Option<String> {
    match reg.read(hive, key, value).ok()? {
        RegRead::Present(d) => match d.to_value()? {
            RegValue::String(s) => Some(s),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn reg_dword(reg: &WindowsRegistry, hive: Hive, key: &str, value: &str) -> Option<u32> {
    match reg.read(hive, key, value).ok()? {
        RegRead::Present(d) => match d.to_value()? {
            RegValue::Dword(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}

/// Names of the subkeys of `key`. An absent key gives an empty list.
pub(crate) fn subkeys(hive: Hive, key: &str) -> Result<Vec<String>, String> {
    let root = match hive {
        Hive::CurrentUser => HKEY_CURRENT_USER,
        Hive::LocalMachine => HKEY_LOCAL_MACHINE,
    };
    let mut handle = HKEY::default();
    let path = HSTRING::from(key);
    // SAFETY: `path` outlives the call; `handle` is a valid out-pointer.
    let rc = unsafe {
        RegOpenKeyExW(
            root,
            &path,
            Some(0),
            KEY_ENUMERATE_SUB_KEYS | KEY_WOW64_64KEY,
            &mut handle,
        )
    };
    if rc != ERROR_SUCCESS {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    let mut index = 0;
    loop {
        let mut buf = [0u16; 256];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` holds `len` UTF-16 units.
        let rc = unsafe {
            RegEnumKeyExW(
                handle,
                index,
                Some(windows::core::PWSTR(buf.as_mut_ptr())),
                &mut len,
                None,
                None,
                None,
                None,
            )
        };
        if rc == ERROR_NO_MORE_ITEMS {
            break;
        }
        if rc != ERROR_SUCCESS {
            // SAFETY: opened above, closed once.
            let _ = unsafe { RegCloseKey(handle) };
            return Err(format!("{key}: cannot list subkeys ({})", rc.0));
        }
        names.push(String::from_utf16_lossy(&buf[..len as usize]));
        index += 1;
    }
    // SAFETY: opened above, closed once.
    let _ = unsafe { RegCloseKey(handle) };
    Ok(names)
}

/// Untagged number: WMI returns 64-bit integers as text.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(crate) enum Num {
    U(u64),
    I(i64),
    S(String),
}

impl Num {
    pub(crate) fn get(&self) -> Option<u64> {
        match self {
            Num::U(v) => Some(*v),
            Num::I(v) => u64::try_from(*v).ok(),
            Num::S(s) => s.trim().parse().ok(),
        }
    }
}

pub(crate) fn num(n: &Option<Num>) -> Option<u64> {
    n.as_ref().and_then(Num::get)
}
