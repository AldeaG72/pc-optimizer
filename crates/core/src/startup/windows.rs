//! The real Startup folders (known folders, never environment variables). Reads only.

use windows::core::GUID;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_CommonStartup, FOLDERID_Startup, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};

use super::Location;

fn known_folder(id: &GUID) -> Option<String> {
    // SAFETY: `id` is a static GUID; the returned string is freed below.
    let path = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok()?;
    // SAFETY: a valid NUL-terminated string from SHGetKnownFolderPath.
    let text = unsafe { path.to_string() }.ok();
    // SAFETY: allocated by SHGetKnownFolderPath with CoTaskMemAlloc.
    unsafe { CoTaskMemFree(Some(path.0 as *const _)) };
    text.filter(|t| !t.is_empty())
}

/// The files in this user's and every user's Startup folder.
pub fn folders() -> Vec<(Location, Vec<String>)> {
    [
        (Location::UserFolder, &FOLDERID_Startup),
        (Location::CommonFolder, &FOLDERID_CommonStartup),
    ]
    .into_iter()
    .filter_map(|(location, id)| {
        let dir = known_folder(id)?;
        let files = std::fs::read_dir(&dir)
            .ok()?
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path().to_string_lossy().into_owned())
            .collect();
        Some((location, files))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::windows::WindowsRegistry;
    use crate::backend::ReadOnlyRegistry;

    /// Reads this PC's startup programs; changes nothing.
    #[test]
    fn lists_this_pc_without_changing_it() {
        let reg = ReadOnlyRegistry(WindowsRegistry::system());
        let entries = super::super::list(&reg, &folders()).unwrap();
        for e in &entries {
            assert!(e.id.starts_with("startup."), "{}", e.id);
        }
    }
}
