//! Turning engine results into user-facing text.

use pco_core::backend::{RegRead, ServiceStart};
use pco_core::engine::{NotApplicable, RecipeStatus, Snapshot};
use pco_core::guard::{Arch, Blocked, BlockedProcess, PlatformMismatch};
use pco_core::recipe::{Category, Edition, FormFactor, RegValue, RequiresRestart, Risk};

use crate::i18n::Texts;

pub fn blocked(t: &Texts, b: &Blocked) -> String {
    match b {
        Blocked::NotWindows11 { build } => t.f("blocked.not_windows11", &[("build", build)]),
        Blocked::NotX64 { arch } => t.f("blocked.not_x64", &[("arch", &arch_name(*arch))]),
        Blocked::NeedsAdmin => t.t("blocked.needs_admin").to_owned(),
        Blocked::GameRunning { running } => {
            t.f("blocked.game_running", &[("list", &process_list(running))])
        }
    }
}

pub fn process_list(list: &[BlockedProcess]) -> String {
    list.iter()
        .map(|p| format!("{} ({})", p.what, p.name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn arch_name(arch: Arch) -> String {
    arch.to_string()
}

pub fn state(t: &Texts, s: &Snapshot) -> String {
    match s {
        Snapshot::Registry { state, .. } => match state {
            RegRead::KeyMissing => t.t("state.key_missing").to_owned(),
            RegRead::ValueMissing => t.t("state.value_missing").to_owned(),
            RegRead::Present(data) => match data.to_value() {
                Some(RegValue::Dword(v)) => t.f("state.number", &[("value", &v)]),
                Some(RegValue::Qword(v)) => t.f("state.number", &[("value", &v)]),
                Some(RegValue::String(v)) => t.f("state.text", &[("value", &v)]),
                None => t.f(
                    "state.raw",
                    &[("type", &data.reg_type), ("len", &data.bytes.len())],
                ),
            },
        },
        Snapshot::Service { state, .. } => {
            let key = match (state.start, state.delayed_auto_start) {
                (ServiceStart::Boot, _) => "start.boot",
                (ServiceStart::System, _) => "start.system",
                (ServiceStart::Automatic, true) => "start.automatic_delayed",
                (ServiceStart::Automatic, false) => "start.automatic",
                (ServiceStart::Manual, _) => "start.manual",
                (ServiceStart::Disabled, _) => "start.disabled",
            };
            t.f("state.service", &[("start", &t.t(key))])
        }
    }
}

pub fn not_applicable(t: &Texts, n: &NotApplicable) -> String {
    match n {
        NotApplicable::ServiceMissing { name } => t.f("na.service_missing", &[("name", name)]),
        NotApplicable::Platform(p) => match p {
            PlatformMismatch::BuildTooOld { required, current } => t.f(
                "na.build_too_old",
                &[("required", required), ("current", current)],
            ),
            PlatformMismatch::BuildTooNew { max, current } => {
                t.f("na.build_too_new", &[("max", max), ("current", current)])
            }
            PlatformMismatch::Edition { allowed, current } => {
                let allowed = allowed
                    .iter()
                    .map(|e| edition(*e))
                    .collect::<Vec<_>>()
                    .join(" / ");
                let current = current.map_or_else(
                    || t.t("na.unknown_edition").to_owned(),
                    |e| edition(e).to_owned(),
                );
                t.f(
                    "na.edition",
                    &[("allowed", &allowed), ("current", &current)],
                )
            }
            PlatformMismatch::FormFactor { required, .. } => {
                let key = match required {
                    FormFactor::Laptop => "form.laptop",
                    FormFactor::Desktop | FormFactor::Any => "form.desktop",
                };
                t.f("na.form_factor", &[("required", &t.t(key))])
            }
        },
    }
}

pub fn recipe_status(t: &Texts, s: &RecipeStatus) -> String {
    match s {
        RecipeStatus::NotApplied => t.t("status.not_applied").to_owned(),
        RecipeStatus::Applied => t.t("status.applied").to_owned(),
        RecipeStatus::AlreadyInPlace => t.t("status.already").to_owned(),
        RecipeStatus::ModifiedOutside => t.t("status.modified").to_owned(),
        RecipeStatus::NotApplicable(n) => t.f(
            "status.not_applicable",
            &[("reason", &not_applicable(t, n))],
        ),
    }
}

fn edition(e: Edition) -> &'static str {
    match e {
        Edition::Home => "Home",
        Edition::Pro => "Pro",
        Edition::Enterprise => "Enterprise",
        Edition::Education => "Education",
    }
}

pub fn risk(t: &Texts, r: Risk) -> &str {
    t.t(match r {
        Risk::Safe => "risk.safe",
        Risk::Moderate => "risk.moderate",
        Risk::Advanced => "risk.advanced",
    })
}

pub fn category(c: Category) -> &'static str {
    match c {
        Category::Windows => "windows",
        Category::Gaming => "gaming",
        Category::Power => "power",
        Category::Input => "input",
        Category::Network => "network",
        Category::Audio => "audio",
        Category::Debloat => "debloat",
        Category::Privacy => "privacy",
        Category::Cleanup => "cleanup",
        Category::GpuNvidia => "gpu-nvidia",
        Category::GpuAmd => "gpu-amd",
        Category::Services => "services",
        Category::Advanced => "advanced",
    }
}

/// The text for the strongest restart need, or `None` if nothing is needed.
pub fn restart(t: &Texts, r: RequiresRestart) -> Option<&str> {
    match r {
        RequiresRestart::None => None,
        RequiresRestart::Explorer => Some(t.t("restart.explorer")),
        RequiresRestart::Signout => Some(t.t("restart.signout")),
        RequiresRestart::Reboot => Some(t.t("restart.reboot")),
    }
}

pub fn restart_rank(r: RequiresRestart) -> u8 {
    match r {
        RequiresRestart::None => 0,
        RequiresRestart::Explorer => 1,
        RequiresRestart::Signout => 2,
        RequiresRestart::Reboot => 3,
    }
}

/// Unix milliseconds as `YYYY-MM-DD HH:MM UTC`.
pub fn time(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        (rem % 3600) / 60
    )
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(time(0), "1970-01-01 00:00 UTC");
        // 2026-11-20 19:05:00 UTC
        assert_eq!(time(1_795_201_500_000), "2026-11-20 19:05 UTC");
        assert_eq!(time(951_782_400_000), "2000-02-29 00:00 UTC");
    }
}
