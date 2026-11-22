//! The real side of game mode: running processes with their paths, the active power plan
//! (switching plans needs no administrator rights), closing an app the way its window's close
//! button would (`taskkill` without `/F`), starting it again, and the state file.
//!
//! Like every real backend, its tests on the development PC only read (CLAUDE.md rule 1).

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use windows::core::PWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::backend::{PowerBackend, WindowsPower};
use crate::gamemode::{ActiveState, GameModeSystem, Process};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;

pub struct WindowsGameMode {
    power: WindowsPower,
    state_file: PathBuf,
}

impl WindowsGameMode {
    /// `state_file`: where the original state is kept while a game runs.
    pub fn new(state_file: PathBuf) -> Self {
        Self {
            power: WindowsPower::system(),
            state_file,
        }
    }

    /// The state left by a game mode that never finished (crash, power cut), if any.
    pub fn pending_state(&self) -> Option<ActiveState> {
        let text = std::fs::read_to_string(&self.state_file).ok()?;
        serde_json::from_str(&text).ok()
    }
}

fn windows_dir() -> PathBuf {
    let mut buf = [0u16; 260];
    // SAFETY: the buffer is writable for its whole length.
    let len = unsafe { GetSystemWindowsDirectoryW(Some(&mut buf)) } as usize;
    if len > 0 && len < buf.len() {
        PathBuf::from(String::from_utf16_lossy(&buf[..len]))
    } else {
        PathBuf::from(r"C:\Windows")
    }
}

fn process_path(pid: u32) -> Option<String> {
    // SAFETY: plain query access; the handle is closed below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` has `len` writable units.
    let ok = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    // SAFETY: opened above.
    let _ = unsafe { CloseHandle(handle) };
    ok.ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// Every running process with its path (when Windows gives it to a normal user).
pub fn processes() -> Result<Vec<Process>, String> {
    // SAFETY: a snapshot of all processes; closed below.
    let snap =
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.map_err(|e| e.message())?;
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut out = Vec::new();
    // SAFETY: `entry` has its size set.
    let mut more = unsafe { Process32FirstW(snap, &mut entry) }.is_ok();
    while more {
        let len = entry
            .szExeFile
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
        out.push(Process {
            path: process_path(entry.th32ProcessID),
            name,
        });
        // SAFETY: as above.
        more = unsafe { Process32NextW(snap, &mut entry) }.is_ok();
    }
    // SAFETY: from CreateToolhelp32Snapshot.
    let _ = unsafe { CloseHandle(snap) };
    Ok(out)
}

impl GameModeSystem for WindowsGameMode {
    fn processes(&self) -> Result<Vec<Process>, String> {
        processes()
    }

    fn active_plan(&self) -> Result<String, String> {
        self.power.active_scheme().map_err(|e| e.to_string())
    }

    fn plan_exists(&self, plan: &str) -> Result<bool, String> {
        self.power.scheme_exists(plan).map_err(|e| e.to_string())
    }

    fn set_plan(&mut self, plan: &str) -> Result<(), String> {
        self.power
            .set_active_scheme(plan)
            .map_err(|e| e.to_string())
    }

    fn close(&mut self, name: &str) -> Result<(), String> {
        // Polite close only (no /F): the app may ask to save, or stay if it refuses.
        Command::new(windows_dir().join(r"System32\taskkill.exe"))
            .args(["/IM", name])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn start(&mut self, path: &str) -> Result<(), String> {
        Command::new(path)
            .creation_flags(DETACHED_PROCESS)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn save_state(&mut self, state: Option<&ActiveState>) -> Result<(), String> {
        match state {
            Some(s) => {
                if let Some(dir) = self.state_file.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                let tmp = self.state_file.with_extension("tmp");
                let text = serde_json::to_string(s).map_err(|e| e.to_string())?;
                std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
                std::fs::rename(&tmp, &self.state_file).map_err(|e| e.to_string())
            }
            None => match std::fs::remove_file(&self.state_file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads only: the process list has this test itself, with a path.
    #[test]
    fn lists_processes_with_paths() {
        let list = processes().unwrap();
        let me = std::env::current_exe().unwrap();
        let name = me.file_name().unwrap().to_string_lossy().to_lowercase();
        let found = list.iter().find(|p| p.name.to_lowercase() == name).unwrap();
        assert!(found.path.is_some());
    }

    #[test]
    fn state_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("pco-gm-{}", std::process::id()));
        let mut gm = WindowsGameMode::new(dir.join("gamemode-state.json"));
        let state = ActiveState {
            game: "cs2.exe".into(),
            plan_before: Some("x".into()),
            closed: vec![r"C:\a.exe".into()],
        };
        gm.save_state(Some(&state)).unwrap();
        assert_eq!(gm.pending_state(), Some(state));
        gm.save_state(None).unwrap();
        assert_eq!(gm.pending_state(), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
