//! The optional daily backup (SPEC 4.9, Stage 3: 3.13): once a day, the current value of
//! every registry value the app has changed, as a `.reg` file next to the journal. The
//! journal stays what undoes changes; this is a copy Windows itself can import (double-click
//! in Explorer), in case the journal is lost. The last 7 days are kept (owner decision).
//!
//! Reads the registry only; writes only its own files.

use std::path::{Path, PathBuf};

use crate::backend::{RegRead, RegistryBackend, REG_BINARY, REG_DWORD, REG_QWORD, REG_SZ};
use crate::engine::Snapshot;
use crate::journal::{Journal, JournalError};
use crate::recipe::Hive;

/// Days of backups kept.
pub const KEEP: usize = 7;
const PREFIX: &str = "registry-";

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Registry(String),
}

/// `YYYY-MM-DD` for a count of days since 1970-01-01 (civil calendar).
pub fn date_from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn root(hive: Hive) -> &'static str {
    match hive {
        Hive::CurrentUser => "HKEY_CURRENT_USER",
        Hive::LocalMachine => "HKEY_LOCAL_MACHINE",
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// One `"Name"=…` line of a `.reg` file; a missing value becomes `"Name"=-` (delete).
fn value_line(name: &str, state: &RegRead) -> String {
    let name = if name.is_empty() {
        "@".to_owned()
    } else {
        quote(name)
    };
    match state {
        RegRead::KeyMissing | RegRead::ValueMissing => format!("{name}=-"),
        RegRead::Present(d) => match d.reg_type {
            REG_DWORD if d.bytes.len() == 4 => {
                let v = u32::from_le_bytes([d.bytes[0], d.bytes[1], d.bytes[2], d.bytes[3]]);
                format!("{name}=dword:{v:08x}")
            }
            REG_SZ => match d.to_value() {
                Some(crate::recipe::RegValue::String(s)) => format!("{name}={}", quote(&s)),
                _ => format!("{name}=hex(1):{}", hex(&d.bytes)),
            },
            REG_BINARY => format!("{name}=hex:{}", hex(&d.bytes)),
            REG_QWORD => format!("{name}=hex(b):{}", hex(&d.bytes)),
            other => format!("{name}=hex({other:x}):{}", hex(&d.bytes)),
        },
    }
}

/// The `.reg` text for these values (grouped by key, in the order given).
pub fn to_reg(values: &[(Hive, String, String, RegRead)]) -> String {
    let mut out = String::from("Windows Registry Editor Version 5.00\r\n");
    let mut current: Option<(Hive, String)> = None;
    for (hive, key, name, state) in values {
        let this = (*hive, key.to_lowercase());
        if current.as_ref() != Some(&this) {
            out.push_str(&format!("\r\n[{}\\{}]\r\n", root(*hive), key));
            current = Some(this);
        }
        out.push_str(&value_line(name, state));
        out.push_str("\r\n");
    }
    out
}

/// `.reg` files are UTF-16LE with a byte-order mark.
pub fn encode(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xfe];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    bytes
}

/// The registry values the app has changed and not undone, read now.
pub fn current_values(
    reg: &dyn RegistryBackend,
    journal: &Journal,
) -> Result<Vec<(Hive, String, String, RegRead)>, BackupError> {
    let mut out: Vec<(Hive, String, String, RegRead)> = Vec::new();
    for entry in journal.all_applied_entries()? {
        let Ok(Snapshot::Registry {
            hive, key, value, ..
        }) = entry.previous::<Snapshot>()
        else {
            continue;
        };
        let seen = out.iter().any(|(h, k, v, _)| {
            *h == hive && k.eq_ignore_ascii_case(&key) && v.eq_ignore_ascii_case(&value)
        });
        if seen {
            continue;
        }
        let state = reg
            .read(hive, &key, &value)
            .map_err(|e| BackupError::Registry(e.to_string()))?;
        out.push((hive, key, value, state));
    }
    out.sort_by_key(|a| (a.0 as u8, a.1.to_lowercase()));
    Ok(out)
}

/// Writes today's backup in `dir` unless it exists, then keeps the newest [`KEEP`] files.
/// Returns the new file, if one was written.
pub fn run_daily(
    dir: &Path,
    today: &str,
    reg: &dyn RegistryBackend,
    journal: &Journal,
) -> Result<Option<PathBuf>, BackupError> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{PREFIX}{today}.reg"));
    let written = if path.exists() {
        None
    } else {
        let text = to_reg(&current_values(reg, journal)?);
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, encode(&text))?;
        std::fs::rename(&tmp, &path)?;
        Some(path)
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(PREFIX) && n.ends_with(".reg"))
        })
        .collect();
    // Names sort by date.
    files.sort();
    let extra = files.len().saturating_sub(KEEP);
    for old in &files[..extra] {
        std::fs::remove_file(old)?;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::memory::MemoryRegistry;
    use crate::backend::RegData;
    use crate::recipe::RegValue;

    #[test]
    fn dates() {
        assert_eq!(date_from_days(0), "1970-01-01");
        assert_eq!(date_from_days(20_779), "2026-11-22");
        assert_eq!(date_from_days(19_782), "2024-02-29");
    }

    #[test]
    fn reg_file_format() {
        let text = to_reg(&[
            (
                Hive::CurrentUser,
                r"Software\Microsoft\GameBar".into(),
                "AutoGameModeEnabled".into(),
                RegRead::Present(RegData::from_value(&RegValue::Dword(1))),
            ),
            (
                Hive::CurrentUser,
                r"Software\Microsoft\GameBar".into(),
                "Other".into(),
                RegRead::ValueMissing,
            ),
            (
                Hive::LocalMachine,
                r"SOFTWARE\X".into(),
                "Text".into(),
                RegRead::Present(RegData::from_value(&RegValue::String(r#"a\b"c"#.into()))),
            ),
        ]);
        assert!(text.starts_with("Windows Registry Editor Version 5.00\r\n"));
        assert!(text.contains("[HKEY_CURRENT_USER\\Software\\Microsoft\\GameBar]\r\n\"AutoGameModeEnabled\"=dword:00000001\r\n\"Other\"=-\r\n"));
        assert!(text.contains("\"Text\"=\"a\\\\b\\\"c\""), "{text}");
        assert_eq!(&encode("W")[..], &[0xff, 0xfe, b'W', 0]);
    }

    #[test]
    fn writes_once_a_day_and_keeps_seven() {
        let dir = std::env::temp_dir().join(format!("pco-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let journal = Journal::open_in_memory().unwrap();
        let reg = MemoryRegistry::new();
        for day in 1..=9 {
            let written = run_daily(&dir, &format!("2026-11-{day:02}"), &reg, &journal).unwrap();
            assert!(written.is_some());
        }
        assert!(run_daily(&dir, "2026-11-09", &reg, &journal)
            .unwrap()
            .is_none());
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names.len(), KEEP);
        assert_eq!(names[0], "registry-2026-11-03.reg");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
