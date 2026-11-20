//! Command-line arguments. Built with clap's builder API so the help text can be translated.

use std::ffi::OsString;

use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};
use pco_core::recipe::Lang;

use crate::i18n::Texts;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    List,
    Plan { ids: Vec<String> },
    Apply { ids: Vec<String>, dry_run: bool },
    Undo { ids: Vec<String>, all: bool },
    Status,
    Health { json: bool },
    Bench(BenchCmd),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BenchCmd {
    Run(BenchRun),
    Compare { target: String },
    History { target: Option<String> },
    Forget { target: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchRun {
    /// A game id (`cs2`, `valorant`, …) or a process name (`msedge.exe`).
    pub target: String,
    pub after: bool,
    pub runs: u32,
    /// Seconds measured per run (SPEC: 60).
    pub duration: u32,
    /// Seconds to wait after Enter (SPEC: 10).
    pub warmup: u32,
    /// Start the game through its launcher if it is not running.
    pub launch: bool,
    /// Wait for Enter before each run (off for automated tests).
    pub wait_for_enter: bool,
}

impl Cmd {
    /// Whether the command changes settings (and so needs every guard).
    pub fn writes(&self) -> bool {
        matches!(self, Cmd::Apply { dry_run: false, .. } | Cmd::Undo { .. })
    }
}

/// `--lang ro|en`, read before full parsing so the help itself is in that language.
pub fn lang_from_args(args: &[OsString]) -> Option<Lang> {
    let mut iter = args.iter().map(|a| a.to_string_lossy());
    while let Some(arg) = iter.next() {
        let value = if arg == "--lang" {
            iter.next().map(|v| v.into_owned())
        } else {
            arg.strip_prefix("--lang=").map(str::to_owned)
        };
        match value.as_deref() {
            Some("ro") => return Some(Lang::Ro),
            Some("en") => return Some(Lang::En),
            _ => {}
        }
    }
    None
}

fn ids_arg(texts: &Texts, required: bool) -> Arg {
    Arg::new("ids")
        .value_name("ID")
        .num_args(1..)
        .required(required)
        .help(texts.t("help.ids").to_owned())
}

pub fn command(texts: &Texts) -> Command {
    Command::new("pco")
        .version(pco_core::VERSION)
        .about(texts.t("help.about").to_owned())
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new("lang")
                .long("lang")
                .global(true)
                .value_parser(["ro", "en"])
                .help(texts.t("help.lang").to_owned()),
        )
        .subcommand(Command::new("list").about(texts.t("help.list").to_owned()))
        .subcommand(
            Command::new("plan")
                .about(texts.t("help.plan").to_owned())
                .arg(ids_arg(texts, true)),
        )
        .subcommand(
            Command::new("apply")
                .about(texts.t("help.apply").to_owned())
                .arg(ids_arg(texts, true))
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .action(ArgAction::SetTrue)
                        .help(texts.t("help.dry_run").to_owned()),
                ),
        )
        .subcommand(
            Command::new("undo")
                .about(texts.t("help.undo").to_owned())
                .arg(ids_arg(texts, false))
                .arg(
                    Arg::new("all")
                        .long("all")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("ids")
                        .help(texts.t("help.all").to_owned()),
                )
                .group(ArgGroup::new("what").args(["ids", "all"]).required(true)),
        )
        .subcommand(Command::new("status").about(texts.t("help.status").to_owned()))
        .subcommand(bench_command(texts))
        .subcommand(
            Command::new("health")
                .about(texts.t("help.health").to_owned())
                .arg(
                    Arg::new("json")
                        .long("json")
                        .action(ArgAction::SetTrue)
                        .help(texts.t("help.json").to_owned()),
                ),
        )
}

fn number(name: &'static str, default: &'static str, help: String) -> Arg {
    Arg::new(name)
        .long(name)
        .value_name("N")
        .default_value(default)
        .value_parser(clap::value_parser!(u32).range(1..=3600))
        .help(help)
}

fn bench_command(texts: &Texts) -> Command {
    let target = |required| {
        Arg::new("target")
            .value_name("GAME|PROCESS.exe")
            .required(required)
            .help(texts.t("help.bench_target").to_owned())
    };
    Command::new("bench")
        .about(texts.t("help.bench").to_owned())
        .subcommand_required(true)
        .subcommand(
            Command::new("run")
                .about(texts.t("help.bench_run").to_owned())
                .arg(target(true))
                .arg(
                    Arg::new("before")
                        .long("before")
                        .action(ArgAction::SetTrue)
                        .help(texts.t("help.bench_before").to_owned()),
                )
                .arg(
                    Arg::new("after")
                        .long("after")
                        .action(ArgAction::SetTrue)
                        .help(texts.t("help.bench_after").to_owned()),
                )
                .group(
                    ArgGroup::new("label")
                        .args(["before", "after"])
                        .required(true),
                )
                .arg(number("runs", "3", texts.t("help.bench_runs").to_owned()))
                .arg(number(
                    "duration",
                    "60",
                    texts.t("help.bench_duration").to_owned(),
                ))
                .arg(number(
                    "warmup",
                    "10",
                    texts.t("help.bench_warmup").to_owned(),
                ))
                .arg(
                    Arg::new("no-launch")
                        .long("no-launch")
                        .action(ArgAction::SetTrue)
                        .help(texts.t("help.bench_no_launch").to_owned()),
                )
                .arg(
                    Arg::new("start-now")
                        .long("start-now")
                        .action(ArgAction::SetTrue)
                        .help(texts.t("help.bench_start_now").to_owned()),
                ),
        )
        .subcommand(
            Command::new("compare")
                .about(texts.t("help.bench_compare").to_owned())
                .arg(target(true)),
        )
        .subcommand(
            Command::new("history")
                .about(texts.t("help.bench_history").to_owned())
                .arg(target(false)),
        )
        .subcommand(
            Command::new("forget")
                .about(texts.t("help.bench_forget").to_owned())
                .arg(target(false))
                .arg(
                    Arg::new("all")
                        .long("all")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("target")
                        .help(texts.t("help.bench_forget_all").to_owned()),
                )
                .group(ArgGroup::new("what").args(["target", "all"]).required(true)),
        )
}

fn bench_cmd(m: &ArgMatches) -> BenchCmd {
    match m.subcommand() {
        Some(("run", r)) => {
            let num = |name| r.get_one::<u32>(name).copied().unwrap_or(1);
            BenchCmd::Run(BenchRun {
                target: r.get_one::<String>("target").cloned().unwrap_or_default(),
                after: r.get_flag("after"),
                runs: num("runs"),
                duration: num("duration"),
                warmup: num("warmup"),
                launch: !r.get_flag("no-launch"),
                wait_for_enter: !r.get_flag("start-now"),
            })
        }
        Some(("compare", c)) => BenchCmd::Compare {
            target: c.get_one::<String>("target").cloned().unwrap_or_default(),
        },
        Some(("forget", f)) => BenchCmd::Forget {
            target: f.get_one::<String>("target").cloned(),
        },
        Some(("history", h)) => BenchCmd::History {
            target: h.get_one::<String>("target").cloned(),
        },
        // A subcommand is required, so only `history` without a target can be meant here.
        _ => BenchCmd::History { target: None },
    }
}

fn ids(m: &ArgMatches) -> Vec<String> {
    m.get_many::<String>("ids")
        .map(|v| v.cloned().collect())
        .unwrap_or_default()
}

pub fn parse(args: Vec<OsString>, texts: &Texts) -> Result<Cmd, clap::Error> {
    let matches = command(texts).try_get_matches_from(args)?;
    Ok(match matches.subcommand() {
        Some(("plan", m)) => Cmd::Plan { ids: ids(m) },
        Some(("apply", m)) => Cmd::Apply {
            ids: ids(m),
            dry_run: m.get_flag("dry-run"),
        },
        Some(("undo", m)) => Cmd::Undo {
            ids: ids(m),
            all: m.get_flag("all"),
        },
        Some(("status", _)) => Cmd::Status,
        Some(("bench", b)) => Cmd::Bench(bench_cmd(b)),
        Some(("health", m)) => Cmd::Health {
            json: m.get_flag("json"),
        },
        // `list`, and nothing else is possible because a subcommand is required.
        _ => Cmd::List,
    })
}
