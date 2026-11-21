//! The Home page: the detected hardware as a compact list and the health findings as cards,
//! in the chosen language (the same texts as `pco health`).

use pco_core::health::{CheckId, Finding, Outcome};
use pco_core::hw::{DiskMedia, HwInfo};
use pco_core::i18n::Texts;
use pco_core::recipe::{Edition, FormFactor, Recipe};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HwRow {
    /// cpu, gpu, memory, display, disk, windows, power, games (the interface picks the icon).
    pub kind: &'static str,
    pub label: String,
    pub value: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingView {
    pub check: CheckId,
    pub outcome: Outcome,
    pub subject: Option<String>,
    pub title: String,
    pub message: String,
    pub fix: String,
    pub link: Option<&'static str>,
    /// The built-in recipe that fixes it ("Rezolvă"), when there is one.
    pub recipe: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayChoice {
    pub name: String,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeView {
    pub hardware: Vec<HwRow>,
    /// Problems, warnings and notes, most important first.
    pub findings: Vec<FindingView>,
    /// Checks that passed / could not be run, by title.
    pub ok: Vec<String>,
    pub unknown: Vec<String>,
    pub laptop: bool,
    /// More than one: the interface asks which one the user plays on.
    pub displays: Vec<DisplayChoice>,
    /// Parts of the hardware that could not be read.
    pub probe_errors: Vec<String>,
}

/// The recipe that fixes a check, if one is built in. Grows with the catalog (Part C).
fn fix_recipe(check: CheckId, recipes: &[Recipe]) -> Option<String> {
    let id = match check {
        CheckId::PowerPlan => "power.high-performance-plan",
        _ => return None,
    };
    recipes.iter().any(|r| r.id == id).then(|| id.to_owned())
}

/// "12th Gen Intel(R) Core(TM) i5-1245U" → "Intel Core i5-1245U".
fn clean_cpu(name: &str) -> String {
    let cleaned = name
        .replace("(R)", "")
        .replace("(TM)", "")
        .replace("(tm)", "");
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let skip = match words.as_slice() {
        [ordinal, gen, ..] if gen.eq_ignore_ascii_case("gen") && ordinal.ends_with("th") => 2,
        _ => 0,
    };
    words[skip..].join(" ")
}

fn gb(bytes: u64) -> u64 {
    (bytes + 512 * 1024 * 1024) / (1024 * 1024 * 1024)
}

fn edition(e: Option<Edition>) -> &'static str {
    match e {
        Some(Edition::Home) => "Home",
        Some(Edition::Pro) => "Pro",
        Some(Edition::Enterprise) => "Enterprise",
        Some(Edition::Education) => "Education",
        None => "",
    }
}

pub fn hardware(t: &Texts, info: &HwInfo) -> Vec<HwRow> {
    let unknown = "?".to_owned();
    let row = |kind, label: &str, value: String, detail: String| HwRow {
        kind,
        label: t.t(label).to_owned(),
        value,
        detail,
    };
    let mut rows = Vec::new();
    if let Some(cpu) = &info.cpu {
        rows.push(row(
            "cpu",
            "app.hw.cpu",
            clean_cpu(&cpu.name),
            t.f(
                "app.hw.cpu_detail",
                &[("cores", &cpu.cores), ("threads", &cpu.threads)],
            ),
        ));
    }
    for g in &info.gpus {
        let kind = t.t(match g.integrated {
            Some(true) => "hw.gpu.integrated",
            Some(false) => "hw.gpu.dedicated",
            None => "hw.gpu.unknown",
        });
        rows.push(row(
            "gpu",
            "app.hw.gpu",
            t.f(
                "app.hw.gpu_value",
                &[("name", &clean_cpu(&g.name)), ("kind", &kind)],
            ),
            t.f(
                "app.hw.gpu_detail",
                &[
                    ("version", g.driver_version.as_ref().unwrap_or(&unknown)),
                    ("date", g.driver_date.as_ref().unwrap_or(&unknown)),
                ],
            ),
        ));
    }
    if !info.memory.is_empty() {
        let total_mb: u64 = info.memory.iter().map(|m| m.capacity_mb).sum();
        let mhz = info
            .memory
            .iter()
            .filter_map(|m| m.configured_mhz)
            .min()
            .map_or(unknown.clone(), |m| m.to_string());
        rows.push(row(
            "memory",
            "app.hw.memory",
            t.f("app.hw.memory_value", &[("gb", &(total_mb / 1024))]),
            t.f(
                "app.hw.memory_detail",
                &[("modules", &info.memory.len()), ("mhz", &mhz)],
            ),
        ));
    }
    for d in &info.displays {
        rows.push(row(
            "display",
            "app.hw.display",
            t.f(
                "app.hw.display_value",
                &[
                    ("width", &d.width),
                    ("height", &d.height),
                    ("hz", &d.refresh_hz),
                ],
            ),
            d.name.clone(),
        ));
    }
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
        let free: Vec<String> = info
            .volumes
            .iter()
            .filter(|v| v.disk == Some(d.number))
            .map(|v| {
                t.f(
                    "app.hw.volume",
                    &[("letter", &v.letter), ("free", &gb(v.free_bytes))],
                )
            })
            .collect();
        rows.push(row(
            "disk",
            "app.hw.disk",
            t.f(
                "app.hw.disk_value",
                &[("media", &media), ("gb", &gb(d.size_bytes))],
            ),
            if free.is_empty() {
                d.model.clone()
            } else {
                free.join(", ")
            },
        ));
    }
    let w = &info.windows;
    let managed = w.domain_joined == Some(true) || w.mdm_enrolled == Some(true);
    rows.push(row(
        "windows",
        "app.hw.windows",
        t.f(
            "app.hw.windows_value",
            &[
                ("edition", &edition(w.edition)),
                ("version", w.display_version.as_ref().unwrap_or(&unknown)),
            ],
        ),
        t.f(
            if managed {
                "app.hw.windows_managed"
            } else {
                "app.hw.windows_detail"
            },
            &[("build", &w.build)],
        ),
    ));
    let form = t.t(match info.power.form_factor {
        Some(FormFactor::Laptop) => "hw.form.laptop",
        Some(FormFactor::Desktop) => "hw.form.desktop",
        _ => "hw.form.unknown",
    });
    let source = t.t(match info.power.on_battery {
        Some(true) => "hw.power.battery",
        Some(false) => "hw.power.ac",
        None => "hw.power.unknown",
    });
    rows.push(row(
        "power",
        "app.hw.power",
        t.f(
            "app.hw.power_value",
            &[("form", &form), ("source", &source)],
        ),
        t.f(
            "app.hw.power_detail",
            &[("plan", info.power.plan_name.as_ref().unwrap_or(&unknown))],
        ),
    ));
    let games: Vec<&str> = info.games.iter().map(|g| g.id.name()).collect();
    rows.push(row(
        "games",
        "app.hw.games",
        if games.is_empty() {
            t.t("app.hw.no_games").to_owned()
        } else {
            games.join(", ")
        },
        String::new(),
    ));
    rows
}

pub fn home_view(t: &Texts, info: &HwInfo, findings: &[Finding], recipes: &[Recipe]) -> HomeView {
    let titles = |outcome: Outcome| {
        let mut list: Vec<String> = Vec::new();
        for f in findings.iter().filter(|f| f.outcome == outcome) {
            let title = f.text(t).title;
            if !list.contains(&title) {
                list.push(title);
            }
        }
        list
    };
    HomeView {
        hardware: hardware(t, info),
        findings: findings
            .iter()
            .filter(|f| {
                matches!(
                    f.outcome,
                    Outcome::Problem | Outcome::Warning | Outcome::Info
                )
            })
            .map(|f| {
                let text = f.text(t);
                FindingView {
                    check: f.check,
                    outcome: f.outcome,
                    subject: f.subject.clone(),
                    title: text.title,
                    message: text.message,
                    fix: text.fix,
                    link: f.link,
                    recipe: fix_recipe(f.check, recipes),
                }
            })
            .collect(),
        ok: titles(Outcome::Ok),
        unknown: titles(Outcome::Unknown),
        laptop: info.power.form_factor == Some(FormFactor::Laptop),
        displays: info
            .displays
            .iter()
            .map(|d| DisplayChoice {
                name: d.name.clone(),
                primary: d.primary,
            })
            .collect(),
        probe_errors: info.probe_errors.clone(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use pco_core::recipe::Lang;

    fn dev_laptop() -> HwInfo {
        serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/hw/dev-laptop.json"
        ))
        .unwrap()
    }

    #[test]
    fn the_owners_laptop_in_romanian() {
        let t = Texts::load(Lang::Ro).unwrap();
        let info = dev_laptop();
        let findings = pco_core::health::run(
            &info,
            pco_core::health::days_from_date("2026-11-21").unwrap(),
        );
        let v = home_view(&t, &info, &findings, &[]);

        let row = |kind: &str| v.hardware.iter().find(|r| r.kind == kind).unwrap();
        assert_eq!(row("cpu").label, "Procesor");
        assert_eq!(row("cpu").value, "Intel Core i5-1245U");
        assert_eq!(row("cpu").detail, "10 nuclee, 12 fire");
        assert_eq!(row("memory").value, "16 GB");
        assert_eq!(row("memory").detail, "2 module, 3200 MT/s");
        assert_eq!(row("display").value, "1920 × 1200, 60 Hz");
        assert_eq!(row("disk").detail, "C: 376 GB liberi");
        assert_eq!(row("windows").value, "Windows 11 Pro 24H2");
        assert_eq!(row("power").value, "laptop, pe baterie");
        assert!(v.laptop);
        assert_eq!(v.displays.len(), 1);

        // On battery and VBS are reported; every card has its texts.
        assert!(v.findings.iter().any(|f| f.check == CheckId::OnBattery));
        assert!(v.findings.iter().any(|f| f.check == CheckId::Vbs));
        for f in &v.findings {
            assert!(!f.title.starts_with("health."), "{f:?}");
            assert!(!f.message.contains('{'), "{f:?}");
            assert!(!f.fix.is_empty());
        }
        assert!(!v.ok.is_empty());
        // Nothing built in fixes these yet.
        assert!(v.findings.iter().all(|f| f.recipe.is_none()));
    }

    #[test]
    fn english_and_cpu_names() {
        let t = Texts::load(Lang::En).unwrap();
        let v = home_view(&t, &dev_laptop(), &[], &[]);
        assert_eq!(v.hardware[0].label, "Processor");
        assert_eq!(v.hardware[0].detail, "10 cores, 12 threads");
        assert_eq!(
            clean_cpu("AMD Ryzen 7 7800X3D 8-Core Processor"),
            "AMD Ryzen 7 7800X3D 8-Core Processor"
        );
        assert_eq!(
            clean_cpu("13th Gen Intel(R) Core(TM) i9-13900K"),
            "Intel Core i9-13900K"
        );
    }
}
