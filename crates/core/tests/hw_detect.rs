//! Real hardware detection on Windows. Read-only, so it runs everywhere: the development PC
//! and CI. Values differ per machine; the test checks they are plausible.

#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pco_core::hw::windows::detect;

#[test]
fn detects_plausible_values() {
    let info = detect();
    println!("{}", serde_json::to_string_pretty(&info).unwrap());

    assert!(info.windows.build >= 10_000, "build {}", info.windows.build);
    let cpu = info.cpu.as_ref().expect("CPU detected");
    assert!(!cpu.name.is_empty());
    assert!(cpu.cores >= 1 && cpu.threads >= cpu.cores);
    assert!(
        info.memory.iter().map(|m| m.capacity_mb).sum::<u64>() >= 1024 || info.memory.is_empty()
    );
    assert!(
        info.volumes.iter().any(|v| v.system),
        "the Windows drive is among the volumes: {:?}",
        info.volumes
    );
    for v in &info.volumes {
        assert!(v.free_bytes <= v.size_bytes, "{v:?}");
    }
    assert!(info.trim_enabled.is_some());
    assert!(
        info.power.plan.is_some(),
        "power plan: {:?}",
        info.probe_errors
    );
    for d in &info.displays {
        assert!(d.width > 0 && d.height > 0, "{d:?}");
        if let Some(max) = d.max_refresh_hz {
            assert!(max >= d.refresh_hz, "{d:?}");
        }
    }
    for g in &info.games {
        assert!(!g.exe.is_empty(), "{g:?}");
    }
}
