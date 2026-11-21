//! The pinned PresentMon console app: where it is, and refusing any other file.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Keep in sync with `scripts/fetch-presentmon.ps1` (a test checks).
pub const VERSION: &str = "2.6.0";
pub const FILE_NAME: &str = "PresentMon-2.6.0-x64.exe";
pub const SHA256: &str = "b2a706bc6ad475749e3b7e3409263aa1e6906d45bdcf993f6dbc0f660188f1af";

/// ETW session name, so we never touch someone else's PresentMon session.
pub const SESSION_NAME: &str = "PcOptimizerBench";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PresentMonError {
    #[error("PresentMon {VERSION} was not found (looked in: {0})")]
    NotFound(String),
    #[error(
        "{path} is not the official PresentMon {VERSION}: SHA-256 {actual} instead of {SHA256}"
    )]
    WrongHash { path: String, actual: String },
    #[error("cannot read {path}: {message}")]
    Io { path: String, message: String },
}

/// Places to look, in order: `presentmon\` next to pco.exe (installed), then the project's
/// `vendor\presentmon\` (development).
pub fn candidates(exe_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = exe_dir {
        out.push(dir.join("presentmon").join(FILE_NAME));
        out.push(dir.join(FILE_NAME));
    }
    out.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("vendor")
            .join("presentmon")
            .join(FILE_NAME),
    );
    out
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The first candidate that exists, after checking its SHA-256.
pub fn locate(exe_dir: Option<&Path>) -> Result<PathBuf, PresentMonError> {
    let candidates = candidates(exe_dir);
    let Some(path) = candidates.iter().find(|p| p.is_file()) else {
        let list = candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(PresentMonError::NotFound(list));
    };
    verify(path)?;
    Ok(path.clone())
}

pub fn verify(path: &Path) -> Result<(), PresentMonError> {
    let bytes = std::fs::read(path).map_err(|e| PresentMonError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    let actual = sha256_hex(&bytes);
    if actual != SHA256 {
        return Err(PresentMonError::WrongHash {
            path: path.display().to_string(),
            actual,
        });
    }
    Ok(())
}

/// Arguments for one timed capture of one process into a CSV file. No overlay, no console
/// statistics; our own session name; stops by itself after `seconds`.
///
/// Only the time between presents is used (FPS and 1% low), so display, GPU and input
/// tracking are off: less overhead, and frames are recorded even where a present never
/// reaches a real display (a CI runner with the WARP renderer recorded nothing with them on).
pub fn arguments(process: &str, csv: &Path, seconds: u32) -> Vec<String> {
    vec![
        "--no_track_display".into(),
        "--no_track_gpu".into(),
        "--no_track_input".into(),
        "--process_name".into(),
        process.into(),
        "--output_file".into(),
        csv.display().to_string(),
        "--timed".into(),
        seconds.to_string(),
        "--terminate_after_timed".into(),
        "--no_console_stats".into(),
        "--session_name".into(),
        SESSION_NAME.into(),
        "--stop_existing_session".into(),
    ]
}

/// One timed capture of `process` with the PresentMon at `exe` (checked again right before
/// it runs); returns the CSV text. Stops early when `cancel` becomes true. Windows only;
/// needs administrator rights (ETW).
#[cfg(windows)]
pub fn capture(
    exe: &Path,
    process: &str,
    seconds: u32,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<String, String> {
    use std::process::{Command, Stdio};
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    verify(exe).map_err(|e| e.to_string())?;
    static CAPTURES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = CAPTURES.fetch_add(1, Ordering::SeqCst);
    let csv = std::env::temp_dir().join(format!("pco-bench-{}-{n}.csv", std::process::id()));
    let log = csv.with_extension("log");
    let _ = std::fs::remove_file(&csv);
    // PresentMon's own messages go to a file (not a pipe: it cannot fill up and block),
    // so a failed capture can say why.
    let log_file = std::fs::File::create(&log).map_err(|e| e.to_string())?;
    let log_err = log_file.try_clone().map_err(|e| e.to_string())?;
    let mut child = Command::new(exe)
        .args(arguments(process, &csv, seconds))
        .stdin(Stdio::null())
        .stdout(log_file)
        .stderr(log_err)
        .spawn()
        .map_err(|e| format!("cannot start PresentMon: {e}"))?;
    // PresentMon stops by itself (--timed); give it a margin, then stop it.
    let deadline = Instant::now() + Duration::from_secs(u64::from(seconds) + 30);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if cancel.load(Ordering::SeqCst) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&csv);
                let _ = std::fs::remove_file(&log);
                return Err("cancelled".into());
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(250)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("PresentMon did not stop in time".into());
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let said = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_file(&log);
    let text = std::fs::read_to_string(&csv).map_err(|_| {
        let mut why = "no frames were captured (was the game running and in focus?)".to_owned();
        let said: String = said.split_whitespace().collect::<Vec<_>>().join(" ");
        if !status.success() || !said.is_empty() {
            let skip = said.chars().count().saturating_sub(300);
            let tail: String = said.chars().skip(skip).collect();
            why.push_str(&format!(" [PresentMon {status}: {tail}]"));
        }
        why
    });
    let _ = std::fs::remove_file(&csv);
    text
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_values_match_the_fetch_script() {
        let script = include_str!("../../../../scripts/fetch-presentmon.ps1");
        assert!(script.contains(&format!("$VERSION = '{VERSION}'")));
        assert!(script.contains(&format!("$FILE = '{FILE_NAME}'")));
        assert!(script.contains(&format!("$SHA256 = '{SHA256}'")));
        let notice = include_str!("../../../../THIRD-PARTY.md");
        assert!(notice.contains(SHA256) && notice.contains(VERSION));
    }

    #[test]
    fn refuses_a_file_with_another_hash() {
        let dir = std::env::temp_dir().join(format!("pco-pm-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join(FILE_NAME);
        std::fs::write(&fake, b"not presentmon").unwrap();
        assert!(matches!(
            verify(&fake),
            Err(PresentMonError::WrongHash { .. })
        ));
        assert!(matches!(
            locate(Some(&dir)),
            Err(PresentMonError::WrongHash { .. })
        ));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn capture_arguments() {
        let args = arguments("cs2.exe", Path::new(r"C:\t\run.csv"), 60);
        let joined = args.join(" ");
        assert!(joined.contains("--process_name cs2.exe"));
        assert!(joined.contains(r"--output_file C:\t\run.csv"));
        assert!(joined.contains("--timed 60 --terminate_after_timed"));
        assert!(joined.contains("--session_name PcOptimizerBench"));
        assert!(joined.contains("--no_track_display --no_track_gpu --no_track_input"));
    }
}
