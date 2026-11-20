//! Reads the real [`Host`] facts and the running processes. Read-only: nothing here
//! changes the system.

use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::SystemInformation::{
    IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64, IMAGE_FILE_MACHINE_I386,
};
use windows::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2, OpenProcessToken};

use super::{edition_from_id, Arch, Host};
use crate::backend::windows::WindowsRegistry;
use crate::backend::{BackendError, RegRead, RegistryBackend};
use crate::recipe::{Hive, RegValue};

const VERSION_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("{op} failed: {message}")]
    Os { op: &'static str, message: String },

    #[error(transparent)]
    Registry(#[from] BackendError),
}

fn os(op: &'static str) -> impl FnOnce(windows::core::Error) -> ProbeError {
    move |e| ProbeError::Os {
        op,
        message: e.message(),
    }
}

/// Closes a kernel handle when dropped.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful Win32 call and is closed once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub fn host() -> Result<Host, ProbeError> {
    // Reads only; the sandboxed backend refuses any write anyway.
    let reg = WindowsRegistry::test_sandbox();
    let read_string = |value: &str| -> Result<Option<String>, ProbeError> {
        Ok(match reg.read(Hive::LocalMachine, VERSION_KEY, value)? {
            RegRead::Present(d) => match d.to_value() {
                Some(RegValue::String(s)) => Some(s),
                _ => None,
            },
            _ => None,
        })
    };

    // The build number is reliable; the product name still says "Windows 10" on Windows 11.
    let build = read_string("CurrentBuildNumber")?
        .and_then(|s| s.trim().parse::<u32>().ok())
        .ok_or(ProbeError::Os {
            op: "read the Windows build number",
            message: "CurrentBuildNumber is missing or not a number".into(),
        })?;
    let edition = read_string("EditionID")?
        .as_deref()
        .and_then(edition_from_id);

    Ok(Host {
        build,
        arch: native_arch()?,
        elevated: is_elevated()?,
        edition,
        // Chassis type and battery (Stage 1); `None` if Windows does not say.
        form_factor: crate::hw::windows::form_factor(),
    })
}

fn native_arch() -> Result<Arch, ProbeError> {
    let mut process = IMAGE_FILE_MACHINE::default();
    let mut native = IMAGE_FILE_MACHINE::default();
    // SAFETY: GetCurrentProcess returns a pseudo-handle; both out-pointers are valid.
    unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, Some(&mut native)) }
        .map_err(os("detect the processor architecture"))?;
    Ok(match native {
        IMAGE_FILE_MACHINE_AMD64 => Arch::X64,
        IMAGE_FILE_MACHINE_ARM64 => Arch::Arm64,
        IMAGE_FILE_MACHINE_I386 => Arch::X86,
        other => Arch::Other(other.0),
    })
}

fn is_elevated() -> Result<bool, ProbeError> {
    let mut token = HANDLE::default();
    // SAFETY: pseudo-handle for this process; `token` is a valid out-pointer.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(os("open the process token"))?;
    let token = Handle(token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut len = 0u32;
    // SAFETY: the buffer is a TOKEN_ELEVATION and its exact size is passed.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some((&raw mut elevation).cast::<c_void>()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    }
    .map_err(os("read the process token"))?;
    Ok(elevation.TokenIsElevated != 0)
}

/// Executable names of all running processes, e.g. `explorer.exe`.
pub fn running_processes() -> Result<Vec<String>, ProbeError> {
    // SAFETY: plain snapshot of the process list.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(os("list running processes"))?;
    let snapshot = Handle(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut names = Vec::new();
    // SAFETY: `entry.dwSize` is set as the API requires.
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_ok();
    while more {
        let len = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        names.push(String::from_utf16_lossy(&entry.szExeFile[..len]));
        // SAFETY: same snapshot and entry as above.
        more = unsafe { Process32NextW(snapshot.0, &mut entry) }.is_ok();
    }
    Ok(names)
}
