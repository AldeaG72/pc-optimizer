//! `pco-helper.exe --pipe <name> --parent <pid>`: started by the app (with UAC), runs one
//! request and exits. Not meant to be started by hand.

use std::process::ExitCode;

fn main() -> ExitCode {
    real::main()
}

#[cfg(windows)]
mod real {
    use std::path::PathBuf;
    use std::process::ExitCode;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use pco_core::backend::windows::{WindowsRegistry, WindowsServices};
    use pco_core::guard::{self, BlockedProcesses};
    use pco_core::journal::Journal;
    use pco_core::recipe;
    use pco_helper::pipe::{self, Connection};
    use pco_helper::protocol::{self, Event, Refusal, MAX_REQUEST, VERSION};
    use pco_helper::serve::{serve, Env};

    /// Exit codes: 0 the request ran (whatever its outcome); 64 wrong arguments; 65 no
    /// channel; 66 the other end was refused.
    const BAD_ARGS: u8 = 64;
    const NO_CHANNEL: u8 = 65;
    const REFUSED: u8 = 66;

    fn args() -> Option<(String, u32)> {
        let mut it = std::env::args().skip(1);
        let (mut pipe, mut parent) = (None, None);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--pipe" => pipe = it.next(),
                "--parent" => parent = it.next().and_then(|p| p.parse().ok()),
                _ => return None,
            }
        }
        Some((pipe?, parent?))
    }

    /// `%LOCALAPPDATA%\PcOptimizer`, or `PCO_DATA_DIR` (tests), like `pco`.
    fn data_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("PCO_DATA_DIR") {
            return Some(PathBuf::from(dir));
        }
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("PcOptimizer"))
    }

    /// `config\blocked-processes.yaml` next to the helper overrides the built-in list, like `pco`.
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

    fn send(conn: &mut Connection, event: &Event) {
        // If the app went away, keep going: the work must finish so the journal stays whole.
        let _ = conn.write_line(&event.to_line());
    }

    pub fn main() -> ExitCode {
        let Some((name, parent)) = args() else {
            eprintln!("pco-helper is started by PC Optimizer; it is not meant to be run by hand.");
            return ExitCode::from(BAD_ARGS);
        };
        let Ok(mut conn) = pipe::connect(&name, Duration::from_secs(10)) else {
            return ExitCode::from(NO_CHANNEL);
        };

        // The pipe must belong to the process that started us, under the same Windows user.
        if conn.server_pid().ok() != Some(parent) {
            send(
                &mut conn,
                &Event::Refused {
                    reason: Refusal::WrongPeer,
                },
            );
            return ExitCode::from(REFUSED);
        }
        if !pipe::same_user_as(parent).unwrap_or(false) {
            send(
                &mut conn,
                &Event::Refused {
                    reason: Refusal::OtherUser,
                },
            );
            return ExitCode::from(REFUSED);
        }

        send(&mut conn, &Event::Hello { version: VERSION });

        // Do not wait for a request forever.
        let got_request = Arc::new(AtomicBool::new(false));
        {
            let got = Arc::clone(&got_request);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(60));
                if !got.load(Ordering::SeqCst) {
                    std::process::exit(i32::from(NO_CHANNEL));
                }
            });
        }
        let line = match conn.read_line(MAX_REQUEST) {
            Ok(Some(line)) => line,
            Ok(None) => return ExitCode::from(NO_CHANNEL),
            Err(_) => {
                got_request.store(true, Ordering::SeqCst);
                send(
                    &mut conn,
                    &Event::Refused {
                        reason: Refusal::TooLarge,
                    },
                );
                return ExitCode::SUCCESS;
            }
        };
        got_request.store(true, Ordering::SeqCst);

        let request = match protocol::parse_request(&line) {
            Ok(r) => r,
            Err(reason) => {
                send(&mut conn, &Event::Refused { reason });
                return ExitCode::SUCCESS;
            }
        };

        let fail = |conn: &mut Connection, message: String| {
            send(conn, &Event::Error { message });
            ExitCode::SUCCESS
        };
        let recipes = match recipe::load_embedded(pco_core::embedded::RECIPES) {
            Ok(r) => r,
            Err(e) => return fail(&mut conn, e.to_string()),
        };
        let blocked = match blocked_processes() {
            Ok(b) => b,
            Err(e) => return fail(&mut conn, e),
        };
        let host = match guard::windows::host() {
            Ok(h) => h,
            Err(e) => return fail(&mut conn, e.to_string()),
        };
        let running = match guard::windows::running_processes() {
            Ok(r) => r,
            Err(e) => return fail(&mut conn, e.to_string()),
        };
        let Some(dir) = data_dir() else {
            return fail(&mut conn, "LOCALAPPDATA is not set".into());
        };
        let mut journal = match Journal::open(&dir.join("journal.db")) {
            Ok(j) => j,
            Err(e) => return fail(&mut conn, e.to_string()),
        };
        let mut registry = WindowsRegistry::system();
        let mut services = WindowsServices::system();
        let mut env = Env {
            registry: &mut registry,
            services: &mut services,
            journal: &mut journal,
            host,
            running,
            blocked,
            recipes,
        };
        serve(&mut env, &request, &mut |event| send(&mut conn, &event));
        ExitCode::SUCCESS
    }
}

#[cfg(not(windows))]
mod real {
    use std::process::ExitCode;

    pub fn main() -> ExitCode {
        eprintln!("pco-helper runs on Windows only");
        ExitCode::from(64)
    }
}
