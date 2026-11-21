//! The helper's benchmark flow with a simulated PresentMon, key and clock (any OS).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

use pco_core::bench::Label;
use pco_core::guard::{Arch, Host};
use pco_core::journal::Journal;
use pco_core::recipe::Edition;
use pco_helper::bench::{run_bench, BenchDriver, BenchRequest};
use pco_helper::protocol::{parse_request, BlockReason, Event, Refusal, Request, Restart};

#[derive(Default)]
struct Fake {
    /// The game appears after this many checks (None = never).
    appears_after: Option<u32>,
    checks: u32,
    /// Answers to "wait for the key"; missing = pressed.
    keys: VecDeque<Result<bool, String>>,
    /// FPS of each capture; missing = PresentMon failure.
    fps: VecDeque<f64>,
    /// Set the cancel flag at this capture number.
    cancel_on_capture: Option<usize>,
    captures: Vec<(String, u32)>,
    beeps: u32,
    slept: u32,
}

impl BenchDriver for Fake {
    fn presentmon(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn running(&mut self, processes: &[String]) -> Option<String> {
        self.checks += 1;
        match self.appears_after {
            Some(n) if self.checks > n => processes.first().cloned(),
            _ => None,
        }
    }
    fn wait_key(&mut self, cancel: &AtomicBool) -> Result<bool, String> {
        if cancel.load(Ordering::SeqCst) {
            return Ok(false);
        }
        self.keys.pop_front().unwrap_or(Ok(true))
    }
    fn sleep(&mut self, seconds: u32, cancel: &AtomicBool) -> bool {
        self.slept += seconds;
        !cancel.load(Ordering::SeqCst)
    }
    fn beep(&mut self) {
        self.beeps += 1;
    }
    fn capture(
        &mut self,
        process: &str,
        seconds: u32,
        cancel: &AtomicBool,
    ) -> Result<String, String> {
        self.captures.push((process.to_owned(), seconds));
        if self.cancel_on_capture == Some(self.captures.len()) {
            cancel.store(true, Ordering::SeqCst);
            return Err("cancelled".into());
        }
        let fps = self.fps.pop_front().ok_or("simulated PresentMon failure")?;
        let mut csv = String::from("Application,SwapChainAddress,MsBetweenPresents\n");
        for _ in 0..(fps as usize * 10) {
            csv.push_str(&format!("{process},0x1,{:.6}\n", 1000.0 / fps));
        }
        Ok(csv)
    }
    fn now_ms(&mut self) -> i64 {
        1_800_000_000_000
    }
}

fn admin() -> Host {
    Host {
        build: 26100,
        arch: Arch::X64,
        elevated: true,
        edition: Some(Edition::Pro),
        form_factor: None,
    }
}

fn request(target: &str, runs: u32) -> BenchRequest<'_> {
    BenchRequest {
        target,
        after: false,
        runs,
        duration: 60,
        warmup: 10,
    }
}

fn run(
    fake: &mut Fake,
    host: &Host,
    req: &BenchRequest<'_>,
    cancel: &AtomicBool,
) -> (Vec<Event>, Journal) {
    let mut journal = Journal::open_in_memory().unwrap();
    let mut events = Vec::new();
    run_bench(&mut journal, host, fake, req, cancel, &mut |e| {
        events.push(e)
    });
    assert_eq!(
        events.iter().filter(|e| e.is_final()).count(),
        1,
        "{events:?}"
    );
    (events, journal)
}

fn kinds(events: &[Event]) -> Vec<&'static str> {
    events
        .iter()
        .map(|e| match e {
            Event::BenchWaitingGame { .. } => "game",
            Event::BenchWaitingKey { .. } => "key",
            Event::BenchMeasuring { .. } => "measuring",
            Event::BenchRun { .. } => "run",
            Event::BenchFinished { .. } => "finished",
            Event::Done { .. } => "done",
            Event::Error { .. } => "error",
            Event::Blocked { .. } => "blocked",
            _ => "other",
        })
        .collect()
}

#[test]
fn two_runs_started_with_the_key_are_measured_and_saved() {
    let mut fake = Fake {
        appears_after: Some(0),
        fps: VecDeque::from([140.0, 144.0]),
        ..Fake::default()
    };
    let never = AtomicBool::new(false);
    let (events, journal) = run(&mut fake, &admin(), &request("cs2", 2), &never);
    assert_eq!(
        kinds(&events),
        [
            "key",
            "measuring",
            "run",
            "key",
            "measuring",
            "run",
            "finished",
            "done"
        ]
    );
    assert_eq!(
        events.last(),
        Some(&Event::Done {
            restart: Restart::None
        })
    );
    assert_eq!(
        fake.captures,
        [("cs2.exe".into(), 60), ("cs2.exe".into(), 60)]
    );
    assert_eq!(fake.beeps, 4, "a beep at the start and the end of each run");
    assert_eq!(fake.slept, 20, "10 s warm-up before each run");
    let saved = journal
        .latest_series("cs2", Label::Before)
        .unwrap()
        .unwrap();
    assert_eq!(saved.runs.len(), 2);
    assert!((saved.runs[0].avg_fps - 140.0).abs() < 0.5);
    assert!(matches!(events[6], Event::BenchFinished { runs: 2, .. }));
}

#[test]
fn waits_for_the_game_the_app_started() {
    let mut fake = Fake {
        appears_after: Some(3),
        fps: VecDeque::from([60.0]),
        ..Fake::default()
    };
    let never = AtomicBool::new(false);
    let (events, _) = run(&mut fake, &admin(), &request("msedge.exe", 1), &never);
    assert_eq!(kinds(&events)[..2], ["game", "key"]);
    assert_eq!(kinds(&events).last(), Some(&"done"));

    let mut never_starts = Fake::default();
    let (events, _) = run(&mut never_starts, &admin(), &request("cs2", 1), &never);
    assert!(
        matches!(&events[..], [Event::BenchWaitingGame { .. }, Event::Error { message }] if message.contains("cs2.exe"))
    );
}

#[test]
fn needs_administrator_rights() {
    let mut fake = Fake::default();
    let host = Host {
        elevated: false,
        ..admin()
    };
    let never = AtomicBool::new(false);
    let (events, _) = run(&mut fake, &host, &request("cs2", 1), &never);
    assert_eq!(
        events,
        [Event::Blocked {
            reason: BlockReason::NeedsAdmin
        }]
    );
}

#[test]
fn stopping_keeps_the_runs_already_measured() {
    let mut fake = Fake {
        appears_after: Some(0),
        fps: VecDeque::from([100.0]),
        cancel_on_capture: Some(2),
        ..Fake::default()
    };
    let cancel = AtomicBool::new(false);
    let (events, journal) = run(&mut fake, &admin(), &request("cs2", 3), &cancel);
    assert!(matches!(
        events[events.len() - 2],
        Event::BenchFinished { runs: 1, .. }
    ));
    assert_eq!(
        journal
            .latest_series("cs2", Label::Before)
            .unwrap()
            .unwrap()
            .runs
            .len(),
        1
    );

    // Stopping before the first key: nothing saved, still a clean end.
    let mut fake = Fake {
        appears_after: Some(0),
        keys: VecDeque::from([Ok(false)]),
        ..Fake::default()
    };
    let never = AtomicBool::new(false);
    let (events, _) = run(&mut fake, &admin(), &request("cs2", 3), &never);
    assert_eq!(kinds(&events), ["key", "finished", "done"]);
}

#[test]
fn errors_are_reported() {
    // PresentMon fails on the second run: the first stays saved.
    let mut fake = Fake {
        appears_after: Some(0),
        fps: VecDeque::from([100.0]),
        ..Fake::default()
    };
    let never = AtomicBool::new(false);
    let (events, journal) = run(&mut fake, &admin(), &request("cs2", 2), &never);
    assert!(
        matches!(events.last(), Some(Event::Error { message }) if message.contains("PresentMon"))
    );
    assert_eq!(
        journal
            .latest_series("cs2", Label::Before)
            .unwrap()
            .unwrap()
            .runs
            .len(),
        1
    );

    let mut fake = Fake {
        appears_after: Some(0),
        keys: VecDeque::from([Err("Ctrl+Shift+F10 is taken by another program".to_owned())]),
        ..Fake::default()
    };
    let (events, _) = run(&mut fake, &admin(), &request("cs2", 1), &never);
    assert!(matches!(events.last(), Some(Event::Error { message }) if message.contains("F10")));
}

#[test]
fn bench_requests_are_checked() {
    let ok =
        r#"{"v":1,"op":"bench","target":"cs2","label":"after","runs":3,"duration":60,"warmup":10}"#;
    assert_eq!(
        parse_request(ok),
        Ok(Request::Bench {
            target: "cs2".into(),
            after: true,
            runs: 3,
            duration: 60,
            warmup: 10
        })
    );
    let req = Request::Bench {
        target: "League of Legends.exe".into(),
        after: false,
        runs: 1,
        duration: 10,
        warmup: 0,
    };
    assert_eq!(parse_request(&req.to_line()), Ok(req));
    let bad = [
        (
            r#"{"v":1,"op":"bench","target":"C:\\x\\evil.exe","label":"before","runs":3,"duration":60,"warmup":10}"#,
            Refusal::BadTarget("C:\\x\\evil.exe".into()),
        ),
        (
            r#"{"v":1,"op":"bench","target":"cs2","label":"during","runs":3,"duration":60,"warmup":10}"#,
            Refusal::BadShape("label".into()),
        ),
        (
            r#"{"v":1,"op":"bench","target":"cs2","label":"before","runs":9,"duration":60,"warmup":10}"#,
            Refusal::BadShape("runs".into()),
        ),
        (
            r#"{"v":1,"op":"bench","target":"cs2","label":"before","runs":3,"duration":5,"warmup":10}"#,
            Refusal::BadShape("duration".into()),
        ),
        (
            r#"{"v":1,"op":"bench","target":"cs2","label":"before","runs":3,"duration":60,"warmup":10,"args":"-x"}"#,
            Refusal::BadShape("args".into()),
        ),
    ];
    for (line, expected) in bad {
        assert_eq!(parse_request(line), Err(expected), "{line}");
    }
}
