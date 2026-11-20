//! Benchmark (Stage 2): PresentMon captures, metrics and the before/after verdict.
//!
//! Everything here is pure (parsing, statistics, verdict) except `presentmon`'s file check,
//! so it is tested on sample CSV files on any OS.

pub mod presentmon;

use std::collections::HashMap;

use serde::Serialize;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum BenchError {
    #[error("PresentMon output has no `{0}` column")]
    MissingColumn(&'static str),
    #[error("PresentMon output is empty")]
    Empty,
    #[error(
        "only {frames} frames were captured; at least {min} are needed (was the game in focus?)"
    )]
    TooFewFrames { frames: usize, min: usize },
}

/// Frames needed for a meaningful result (a few seconds at low FPS).
pub const MIN_FRAMES: usize = 30;

/// One run's result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Metrics {
    pub frames: u64,
    pub seconds: f64,
    /// Frames divided by time.
    pub avg_fps: f64,
    /// "1% low": average FPS over the slowest 1% of frames.
    pub low1_fps: f64,
    pub avg_frametime_ms: f64,
}

/// Frame times (ms) of the main swap chain in a PresentMon CSV.
///
/// Uses `MsBetweenPresents` (time between two Present() calls), falling back to
/// `MsBetweenAppStart`. Rows with `NA` or non-positive values are skipped. When a process
/// has several swap chains (e.g. a launcher overlay), the one with the most frames wins.
pub fn frame_times(csv: &str) -> Result<Vec<f64>, BenchError> {
    let mut lines = csv.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<&str> = lines
        .next()
        .ok_or(BenchError::Empty)?
        .split(',')
        .map(str::trim)
        .collect();
    let col = |name: &str| header.iter().position(|h| h.eq_ignore_ascii_case(name));
    let time_col = col("MsBetweenPresents")
        .or_else(|| col("MsBetweenAppStart"))
        .ok_or(BenchError::MissingColumn("MsBetweenPresents"))?;
    let chain_col = col("SwapChainAddress");

    let mut chains: HashMap<String, Vec<f64>> = HashMap::new();
    for line in lines {
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        let Some(ms) = fields.get(time_col).and_then(|v| v.parse::<f64>().ok()) else {
            continue;
        };
        if !ms.is_finite() || ms <= 0.0 {
            continue;
        }
        let chain = chain_col
            .and_then(|c| fields.get(c))
            .map_or(String::new(), |c| (*c).to_owned());
        chains.entry(chain).or_default().push(ms);
    }
    Ok(chains
        .into_values()
        .max_by_key(Vec::len)
        .unwrap_or_default())
}

/// Metrics from frame times.
pub fn metrics(frame_times_ms: &[f64]) -> Result<Metrics, BenchError> {
    if frame_times_ms.len() < MIN_FRAMES {
        return Err(BenchError::TooFewFrames {
            frames: frame_times_ms.len(),
            min: MIN_FRAMES,
        });
    }
    let total_ms: f64 = frame_times_ms.iter().sum();
    let frames = frame_times_ms.len();
    // "1% low": the average FPS over the slowest 1% of frames. (The 99th-percentile frame time
    // can sit just below the stutters when they are exactly 1% of the frames, and then hides
    // them; the average of the worst 1% always includes them.)
    let mut slowest = frame_times_ms.to_vec();
    slowest.sort_by(|a, b| b.total_cmp(a));
    let count = frames.div_ceil(100).max(1);
    let worst_mean_ms = slowest[..count].iter().sum::<f64>() / count as f64;
    Ok(Metrics {
        frames: frames as u64,
        seconds: total_ms / 1000.0,
        avg_fps: frames as f64 * 1000.0 / total_ms,
        low1_fps: 1000.0 / worst_mean_ms,
        avg_frametime_ms: total_ms / frames as f64,
    })
}

/// A drop counts only if it is bigger than this share of the "before" FPS…
pub const DROP_THRESHOLD: f64 = 0.03;

/// Mean and spread (max − min) of several runs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Summary {
    pub runs: usize,
    pub avg_fps: f64,
    pub low1_fps: f64,
    pub avg_frametime_ms: f64,
    /// max − min of the runs' average FPS.
    pub fps_spread: f64,
}

pub fn summarize(runs: &[Metrics]) -> Option<Summary> {
    if runs.is_empty() {
        return None;
    }
    let n = runs.len() as f64;
    let mean = |f: fn(&Metrics) -> f64| runs.iter().map(f).sum::<f64>() / n;
    let fps = runs.iter().map(|m| m.avg_fps);
    let spread = fps.clone().fold(f64::MIN, f64::max) - fps.fold(f64::MAX, f64::min);
    Some(Summary {
        runs: runs.len(),
        avg_fps: mean(|m| m.avg_fps),
        low1_fps: mean(|m| m.low1_fps),
        avg_frametime_ms: mean(|m| m.avg_frametime_ms),
        fps_spread: spread,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Comparison {
    pub before: Summary,
    pub after: Summary,
    /// Percentage change of the average FPS (positive = faster).
    pub fps_change_pct: f64,
    pub low1_change_pct: f64,
    /// … and bigger than the run-to-run variation (owner decision, SPEC 4.4): then the
    /// app proposes undoing the changes.
    pub significant_drop: bool,
}

pub fn compare(before: &Summary, after: &Summary) -> Comparison {
    let pct = |b: f64, a: f64| if b > 0.0 { (a - b) / b * 100.0 } else { 0.0 };
    let drop = before.avg_fps - after.avg_fps;
    let noise = before.fps_spread.max(after.fps_spread);
    Comparison {
        before: *before,
        after: *after,
        fps_change_pct: pct(before.avg_fps, after.avg_fps),
        low1_change_pct: pct(before.low1_fps, after.low1_fps),
        significant_drop: drop > before.avg_fps * DROP_THRESHOLD && drop > noise,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steady(ms: f64, n: usize) -> Vec<f64> {
        vec![ms; n]
    }

    #[test]
    fn steady_frames() {
        let m = metrics(&steady(10.0, 1000)).unwrap();
        assert_eq!(m.frames, 1000);
        assert!((m.avg_fps - 100.0).abs() < 1e-9);
        assert!((m.low1_fps - 100.0).abs() < 1e-9);
        assert!((m.avg_frametime_ms - 10.0).abs() < 1e-9);
        assert!((m.seconds - 10.0).abs() < 1e-9);
    }

    #[test]
    fn stutters_pull_down_the_1_percent_low() {
        // 990 frames at 5 ms and 10 stutters of 50 ms: 1% of 1000 frames.
        let mut times = steady(5.0, 990);
        times.extend(steady(50.0, 10));
        let m = metrics(&times).unwrap();
        assert!((m.low1_fps - 20.0).abs() < 1e-9, "{m:?}");
        assert!(m.avg_fps > 180.0 && m.avg_fps < 200.0, "{m:?}");
        assert!(m.low1_fps <= m.avg_fps);
    }

    #[test]
    fn too_few_frames() {
        assert_eq!(
            metrics(&steady(10.0, 5)),
            Err(BenchError::TooFewFrames {
                frames: 5,
                min: MIN_FRAMES
            })
        );
    }

    fn run(fps: f64) -> Metrics {
        metrics(&steady(1000.0 / fps, 600)).unwrap()
    }

    #[test]
    fn verdicts() {
        let before = summarize(&[run(140.0), run(142.0), run(144.0)]).unwrap();
        assert!((before.avg_fps - 142.0).abs() < 1e-6);
        assert!((before.fps_spread - 4.0).abs() < 1e-6);

        // Faster.
        let up = compare(
            &before,
            &summarize(&[run(150.0), run(151.0), run(152.0)]).unwrap(),
        );
        assert!(up.fps_change_pct > 5.0 && !up.significant_drop);

        // A real drop: −7%, far beyond the 4 FPS spread.
        let down = compare(
            &before,
            &summarize(&[run(132.0), run(132.0), run(133.0)]).unwrap(),
        );
        assert!(down.significant_drop, "{down:?}");

        // −2%: below the 3% threshold.
        let small = compare(
            &before,
            &summarize(&[run(139.0), run(139.0), run(139.5)]).unwrap(),
        );
        assert!(!small.significant_drop, "{small:?}");

        // −5%, but the runs vary by 12 FPS: noise.
        let noisy_before = summarize(&[run(130.0), run(142.0), run(154.0)]).unwrap();
        let noisy = compare(
            &noisy_before,
            &summarize(&[run(135.0), run(135.0), run(135.0)]).unwrap(),
        );
        assert!(!noisy.significant_drop, "{noisy:?}");
        assert!(summarize(&[]).is_none());
    }
}
