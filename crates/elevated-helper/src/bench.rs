//! A benchmark series run by the elevated helper (PresentMon needs administrator rights),
//! started from the app. The user presses Ctrl+Shift+F10 in the match to start each run
//! (SPEC 4.4); a short beep marks the start and the end of each measurement.
//!
//! Everything that touches the system goes through [`BenchDriver`], so the flow is tested
//! with a simulated driver on any OS. It changes no setting: it only records results.

use std::sync::atomic::AtomicBool;

use pco_core::bench::{self, target, Label};
use pco_core::guard::{check_host, Host};
use pco_core::journal::Journal;

use crate::protocol::{BlockReason, Event, Restart};

/// The parts of a benchmark that touch the real system.
pub trait BenchDriver {
    /// Finds PresentMon and checks its SHA-256.
    fn presentmon(&mut self) -> Result<(), String>;
    /// The first of `processes` that is running.
    fn running(&mut self, processes: &[String]) -> Option<String>;
    /// Waits for Ctrl+Shift+F10. `Ok(false)` when cancelled or after a long wait.
    fn wait_key(&mut self, cancel: &AtomicBool) -> Result<bool, String>;
    /// `false` when cancelled meanwhile.
    fn sleep(&mut self, seconds: u32, cancel: &AtomicBool) -> bool;
    fn beep(&mut self);
    /// One timed PresentMon capture; the CSV text.
    fn capture(
        &mut self,
        process: &str,
        seconds: u32,
        cancel: &AtomicBool,
    ) -> Result<String, String>;
    /// Unix milliseconds.
    fn now_ms(&mut self) -> i64;
}

pub struct BenchRequest<'a> {
    pub target: &'a str,
    pub after: bool,
    pub runs: u32,
    pub duration: u32,
    pub warmup: u32,
}

/// How long to wait for the game to start, in 2-second steps (launchers can be slow).
const GAME_WAIT_STEPS: u32 = 90;

fn cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn run_bench(
    journal: &mut Journal,
    host: &Host,
    driver: &mut dyn BenchDriver,
    request: &BenchRequest<'_>,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Event),
) {
    let error = |emit: &mut dyn FnMut(Event), message: String| emit(Event::Error { message });
    if let Err(b) = check_host(host) {
        emit(Event::Blocked {
            reason: crate::serve::block(&b),
        });
        return;
    }
    // PresentMon traces frames with ETW, which needs administrator rights.
    if !host.elevated {
        emit(Event::Blocked {
            reason: BlockReason::NeedsAdmin,
        });
        return;
    }
    let Some(target) = target::resolve(request.target) else {
        return error(emit, format!("unknown target {}", request.target));
    };
    if let Err(e) = driver.presentmon() {
        return error(emit, format!("PresentMon: {e}"));
    }

    // The app starts the game; wait for its process.
    let mut process = driver.running(&target.processes);
    if process.is_none() {
        emit(Event::BenchWaitingGame {
            processes: target.processes.clone(),
        });
        for _ in 0..GAME_WAIT_STEPS {
            if !driver.sleep(2, cancel) {
                break;
            }
            process = driver.running(&target.processes);
            if process.is_some() {
                break;
            }
        }
    }
    let label = if request.after {
        Label::After
    } else {
        Label::Before
    };
    let series = driver.now_ms();
    let finish = |emit: &mut dyn FnMut(Event), runs: u32| {
        emit(Event::BenchFinished { series, runs });
        emit(Event::Done {
            restart: Restart::None,
        });
    };
    let Some(process) = process else {
        if cancelled(cancel) {
            return finish(emit, 0);
        }
        return error(
            emit,
            format!("not running: {}", target.processes.join(" / ")),
        );
    };

    let mut done = 0u32;
    for run in 1..=request.runs {
        emit(Event::BenchWaitingKey {
            run,
            runs: request.runs,
        });
        match driver.wait_key(cancel) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => return error(emit, e),
        }
        driver.beep();
        if !driver.sleep(request.warmup, cancel) {
            break;
        }
        emit(Event::BenchMeasuring {
            run,
            seconds: request.duration,
        });
        let csv = match driver.capture(&process, request.duration, cancel) {
            Ok(csv) => csv,
            Err(_) if cancelled(cancel) => break,
            Err(e) => return error(emit, e),
        };
        driver.beep();
        let metrics = match bench::frame_times(&csv).and_then(|t| bench::metrics(&t)) {
            Ok(m) => m,
            Err(e) => return error(emit, e.to_string()),
        };
        if let Err(e) = journal.record_benchmark(series, &target.key, label, &metrics) {
            return error(emit, e.to_string());
        }
        done += 1;
        emit(Event::BenchRun { run, metrics });
    }
    finish(emit, done);
}
