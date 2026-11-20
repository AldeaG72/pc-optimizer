//! PresentMon CSV parsing on sample files: steady, stuttering, several swap chains, missing
//! values, broken rows, empty and wrong files.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt::Write as _;

use pco_core::bench::{frame_times, metrics, BenchError};

/// The default PresentMon 2.6 header (abridged to the columns that matter, in the real order).
const HEADER: &str = "Application,ProcessID,SwapChainAddress,PresentRuntime,SyncInterval,PresentFlags,\
AllowsTearing,PresentMode,CPUStartTime,MsCPUBusy,MsCPUWait,MsGPULatency,MsGPUTime,MsGPUBusy,MsGPUWait,\
DisplayLatency,DisplayedTime,MsBetweenPresents,MsInPresentAPI,MsBetweenDisplayChange,MsUntilDisplayed";

fn row(chain: &str, start: f64, between_presents: &str) -> String {
    format!(
        "cs2.exe,4242,{chain},DXGI,0,512,1,Hardware: Independent Flip,{start:.4},5.1,1.8,0.4,6.0,5.9,0.1,\
         9.2,6.9,{between_presents},0.2,6.9,2.1"
    )
}

fn csv(rows: &[String]) -> String {
    let mut s = String::from(HEADER);
    for r in rows {
        let _ = write!(s, "\n{r}");
    }
    s.push('\n');
    s
}

#[test]
fn steady_capture() {
    let rows: Vec<String> = (0..600)
        .map(|i| row("0x1A2B", i as f64 * 6.944, "6.944"))
        .collect();
    let m = metrics(&frame_times(&csv(&rows)).unwrap()).unwrap();
    assert_eq!(m.frames, 600);
    assert!((m.avg_fps - 144.0).abs() < 0.1, "{m:?}");
    assert!((m.low1_fps - 144.0).abs() < 0.1, "{m:?}");
}

#[test]
fn main_swap_chain_wins_and_bad_values_are_skipped() {
    let mut rows: Vec<String> = (0..500).map(|i| row("0xGAME", i as f64, "8.000")).collect();
    // A small overlay swap chain at 30 FPS in the same process.
    rows.extend((0..40).map(|i| row("0xOVERLAY", i as f64, "33.333")));
    // Values PresentMon writes when it does not know, and junk.
    rows.push(row("0xGAME", 0.0, "NA"));
    rows.push(row("0xGAME", 0.0, "-1"));
    rows.push(row("0xGAME", 0.0, "0"));
    rows.push("garbage line".into());
    let times = frame_times(&csv(&rows)).unwrap();
    assert_eq!(times.len(), 500);
    let m = metrics(&times).unwrap();
    assert!((m.avg_fps - 125.0).abs() < 1e-6, "{m:?}");
}

#[test]
fn stutters_show_in_the_1_percent_low() {
    let mut rows: Vec<String> = (0..990).map(|i| row("0x1", i as f64, "5.0")).collect();
    rows.extend((0..10).map(|i| row("0x1", i as f64, "40.0")));
    let m = metrics(&frame_times(&csv(&rows)).unwrap()).unwrap();
    assert!((m.low1_fps - 25.0).abs() < 1e-9, "{m:?}");
    assert!(m.avg_fps > 180.0);
}

#[test]
fn windows_line_endings_and_case() {
    let rows: Vec<String> = (0..100).map(|i| row("0x1", i as f64, "10.0")).collect();
    let text = csv(&rows)
        .replace('\n', "\r\n")
        .replace("MsBetweenPresents", "msbetweenpresents");
    assert_eq!(frame_times(&text).unwrap().len(), 100);
}

#[test]
fn empty_and_wrong_files() {
    assert_eq!(frame_times(""), Err(BenchError::Empty));
    assert_eq!(
        frame_times("Application,ProcessID\ncs2.exe,1\n"),
        Err(BenchError::MissingColumn("MsBetweenPresents"))
    );
    // Header only: no frames, which `metrics` reports clearly.
    let times = frame_times(&csv(&[])).unwrap();
    assert!(matches!(
        metrics(&times),
        Err(BenchError::TooFewFrames { frames: 0, .. })
    ));
}

#[test]
fn falls_back_to_ms_between_app_start() {
    let text = "Application,SwapChainAddress,MsBetweenAppStart\n".to_owned()
        + &(0..50).map(|_| "game.exe,0x1,20.0\n").collect::<String>();
    let m = metrics(&frame_times(&text).unwrap()).unwrap();
    assert!((m.avg_fps - 50.0).abs() < 1e-9);
}
