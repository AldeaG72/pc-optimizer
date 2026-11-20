//! Power plan, battery, desktop/laptop, and virtualization-based security.

use serde::Deserialize;
use windows::core::GUID;
use windows::Win32::Foundation::{LocalFree, ERROR_SUCCESS, HLOCAL};
use windows::Win32::System::Power::{
    GetSystemPowerStatus, PowerGetActiveScheme, PowerReadFriendlyName, SYSTEM_POWER_STATUS,
};
use wmi::WMIConnection;

use crate::hw::{HwInfo, PowerPlan};
use crate::recipe::FormFactor;

#[derive(Deserialize)]
#[serde(rename = "Win32_SystemEnclosure", rename_all = "PascalCase")]
struct Enclosure {
    chassis_types: Option<Vec<u16>>,
}

/// SMBIOS chassis types (DMTF DSP0134).
fn chassis_form_factor(types: &[u16]) -> Option<FormFactor> {
    const LAPTOP: &[u16] = &[8, 9, 10, 11, 12, 14, 18, 21, 30, 31, 32];
    const DESKTOP: &[u16] = &[3, 4, 5, 6, 7, 13, 15, 16, 24, 34, 35, 36];
    if types.iter().any(|t| LAPTOP.contains(t)) {
        Some(FormFactor::Laptop)
    } else if types.iter().any(|t| DESKTOP.contains(t)) {
        Some(FormFactor::Desktop)
    } else {
        None
    }
}

pub(super) fn power(wmi: Option<&WMIConnection>, info: &mut HwInfo, errors: &mut Vec<String>) {
    match active_plan() {
        Ok((guid, name)) => {
            info.power.plan = Some(PowerPlan::from_guid(&guid));
            info.power.plan_name = name;
        }
        Err(e) => errors.push(format!("power plan: {e}")),
    }

    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: valid out-pointer.
    match unsafe { GetSystemPowerStatus(&mut status) } {
        Ok(()) => {
            // 128 = no system battery, 255 = unknown.
            info.power.has_battery = match status.BatteryFlag {
                255 => None,
                f => Some(f & 128 == 0),
            };
            info.power.on_battery = match status.ACLineStatus {
                0 => Some(true),
                1 => Some(false),
                _ => None,
            };
        }
        Err(e) => errors.push(format!("battery: {}", e.message())),
    }

    let chassis = match wmi.map(|w| w.query::<Enclosure>()) {
        Some(Ok(rows)) => rows
            .iter()
            .filter_map(|r| r.chassis_types.as_deref())
            .find_map(chassis_form_factor),
        Some(Err(e)) => {
            errors.push(format!("chassis: {e}"));
            None
        }
        None => None,
    };
    // The chassis type decides; with an unknown chassis, a battery means laptop.
    info.power.form_factor = chassis.or(match info.power.has_battery {
        Some(true) => Some(FormFactor::Laptop),
        _ => None,
    });
}

fn active_plan() -> Result<(String, Option<String>), String> {
    let mut guid: *mut GUID = std::ptr::null_mut();
    // SAFETY: Windows allocates the GUID; freed below with LocalFree.
    let rc = unsafe { PowerGetActiveScheme(None, &mut guid) };
    if rc != ERROR_SUCCESS || guid.is_null() {
        return Err(format!("PowerGetActiveScheme failed ({})", rc.0));
    }
    // SAFETY: non-null, set by PowerGetActiveScheme.
    let scheme = unsafe { *guid };

    let mut size = 0u32;
    // SAFETY: first call asks for the size only.
    let _ = unsafe { PowerReadFriendlyName(None, Some(&scheme), None, None, None, &mut size) };
    let name = if size > 0 {
        let mut buf = vec![0u8; size as usize];
        // SAFETY: `buf` has `size` bytes.
        let rc = unsafe {
            PowerReadFriendlyName(
                None,
                Some(&scheme),
                None,
                None,
                Some(buf.as_mut_ptr()),
                &mut size,
            )
        };
        (rc == ERROR_SUCCESS).then(|| {
            let units: Vec<u16> = buf
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            super::wide(&units)
        })
    } else {
        None
    };

    // SAFETY: allocated by PowerGetActiveScheme with LocalAlloc.
    unsafe { LocalFree(Some(HLOCAL(guid.cast()))) };
    Ok((format!("{scheme:?}"), name.filter(|n| !n.trim().is_empty())))
}

#[derive(Deserialize)]
#[serde(rename = "Win32_DeviceGuard", rename_all = "PascalCase")]
struct DeviceGuard {
    virtualization_based_security_status: Option<u32>,
    security_services_running: Option<Vec<u32>>,
}

pub(super) fn security(info: &mut HwInfo, errors: &mut Vec<String>) {
    let rows = WMIConnection::with_namespace_path(r"root\Microsoft\Windows\DeviceGuard")
        .and_then(|c| c.query::<DeviceGuard>());
    match rows {
        Ok(rows) => {
            if let Some(dg) = rows.first() {
                // 2 = running.
                info.security.vbs_running = dg.virtualization_based_security_status.map(|s| s == 2);
                // 2 = hypervisor-enforced code integrity (Memory Integrity).
                info.security.hvci_running = dg
                    .security_services_running
                    .as_ref()
                    .map(|s| s.contains(&2));
            }
        }
        Err(e) => errors.push(format!("device guard: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chassis_types() {
        assert_eq!(chassis_form_factor(&[10]), Some(FormFactor::Laptop));
        assert_eq!(chassis_form_factor(&[3]), Some(FormFactor::Desktop));
        assert_eq!(chassis_form_factor(&[31]), Some(FormFactor::Laptop)); // convertible
        assert_eq!(chassis_form_factor(&[1]), None); // "Other" (e.g. VMs)
        assert_eq!(chassis_form_factor(&[]), None);
    }
}
