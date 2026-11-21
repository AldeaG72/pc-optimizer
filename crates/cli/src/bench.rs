//! `pco bench`: measure FPS with PresentMon before and after tweaks (Stage 2).
//!
//! Everything that touches the system (PresentMon, launching games, the keyboard, the clock)
//! goes through [`BenchDriver`], so the whole flow is tested with a simulated driver.

use std::io::Write;

use anyhow::Result;
use pco_core::bench::{self, compare, summarize, Comparison, Label, Metrics};
use pco_core::hw::GameId;

use crate::app::{Env, Exit};
use crate::args::{BenchCmd, BenchRun};
use crate::format;
use crate::i18n::Texts;

/// The parts of a benchmark that touch the real system.
pub trait BenchDriver {
    /// Finds PresentMon and checks its SHA-256.
    fn presentmon(&mut self) -> Result<(), String>;
    fn is_running(&mut self, process: &str) -> bool;
    /// Starts a game through its launcher.
    fn launch(&mut self, game: GameId) -> Result<(), String>;
    /// `false` when the user stopped (end of input).
    fn wait_for_enter(&mut self) -> bool;
    fn sleep(&mut self, seconds: u32);
    /// One timed PresentMon capture of `process`; returns the CSV text.
    fn capture(&mut self, process: &str, seconds: u32) -> Result<String, String>;
    /// Unix milliseconds.
    fn now_ms(&mut self) -> i64;
}

pub use pco_core::bench::target::{game_key, precise, resolve, Target};

pub fn run(env: &mut Env<'_>, t: &Texts, cmd: &BenchCmd, out: &mut dyn Write) -> Result<Exit> {
    match cmd {
        BenchCmd::Run(r) => run_series(env, t, r, out),
        BenchCmd::Compare { target } => {
            let Some(target) = resolve(target) else {
                writeln!(
                    out,
                    "{}",
                    t.f("bench.unknown_target", &[("target", target)])
                )?;
                return Ok(Exit::Error);
            };
            print_comparison(env, t, &target, out)
        }
        BenchCmd::History { target } => history(env, t, target.as_deref(), out),
        BenchCmd::Forget { target } => {
            let key = match target {
                Some(t_) => match resolve(t_) {
                    Some(r) => Some(r.key),
                    None => {
                        writeln!(out, "{}", t.f("bench.unknown_target", &[("target", t_)]))?;
                        return Ok(Exit::Error);
                    }
                },
                None => None,
            };
            let count = env.journal.forget_benchmarks(key.as_deref())?;
            writeln!(out, "{}", t.f("bench.forgot", &[("count", &count)]))?;
            Ok(Exit::Ok)
        }
    }
}

/// `142.3` in English, `142,3` in Romanian.
pub fn num(t: &Texts, v: f64) -> String {
    let s = format!("{v:.1}");
    match t.lang() {
        pco_core::recipe::Lang::Ro => s.replace('.', ","),
        pco_core::recipe::Lang::En => s,
    }
}

fn pct(t: &Texts, v: f64) -> String {
    let sign = if v >= 0.0 { "+" } else { "" };
    format!("{sign}{}%", num(t, v))
}

fn label_text(t: &Texts, label: Label) -> &str {
    t.t(match label {
        Label::Before => "bench.label.before",
        Label::After => "bench.label.after",
    })
}

fn run_series(env: &mut Env<'_>, t: &Texts, r: &BenchRun, out: &mut dyn Write) -> Result<Exit> {
    let Some(target) = resolve(&r.target) else {
        writeln!(
            out,
            "{}",
            t.f("bench.unknown_target", &[("target", &r.target)])
        )?;
        return Ok(Exit::Error);
    };
    // PresentMon traces frames with ETW, which needs administrator rights.
    if !env.host.elevated {
        writeln!(out, "{}", t.t("bench.needs_admin"))?;
        return Ok(Exit::Blocked);
    }
    if let Err(details) = env.bench.presentmon() {
        writeln!(
            out,
            "{}",
            t.f("bench.no_presentmon", &[("details", &details)])
        )?;
        return Ok(Exit::Error);
    }

    let label = if r.after { Label::After } else { Label::Before };
    let Some(process) = find_or_start(env, t, &target, r.launch, out)? else {
        return Ok(Exit::Error);
    };

    writeln!(
        out,
        "{}",
        t.f(
            "bench.intro",
            &[
                ("target", &target.key),
                ("runs", &r.runs),
                ("duration", &r.duration)
            ]
        )
    )?;
    if let Some(game) = target.game.filter(|g| !precise(*g)) {
        writeln!(
            out,
            "{}",
            t.f("bench.less_precise", &[("game", &game.name())])
        )?;
    }

    let series = env.bench.now_ms();
    let mut done = 0u32;
    for run in 1..=r.runs {
        if r.wait_for_enter {
            writeln!(
                out,
                "{}",
                t.f("bench.press_enter", &[("run", &run), ("runs", &r.runs)])
            )?;
            out.flush()?;
            if !env.bench.wait_for_enter() {
                writeln!(out, "{}", t.t("bench.cancelled"))?;
                break;
            }
        }
        writeln!(out, "{}", t.f("bench.warmup", &[("seconds", &r.warmup)]))?;
        out.flush()?;
        env.bench.sleep(r.warmup);
        writeln!(
            out,
            "{}",
            t.f("bench.measuring", &[("seconds", &r.duration)])
        )?;
        out.flush()?;
        let result = env
            .bench
            .capture(&process, r.duration)
            .and_then(|csv| bench::frame_times(&csv).map_err(|e| e.to_string()))
            .and_then(|times| bench::metrics(&times).map_err(|e| e.to_string()));
        match result {
            Ok(m) => {
                env.journal
                    .record_benchmark(series, &target.key, label, &m)?;
                done += 1;
                writeln!(
                    out,
                    "{}",
                    t.f(
                        "bench.run_result",
                        &[
                            ("run", &run),
                            ("fps", &num(t, m.avg_fps)),
                            ("low", &num(t, m.low1_fps)),
                            ("ft", &num(t, m.avg_frametime_ms)),
                        ]
                    )
                )?;
            }
            Err(details) => {
                writeln!(
                    out,
                    "{}",
                    t.f("bench.run_failed", &[("run", &run), ("details", &details)])
                )?;
                return Ok(Exit::Error);
            }
        }
    }
    if done == 0 {
        return Ok(Exit::Error);
    }
    writeln!(
        out,
        "{}",
        t.f(
            "bench.saved",
            &[
                ("runs", &done),
                ("label", &label_text(t, label)),
                ("target", &target.key)
            ]
        )
    )?;
    writeln!(out)?;

    match label {
        Label::Before => {
            writeln!(
                out,
                "{}",
                t.f("bench.next_before", &[("target", &target.key)])
            )?;
            Ok(Exit::Ok)
        }
        Label::After => print_comparison(env, t, &target, out),
    }
}

/// The running process to measure, starting the game first if needed.
fn find_or_start(
    env: &mut Env<'_>,
    t: &Texts,
    target: &Target,
    launch: bool,
    out: &mut dyn Write,
) -> Result<Option<String>> {
    let running = |env: &mut Env<'_>| {
        target
            .processes
            .iter()
            .find(|p| env.bench.is_running(p))
            .cloned()
    };
    if let Some(p) = running(env) {
        return Ok(Some(p));
    }
    let Some(game) = target.game.filter(|_| launch) else {
        writeln!(
            out,
            "{}",
            t.f(
                "bench.not_running",
                &[("process", &target.processes.join(" / "))]
            )
        )?;
        return Ok(None);
    };
    writeln!(out, "{}", t.f("bench.launching", &[("game", &game.name())]))?;
    if let Err(details) = env.bench.launch(game) {
        writeln!(
            out,
            "{}",
            t.f(
                "bench.launch_failed",
                &[("game", &game.name()), ("details", &details)]
            )
        )?;
        return Ok(None);
    }
    writeln!(
        out,
        "{}",
        t.f(
            "bench.waiting_start",
            &[("process", &target.processes.join(" / "))]
        )
    )?;
    out.flush()?;
    // Launchers can take a while (updates, sign-in): up to 3 minutes.
    for _ in 0..90 {
        if let Some(p) = running(env) {
            return Ok(Some(p));
        }
        env.bench.sleep(2);
    }
    writeln!(
        out,
        "{}",
        t.f(
            "bench.not_running",
            &[("process", &target.processes.join(" / "))]
        )
    )?;
    Ok(None)
}

fn print_comparison(
    env: &mut Env<'_>,
    t: &Texts,
    target: &Target,
    out: &mut dyn Write,
) -> Result<Exit> {
    let before = env.journal.latest_series(&target.key, Label::Before)?;
    let after = env.journal.latest_series(&target.key, Label::After)?;
    let (Some(before), Some(after)) = (before, after) else {
        let missing = if env
            .journal
            .latest_series(&target.key, Label::Before)?
            .is_none()
        {
            Label::Before
        } else {
            Label::After
        };
        if missing == Label::Before {
            writeln!(
                out,
                "{}",
                t.f("bench.no_before", &[("target", &target.key)])
            )?;
        } else {
            writeln!(
                out,
                "{}",
                t.f(
                    "bench.no_comparison",
                    &[("target", &target.key), ("label", &label_text(t, missing))]
                )
            )?;
        }
        return Ok(Exit::Ok);
    };
    let (Some(b), Some(a)) = (summarize(&before.runs), summarize(&after.runs)) else {
        return Ok(Exit::Ok);
    };
    let c = compare(&b, &a);
    write_comparison(t, &target.key, &c, out)?;
    if c.significant_drop {
        let ids = env.journal.applied_recipe_ids_since(before.id)?;
        if ids.is_empty() {
            writeln!(out, "{}", t.t("bench.drop_unknown"))?;
        } else {
            writeln!(
                out,
                "{}",
                t.f("bench.drop_undo", &[("ids", &ids.join(" "))])
            )?;
        }
    } else if c.fps_change_pct >= 1.0 && a.avg_fps - b.avg_fps > b.fps_spread.max(a.fps_spread) {
        writeln!(out, "{}", t.t("bench.better"))?;
    } else {
        writeln!(out, "{}", t.t("bench.same"))?;
    }
    Ok(Exit::Ok)
}

fn write_comparison(t: &Texts, key: &str, c: &Comparison, out: &mut dyn Write) -> Result<()> {
    writeln!(out, "{}", t.f("bench.compare_title", &[("target", &key)]))?;
    writeln!(
        out,
        "{}",
        t.f(
            "bench.compare_fps",
            &[
                ("before", &num(t, c.before.avg_fps)),
                ("after", &num(t, c.after.avg_fps)),
                ("change", &pct(t, c.fps_change_pct)),
            ]
        )
    )?;
    writeln!(
        out,
        "{}",
        t.f(
            "bench.compare_low",
            &[
                ("before", &num(t, c.before.low1_fps)),
                ("after", &num(t, c.after.low1_fps)),
                ("change", &pct(t, c.low1_change_pct)),
            ]
        )
    )?;
    writeln!(
        out,
        "{}",
        t.f(
            "bench.compare_ft",
            &[
                ("before", &num(t, c.before.avg_frametime_ms)),
                ("after", &num(t, c.after.avg_frametime_ms)),
            ]
        )
    )?;
    Ok(())
}

fn history(
    env: &mut Env<'_>,
    t: &Texts,
    target: Option<&str>,
    out: &mut dyn Write,
) -> Result<Exit> {
    let key = match target {
        Some(name) => match resolve(name) {
            Some(r) => Some(r.key),
            None => {
                writeln!(out, "{}", t.f("bench.unknown_target", &[("target", &name)]))?;
                return Ok(Exit::Error);
            }
        },
        None => None,
    };
    let all = env.journal.benchmark_series(key.as_deref())?;
    if all.is_empty() {
        writeln!(out, "{}", t.t("bench.history_empty"))?;
        return Ok(Exit::Ok);
    }
    writeln!(out, "{}", t.t("bench.history_title"))?;
    for s in &all {
        let Some(sum) = summarize(&s.runs) else {
            continue;
        };
        writeln!(
            out,
            "{}",
            t.f(
                "bench.history_line",
                &[
                    ("date", &format::time(s.id)),
                    ("target", &s.target),
                    ("label", &label_text(t, s.label)),
                    ("runs", &sum.runs),
                    ("fps", &num(t, sum.avg_fps)),
                    ("low", &num(t, sum.low1_fps)),
                ]
            )
        )?;
    }
    Ok(Exit::Ok)
}

/// Exposed for tests: metrics as the CLI would compute them from a CSV.
pub fn metrics_from_csv(csv: &str) -> Result<Metrics, String> {
    bench::frame_times(csv)
        .and_then(|times| bench::metrics(&times))
        .map_err(|e| e.to_string())
}
