//! `pco health`: the hardware summary and the health check results, as text or JSON.

use std::fmt::Display;
use std::io::Write;

use anyhow::Result;
use pco_core::health::{CheckId, Finding, Outcome};
use pco_core::hw::{DiskMedia, HwInfo};
use pco_core::recipe::FormFactor;

use crate::format;
use crate::i18n::Texts;

pub fn title(t: &Texts, check: CheckId) -> String {
    t.t(&format!("health.{}.title", check.key())).to_owned()
}

/// Message arguments for a finding: its subject and all its facts. The language-neutral
/// values `on`/`off` are translated.
fn args<'a>(t: &'a Texts, f: &'a Finding) -> Vec<(&'a str, &'a dyn Display)> {
    let mut args: Vec<(&str, &dyn Display)> = Vec::new();
    if let Some(s) = &f.subject {
        args.push(("subject", s));
    }
    for (name, value) in &f.facts {
        match value.as_str() {
            "on" => args.push((name, t.text("value.on"))),
            "off" => args.push((name, t.text("value.off"))),
            _ => args.push((name, value)),
        }
    }
    args
}

/// The lines for one problem, warning or info finding.
pub fn finding_lines(t: &Texts, f: &Finding) -> Vec<String> {
    let mut heading = format!(
        "  [{}] {}",
        t.t(&format!("outcome.{}", f.outcome.key())),
        title(t, f.check)
    );
    if let Some(s) = &f.subject {
        heading.push_str(&format!(" — {s}"));
    }
    let key = format!("health.{}.{}", f.check.key(), f.outcome.key());
    let a = args(t, f);
    let mut lines = vec![heading, format!("    {}", t.f(&key, &a))];
    let fix = t.f(&format!("health.{}.fix", f.check.key()), &a);
    lines.push(t.f("health.fix", &[("text", &fix)]));
    if let Some(url) = f.link {
        lines.push(t.f("health.link", &[("url", &url)]));
    }
    lines
}

pub fn hardware_lines(t: &Texts, info: &HwInfo) -> Vec<String> {
    let mut lines = Vec::new();
    let unknown = "?".to_owned();
    let w = &info.windows;
    let form = t.t(match info.power.form_factor {
        Some(FormFactor::Laptop) => "hw.form.laptop",
        Some(FormFactor::Desktop) => "hw.form.desktop",
        _ => "hw.form.unknown",
    });
    lines.push(t.f(
        "hw.windows",
        &[
            ("version", w.display_version.as_ref().unwrap_or(&unknown)),
            ("edition", &w.edition.map_or("", format::edition)),
            ("build", &w.build),
            ("form", &form),
        ],
    ));
    if let Some(cpu) = &info.cpu {
        lines.push(t.f(
            "hw.cpu",
            &[
                ("name", &cpu.name),
                ("cores", &cpu.cores),
                ("threads", &cpu.threads),
            ],
        ));
    }
    if !info.memory.is_empty() {
        let total_mb: u64 = info.memory.iter().map(|m| m.capacity_mb).sum();
        let mhz = info
            .memory
            .iter()
            .filter_map(|m| m.configured_mhz)
            .min()
            .map_or("?".to_owned(), |m| m.to_string());
        lines.push(t.f(
            "hw.memory",
            &[
                ("gb", &(total_mb / 1024)),
                ("modules", &info.memory.len()),
                ("mhz", &mhz),
            ],
        ));
    }
    for g in &info.gpus {
        let kind = t.t(match g.integrated {
            Some(true) => "hw.gpu.integrated",
            Some(false) => "hw.gpu.dedicated",
            None => "hw.gpu.unknown",
        });
        lines.push(t.f(
            "hw.gpu",
            &[
                ("name", &g.name),
                ("kind", &kind),
                ("version", g.driver_version.as_ref().unwrap_or(&unknown)),
                ("date", g.driver_date.as_ref().unwrap_or(&unknown)),
            ],
        ));
    }
    for d in &info.displays {
        lines.push(t.f(
            "hw.display",
            &[
                ("name", &d.name),
                ("width", &d.width),
                ("height", &d.height),
                ("hz", &d.refresh_hz),
            ],
        ));
    }
    let gb = |b: u64| b / (1024 * 1024 * 1024);
    for d in &info.disks {
        let media = t.t(match d.media {
            DiskMedia::Ssd => "hw.media.ssd",
            DiskMedia::Hdd => "hw.media.hdd",
            DiskMedia::Unknown => "hw.media.unknown",
        });
        let media = match &d.bus {
            Some(bus) => format!("{media} {bus}"),
            None => media.to_owned(),
        };
        lines.push(t.f(
            "hw.disk",
            &[
                ("model", &d.model),
                ("media", &media),
                ("gb", &gb(d.size_bytes)),
            ],
        ));
    }
    for v in &info.volumes {
        lines.push(t.f(
            "hw.volume",
            &[
                ("letter", &v.letter),
                ("free", &gb(v.free_bytes)),
                ("size", &gb(v.size_bytes)),
            ],
        ));
    }
    let source = t.t(match info.power.on_battery {
        Some(true) => "hw.power.battery",
        Some(false) => "hw.power.ac",
        None => "hw.power.unknown",
    });
    lines.push(t.f(
        "hw.power",
        &[
            ("plan", info.power.plan_name.as_ref().unwrap_or(&unknown)),
            ("source", &source),
        ],
    ));
    if info.games.is_empty() {
        lines.push(t.t("hw.no_games").to_owned());
    } else {
        let names: Vec<&str> = info.games.iter().map(|g| g.id.name()).collect();
        lines.push(t.f("hw.games", &[("list", &names.join(", "))]));
    }
    lines
}

/// Writes the full text report.
pub fn report(t: &Texts, info: &HwInfo, findings: &[Finding], out: &mut dyn Write) -> Result<()> {
    writeln!(out, "{}", t.t("health.hardware"))?;
    for line in hardware_lines(t, info) {
        writeln!(out, "  {line}")?;
    }
    writeln!(out)?;
    writeln!(out, "{}", t.t("health.checks"))?;

    let shown: Vec<&Finding> = findings
        .iter()
        .filter(|f| {
            matches!(
                f.outcome,
                Outcome::Problem | Outcome::Warning | Outcome::Info
            )
        })
        .collect();
    if shown.is_empty() {
        writeln!(out, "  {}", t.t("health.no_findings"))?;
    }
    for f in shown {
        for line in finding_lines(t, f) {
            writeln!(out, "{line}")?;
        }
        writeln!(out)?;
    }

    let titles = |outcome: Outcome| {
        let mut list: Vec<String> = Vec::new();
        for f in findings.iter().filter(|f| f.outcome == outcome) {
            let title = title(t, f.check);
            if !list.contains(&title) {
                list.push(title);
            }
        }
        list
    };
    let ok = titles(Outcome::Ok);
    if !ok.is_empty() {
        writeln!(
            out,
            "{}",
            t.f(
                "health.ok_summary",
                &[("count", &ok.len()), ("list", &ok.join(", "))]
            )
        )?;
    }
    let unknown = titles(Outcome::Unknown);
    if !unknown.is_empty() {
        writeln!(
            out,
            "{}",
            t.f("health.unknown_summary", &[("list", &unknown.join(", "))])
        )?;
    }
    Ok(())
}
