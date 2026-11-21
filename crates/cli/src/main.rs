//! `pco` binary: real Windows backends plugged into `pco_cli::app`.

use std::process::ExitCode;

use pco_cli::{args, i18n};

fn main() -> ExitCode {
    let argv: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let lang = args::lang_from_args(&argv).unwrap_or_else(i18n::system_lang);
    let texts = match i18n::Texts::load(lang) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    let cmd = match args::parse(argv, &texts) {
        Ok(cmd) => cmd,
        Err(e) => {
            let _ = e.print();
            // --help / --version are not errors.
            return ExitCode::from(u8::try_from(e.exit_code()).unwrap_or(1));
        }
    };
    real::run(&cmd, &texts)
}

#[cfg(windows)]
mod real {
    use std::path::PathBuf;
    use std::process::ExitCode;

    use pco_cli::app::{self, Env, Exit};
    use pco_cli::args::Cmd;
    use pco_cli::embedded::RECIPES;
    use pco_cli::i18n::Texts;
    use pco_cli::testing::{StopAfterChanges, STOP_AFTER_CHANGES_VAR};
    use pco_core::backend::windows::{WindowsRegistry, WindowsServices};
    use pco_core::backend::RegistryBackend;
    use pco_core::guard::{self, BlockedProcesses};
    use pco_core::journal::Journal;
    use pco_core::recipe;

    /// `%LOCALAPPDATA%\PcOptimizer`, or `PCO_DATA_DIR` (used by tests and the VM guide).
    fn data_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("PCO_DATA_DIR") {
            return Some(PathBuf::from(dir));
        }
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("PcOptimizer"))
    }

    /// `config\blocked-processes.yaml` next to `pco.exe` overrides the built-in list.
    fn blocked_processes() -> Result<BlockedProcesses, String> {
        let local = std::env::current_exe()
            .ok()
            .and_then(|exe| {
                exe.parent()
                    .map(|d| d.join("config").join("blocked-processes.yaml"))
            })
            .filter(|p| p.exists());
        match local {
            Some(path) => BlockedProcesses::load(&path),
            None => BlockedProcesses::builtin(),
        }
        .map_err(|e| e.to_string())
    }

    /// Days since 1970-01-01 (UTC).
    fn today() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_secs() / 86_400).unwrap_or(0))
            .unwrap_or(0)
    }

    fn fail(t: &Texts, details: &dyn std::fmt::Display) -> ExitCode {
        eprintln!("{}", t.f("error.generic", &[("details", details)]));
        ExitCode::from(Exit::Error as u8)
    }

    pub fn run(cmd: &Cmd, t: &Texts) -> ExitCode {
        let recipes = match recipe::load_embedded(RECIPES) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{}", t.f("error.recipes_invalid", &[("details", &e)]));
                return ExitCode::from(Exit::Error as u8);
            }
        };
        let blocked = match blocked_processes() {
            Ok(b) => b,
            Err(e) => return fail(t, &e),
        };
        let host = match guard::windows::host() {
            Ok(h) => h,
            Err(e) => return fail(t, &e),
        };
        let running = match guard::windows::running_processes() {
            Ok(r) => r,
            Err(e) => return fail(t, &e),
        };
        let Some(dir) = data_dir() else {
            return fail(t, &"LOCALAPPDATA is not set");
        };
        let mut journal = match Journal::open(&dir.join("journal.db")) {
            Ok(j) => j,
            Err(e) => return fail(t, &e),
        };
        // PCO_TEST_STOP_AFTER_CHANGES=N simulates a crash after N changes (VM and CI tests).
        let stop_after = std::env::var(STOP_AFTER_CHANGES_VAR)
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|n| *n > 0);
        let mut registry: Box<dyn RegistryBackend> = match stop_after {
            Some(n) => Box::new(StopAfterChanges::new(WindowsRegistry::system(), n)),
            None => Box::new(WindowsRegistry::system()),
        };
        let mut services = WindowsServices::system();
        let mut power = pco_core::backend::WindowsPower::system();
        let mut apps = pco_core::backend::WindowsAppx::system();
        let mut bench = pco_cli::bench_windows::WindowsBench::new();

        let mut env = Env {
            registry: &mut *registry,
            services: &mut services,
            power: &mut power,
            apps: &mut apps,
            journal: &mut journal,
            host,
            running,
            blocked,
            recipes,
            hw: matches!(cmd, Cmd::Health { .. }).then(pco_core::hw::windows::detect),
            today: today(),
            bench: &mut bench,
        };
        let mut out = std::io::stdout().lock();
        ExitCode::from(app::run(&mut env, t, cmd, &mut out) as u8)
    }
}

#[cfg(not(windows))]
mod real {
    use std::process::ExitCode;

    use pco_cli::args::Cmd;
    use pco_cli::i18n::Texts;

    pub fn run(_cmd: &Cmd, t: &Texts) -> ExitCode {
        eprintln!("{}", t.f("blocked.not_windows11", &[("build", &"-")]));
        ExitCode::from(2)
    }
}
