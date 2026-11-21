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
    use pco_helper::bench::{run_bench, BenchRequest};
    use pco_helper::pipe::{self, Connection};
    use pco_helper::protocol::{self, Event, Refusal, Request, MAX_REQUEST, VERSION};

    use super::bench_driver;
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

    /// A Windows restore point, with Windows' own command (System Restore must be on).
    /// `Ok(Some(why))` when Windows skips it (one was already made in the last 24 hours).
    fn create_restore_point() -> Result<Option<String>, String> {
        let powershell = std::env::var_os("SystemRoot")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
            .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        let script = "$ErrorActionPreference='Stop'; \
            Checkpoint-Computer -Description 'PC Optimizer' -RestorePointType MODIFY_SETTINGS \
            -WarningVariable w -WarningAction SilentlyContinue; \
            if ($w) { Write-Output $w[0].Message; exit 3 }";
        let out = std::process::Command::new(powershell)
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output()
            .map_err(|e| e.to_string())?;
        let text = |b: &[u8]| String::from_utf8_lossy(b).trim().to_owned();
        match out.status.code() {
            Some(0) => Ok(None),
            Some(3) => Ok(Some(text(&out.stdout))),
            _ => Err(match text(&out.stderr) {
                e if e.is_empty() => text(&out.stdout),
                e => e,
            }),
        }
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
        if let Request::Bench {
            target,
            after,
            runs,
            duration,
            warmup,
        } = &request
        {
            // The app may send "cancel" while the benchmark runs; read it without ever
            // blocking the events this thread writes.
            let cancel = Arc::new(AtomicBool::new(false));
            if let Ok(mut reader) = conn.take_reader() {
                let flag = Arc::clone(&cancel);
                std::thread::spawn(move || loop {
                    match reader.poll_line(64) {
                        Ok(Some(Some(line))) if line == pipe::CANCEL => {
                            flag.store(true, Ordering::SeqCst);
                            return;
                        }
                        Ok(Some(Some(_))) | Ok(None) => {
                            std::thread::sleep(Duration::from_millis(100))
                        }
                        // The app is gone: stop too.
                        Ok(Some(None)) | Err(_) => {
                            flag.store(true, Ordering::SeqCst);
                            return;
                        }
                    }
                });
            }
            let mut driver = bench_driver::WindowsBench::default();
            let req = BenchRequest {
                target,
                after: *after,
                runs: *runs,
                duration: *duration,
                warmup: *warmup,
            };
            run_bench(
                &mut journal,
                &host,
                &mut driver,
                &req,
                &cancel,
                &mut |event| send(&mut conn, &event),
            );
            return ExitCode::SUCCESS;
        }

        let mut registry = WindowsRegistry::system();
        let mut services = WindowsServices::system();
        let mut restore_point = create_restore_point;
        let mut env = Env {
            registry: &mut registry,
            services: &mut services,
            journal: &mut journal,
            host,
            running,
            blocked,
            recipes,
            restore_point: &mut restore_point,
        };
        serve(&mut env, &request, &mut |event| send(&mut conn, &event));
        ExitCode::SUCCESS
    }
}

/// The real benchmark driver: the pinned PresentMon next to the helper, the global
/// Ctrl+Shift+F10 key while waiting, a short beep.
#[cfg(windows)]
mod bench_driver {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use pco_core::bench::presentmon;
    use pco_core::guard;
    use pco_helper::bench::BenchDriver;
    use windows::Win32::System::Diagnostics::Debug::MessageBeep;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, VK_F10,
    };
    use windows::Win32::UI::WindowsAndMessaging::{PeekMessageW, MB_OK, MSG, PM_REMOVE, WM_HOTKEY};

    /// Longest wait for the key before giving up (the user walked away).
    const KEY_TIMEOUT: Duration = Duration::from_secs(30 * 60);
    const HOTKEY_ID: i32 = 0x5043;

    #[derive(Default)]
    pub struct WindowsBench {
        presentmon: Option<PathBuf>,
    }

    impl BenchDriver for WindowsBench {
        fn presentmon(&mut self) -> Result<(), String> {
            let dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(PathBuf::from));
            let path = presentmon::locate(dir.as_deref()).map_err(|e| e.to_string())?;
            self.presentmon = Some(path);
            Ok(())
        }

        fn running(&mut self, processes: &[String]) -> Option<String> {
            let list = guard::windows::running_processes().ok()?;
            processes
                .iter()
                .find(|p| list.iter().any(|n| n.eq_ignore_ascii_case(p)))
                .cloned()
        }

        fn wait_key(&mut self, cancel: &AtomicBool) -> Result<bool, String> {
            // SAFETY: registers a hot key for this thread; unregistered before returning.
            unsafe {
                RegisterHotKey(
                    None,
                    HOTKEY_ID,
                    MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
                    u32::from(VK_F10.0),
                )
                .map_err(|_| "Ctrl+Shift+F10 is taken by another program".to_owned())?;
            }
            let deadline = Instant::now() + KEY_TIMEOUT;
            let pressed = loop {
                let mut msg = MSG::default();
                // SAFETY: reads this thread's own message queue.
                let got = unsafe { PeekMessageW(&mut msg, None, WM_HOTKEY, WM_HOTKEY, PM_REMOVE) };
                if got.as_bool() {
                    break true;
                }
                if cancel.load(Ordering::SeqCst) || Instant::now() >= deadline {
                    break false;
                }
                std::thread::sleep(Duration::from_millis(50));
            };
            // SAFETY: the same id registered above.
            let _ = unsafe { UnregisterHotKey(None, HOTKEY_ID) };
            Ok(pressed)
        }

        fn sleep(&mut self, seconds: u32, cancel: &AtomicBool) -> bool {
            let end = Instant::now() + Duration::from_secs(u64::from(seconds));
            while Instant::now() < end {
                if cancel.load(Ordering::SeqCst) {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            !cancel.load(Ordering::SeqCst)
        }

        fn beep(&mut self) {
            // SAFETY: plays the default system sound.
            let _ = unsafe { MessageBeep(MB_OK) };
        }

        fn capture(
            &mut self,
            process: &str,
            seconds: u32,
            cancel: &AtomicBool,
        ) -> Result<String, String> {
            let exe = self.presentmon.clone().ok_or("PresentMon not checked")?;
            presentmon::capture(&exe, process, seconds, cancel)
        }

        fn now_ms(&mut self) -> i64 {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
                .unwrap_or(0)
        }
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
