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
        // `list`, and nothing else is possible because a subcommand is required.
        _ => Cmd::List,
    })
}
