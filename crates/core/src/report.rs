//! The log export for support (SPEC 4.9, Stage 3: 3.13): Windows, hardware, the tweaks and
//! what happened to them (journal, errors), benchmark results. A plain text file the user
//! saves where they choose and can read before sending.
//!
//! No personal data (CLAUDE.md): the user name, the PC name and the profile folder are
//! replaced everywhere, so is any `\Users\<name>\` path and anything that looks like an IPv4
//! address; volume labels (named by the user) are left out.

use std::fmt::Write as _;

use crate::bench::Series;
use crate::guard::Host;
use crate::hw::HwInfo;
use crate::journal::{Journal, JournalError};

pub const HIDDEN: &str = "<hidden>";

/// What goes into the report.
pub struct ReportInput<'a> {
    pub app_version: &'a str,
    pub host: Option<&'a Host>,
    pub hw: Option<&'a HwInfo>,
    /// Each recipe and its state on this PC, as text.
    pub statuses: Vec<(String, String)>,
    pub series: Vec<Series>,
    /// Words to hide: user name, PC name, profile folder.
    pub secrets: Vec<String>,
}

/// The whole report, already anonymized.
pub fn build(input: &ReportInput<'_>, journal: &Journal) -> Result<String, JournalError> {
    let mut out = String::new();
    let line = |out: &mut String, text: String| {
        out.push_str(&text);
        out.push('\n');
    };
    line(
        &mut out,
        format!("PC Optimizer {} — support log", input.app_version),
    );
    line(&mut out, String::new());

    line(&mut out, "== Windows ==".into());
    match input.host {
        Some(h) => line(
            &mut out,
            format!(
                "build {}, {:?}, edition {:?}, elevated {}, form factor {:?}",
                h.build, h.arch, h.edition, h.elevated, h.form_factor
            ),
        ),
        None => line(&mut out, "not detected".into()),
    }
    line(&mut out, String::new());

    line(&mut out, "== Hardware ==".into());
    match input.hw {
        Some(hw) => {
            let mut value = serde_json::to_value(hw).unwrap_or_default();
            if let Some(volumes) = value.get_mut("volumes").and_then(|v| v.as_array_mut()) {
                for volume in volumes {
                    if let Some(map) = volume.as_object_mut() {
                        map.remove("label");
                    }
                }
            }
            line(
                &mut out,
                serde_json::to_string_pretty(&value).unwrap_or_default(),
            );
        }
        None => line(&mut out, "not detected".into()),
    }
    line(&mut out, String::new());

    line(&mut out, "== Tweaks on this PC ==".into());
    for (id, status) in &input.statuses {
        line(&mut out, format!("{id}: {status}"));
    }
    line(&mut out, String::new());

    line(&mut out, "== Journal (newest first) ==".into());
    let mut sessions = journal.sessions()?;
    sessions.reverse();
    for s in sessions.iter().take(50) {
        line(
            &mut out,
            format!(
                "session {} {} {} started {} finished {:?}, app {}",
                s.id.0,
                s.kind.as_str(),
                s.status.as_str(),
                s.started_at,
                s.finished_at,
                s.app_version
            ),
        );
        for e in journal.entries(s.id)? {
            let mut text = format!(
                "  {} #{} {} {} {}",
                e.recipe_id,
                e.action_index,
                e.kind.as_str(),
                e.target,
                e.status.as_str()
            );
            if let Some(error) = &e.error {
                let _ = write!(text, " — error: {error}");
            }
            line(&mut out, text);
        }
    }
    line(&mut out, String::new());

    line(&mut out, "== Benchmarks ==".into());
    for s in &input.series {
        let runs: Vec<String> = s
            .runs
            .iter()
            .map(|m| format!("{:.1} fps / 1% low {:.1}", m.avg_fps, m.low1_fps))
            .collect();
        line(
            &mut out,
            format!("{} {} {:?}: {}", s.id, s.target, s.label, runs.join("; ")),
        );
    }
    Ok(scrub(&out, &input.secrets))
}

/// Hides the secrets (case-insensitive), the name in any `\Users\<name>` path and IPv4
/// addresses.
pub fn scrub(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_owned();
    // Longest first, so a profile path goes before the user name inside it.
    let mut secrets: Vec<&String> = secrets.iter().filter(|s| s.trim().len() >= 2).collect();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for secret in secrets {
        out = replace_ignore_case(&out, secret, HIDDEN);
    }
    out = hide_user_folders(&out);
    hide_ipv4(&out)
}

fn replace_ignore_case(text: &str, needle: &str, with: &str) -> String {
    let lower_text = text.to_lowercase();
    let lower_needle = needle.to_lowercase();
    // Lowercasing can change byte lengths for some letters; fall back to an exact match.
    if lower_text.len() != text.len() || lower_needle.len() != needle.len() {
        return text.replace(needle, with);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = 0;
    let mut from = 0;
    while let Some(pos) = lower_text[from..].find(&lower_needle) {
        let start = from + pos;
        out.push_str(&text[rest..start]);
        out.push_str(with);
        rest = start + needle.len();
        from = rest;
    }
    out.push_str(&text[rest..]);
    out
}

/// `C:\Users\Ana\…` → `C:\Users\<hidden>\…` (also with `/` and JSON's `\\`).
fn hide_user_folders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let lower = text.to_ascii_lowercase();
    let mut i = 0;
    while i < text.len() {
        let rest = &lower[i..];
        let marker = ["\\\\users\\\\", "\\users\\", "/users/"]
            .into_iter()
            .find(|m| rest.starts_with(m));
        if let Some(m) = marker {
            out.push_str(&text[i..i + m.len()]);
            i += m.len();
            let end = text[i..]
                .find(['\\', '/', '"', '\n', ' '])
                .map_or(text.len(), |e| i + e);
            if end > i {
                out.push_str(HIDDEN);
            }
            i = end;
        } else {
            let ch = text[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Four dot-separated numbers 0–255 → `<ip>`.
fn hide_ipv4(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let starts_word = i == 0 || !(bytes[i - 1].is_ascii_digit() || bytes[i - 1] == b'.');
        if starts_word && bytes[i].is_ascii_digit() {
            if let Some(len) = ipv4_len(&bytes[i..]) {
                out.push_str("<ip>");
                i += len;
                continue;
            }
        }
        let ch = text[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn ipv4_len(bytes: &[u8]) -> Option<usize> {
    let mut i = 0;
    for part in 0..4 {
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() && i - start < 3 {
            i += 1;
        }
        let digits = std::str::from_utf8(&bytes[start..i]).ok()?;
        if digits.is_empty() || digits.parse::<u16>().ok()? > 255 {
            return None;
        }
        if part < 3 {
            if bytes.get(i) != Some(&b'.') {
                return None;
            }
            i += 1;
        }
    }
    // Not part of a longer number or version (1.2.3.4.5).
    match bytes.get(i) {
        Some(b) if b.is_ascii_digit() || *b == b'.' => None,
        _ => Some(i),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_names_paths_and_addresses() {
        let text = r"user Ana on DESKTOP-ANA7 at C:\Users\Ana\Games and C:\\Users\\Bob\\x, ip 192.168.1.20, build 10.0.26100.1";
        let out = scrub(text, &["Ana".into(), "DESKTOP-ANA7".into()]);
        assert!(!out.contains("Ana"), "{out}");
        assert!(!out.contains("Bob"), "{out}");
        assert!(!out.contains("192.168"), "{out}");
        assert!(out.contains("<ip>"));
        // A version number with more than four parts is not an address.
        assert!(out.contains("10.0.26100.1"), "{out}");
        assert!(out.contains(r"C:\Users\<hidden>\Games"), "{out}");
    }

    #[test]
    fn report_has_every_section_and_no_secrets() {
        let journal = Journal::open_in_memory().unwrap();
        let input = ReportInput {
            app_version: "0.3.0",
            host: None,
            hw: None,
            statuses: vec![("gaming.game-mode-on".into(), "applied".into())],
            series: Vec::new(),
            secrets: vec!["Ana".into()],
        };
        let text = build(&input, &journal).unwrap();
        for section in [
            "== Windows ==",
            "== Hardware ==",
            "== Tweaks",
            "== Journal",
            "== Benchmarks ==",
        ] {
            assert!(text.contains(section), "{section}");
        }
        assert!(text.contains("gaming.game-mode-on: applied"));
    }
}
