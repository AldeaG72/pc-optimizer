//! Windows edition and management, CPU, memory modules and GPUs.

use std::ffi::c_void;

use serde::Deserialize;
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
use windows::Win32::Graphics::Direct3D12::{
    D3D12CreateDevice, ID3D12Device, D3D12_FEATURE_ARCHITECTURE, D3D12_FEATURE_DATA_ARCHITECTURE,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
};
use wmi::WMIConnection;

use super::{num, reg_string, subkeys, wide, Num};
use crate::backend::windows::WindowsRegistry;
use crate::guard::edition_from_id;
use crate::hw::{Cpu, Gpu, GpuVendor, HwInfo, MemoryModule};
use crate::recipe::Hive;

const VERSION_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

#[derive(Deserialize)]
#[serde(rename = "Win32_ComputerSystem", rename_all = "PascalCase")]
struct ComputerSystem {
    part_of_domain: Option<bool>,
}

pub(super) fn windows(
    reg: &WindowsRegistry,
    wmi: Option<&WMIConnection>,
    info: &mut HwInfo,
    errors: &mut Vec<String>,
) {
    let w = &mut info.windows;
    w.build = reg_string(reg, Hive::LocalMachine, VERSION_KEY, "CurrentBuildNumber")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or_else(|| {
            errors.push("windows: build number not readable".into());
            0
        });
    w.edition = reg_string(reg, Hive::LocalMachine, VERSION_KEY, "EditionID")
        .as_deref()
        .and_then(edition_from_id);
    w.display_version = reg_string(reg, Hive::LocalMachine, VERSION_KEY, "DisplayVersion");

    if let Some(wmi) = wmi {
        match wmi.query::<ComputerSystem>() {
            Ok(rows) => w.domain_joined = rows.first().and_then(|r| r.part_of_domain),
            Err(e) => errors.push(format!("domain: {e}")),
        }
    }

    // Enrolled in Intune (or another MDM): an enrollment with this provider id.
    match subkeys(Hive::LocalMachine, r"SOFTWARE\Microsoft\Enrollments") {
        Ok(ids) => {
            w.mdm_enrolled = Some(ids.iter().any(|id| {
                reg_string(
                    reg,
                    Hive::LocalMachine,
                    &format!(r"SOFTWARE\Microsoft\Enrollments\{id}"),
                    "ProviderID",
                )
                .is_some_and(|p| p == "MS DM Server")
            }));
        }
        Err(e) => errors.push(format!("mdm: {e}")),
    }
}

#[derive(Deserialize)]
#[serde(rename = "Win32_Processor", rename_all = "PascalCase")]
struct Processor {
    name: Option<String>,
    number_of_cores: Option<u32>,
    number_of_logical_processors: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename = "Win32_PhysicalMemory", rename_all = "PascalCase")]
struct PhysicalMemory {
    capacity: Option<Num>,
    speed: Option<u32>,
    configured_clock_speed: Option<u32>,
    device_locator: Option<String>,
    bank_label: Option<String>,
}

pub(super) fn cpu_and_memory(
    wmi: Option<&WMIConnection>,
    info: &mut HwInfo,
    errors: &mut Vec<String>,
) {
    let Some(wmi) = wmi else { return };
    match wmi.query::<Processor>() {
        Ok(rows) => {
            // Multi-socket PCs: sum cores and threads, keep the first name.
            let name = rows
                .iter()
                .find_map(|r| r.name.as_deref().map(|n| n.trim().to_owned()));
            if let Some(name) = name {
                info.cpu = Some(Cpu {
                    name,
                    cores: rows.iter().filter_map(|r| r.number_of_cores).sum(),
                    threads: rows
                        .iter()
                        .filter_map(|r| r.number_of_logical_processors)
                        .sum(),
                });
            }
        }
        Err(e) => errors.push(format!("cpu: {e}")),
    }
    match wmi.query::<PhysicalMemory>() {
        Ok(rows) => {
            info.memory = rows
                .into_iter()
                .map(|m| MemoryModule {
                    capacity_mb: num(&m.capacity).unwrap_or(0) / (1024 * 1024),
                    rated_mhz: m.speed.filter(|s| *s > 0),
                    configured_mhz: m.configured_clock_speed.filter(|s| *s > 0),
                    slot: m.device_locator.map(|s| s.trim().to_owned()),
                    bank: m.bank_label.map(|s| s.trim().to_owned()),
                })
                .collect();
        }
        Err(e) => errors.push(format!("memory: {e}")),
    }
}

#[derive(Deserialize)]
#[serde(rename = "Win32_VideoController", rename_all = "PascalCase")]
struct VideoController {
    name: Option<String>,
    #[serde(rename = "PNPDeviceID")]
    pnp_device_id: Option<String>,
    driver_version: Option<String>,
    driver_date: Option<String>,
}

/// `VEN_10DE&DEV_2684` from a PNP device id.
fn pci_ids(pnp: &str) -> Option<(u16, u16)> {
    let upper = pnp.to_ascii_uppercase();
    let hex = |tag: &str| {
        let pos = upper.find(tag)? + tag.len();
        u16::from_str_radix(upper.get(pos..pos + 4)?, 16).ok()
    };
    Some((hex("VEN_")?, hex("DEV_")?))
}

/// `20240115000000.000000-000` → `2024-01-15`.
fn cim_date(s: &str) -> Option<String> {
    let d = s.get(0..8)?;
    if !d.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8]))
}

pub(super) fn gpus(wmi: Option<&WMIConnection>, info: &mut HwInfo, errors: &mut Vec<String>) {
    let controllers: Vec<VideoController> = match wmi.map(|w| w.query::<VideoController>()) {
        Some(Ok(rows)) => rows,
        Some(Err(e)) => {
            errors.push(format!("gpu drivers: {e}"));
            Vec::new()
        }
        None => Vec::new(),
    };
    let driver_for = |vendor: u16, device: u16| {
        controllers.iter().find(|c| {
            c.pnp_device_id
                .as_deref()
                .and_then(pci_ids)
                .is_some_and(|ids| ids == (vendor, device))
        })
    };

    match dxgi_adapters() {
        Ok(adapters) => {
            info.gpus = adapters
                .into_iter()
                .map(|a| {
                    let driver = driver_for(a.vendor_id, a.device_id);
                    Gpu {
                        name: a.name,
                        vendor: GpuVendor::from_pci_id(a.vendor_id),
                        vendor_id: a.vendor_id,
                        device_id: a.device_id,
                        dedicated_mb: Some(a.dedicated_mb),
                        integrated: a.integrated,
                        driver_version: driver.and_then(|d| d.driver_version.clone()),
                        driver_date: driver
                            .and_then(|d| d.driver_date.as_deref())
                            .and_then(cim_date),
                    }
                })
                .collect();
        }
        Err(e) => {
            errors.push(format!("gpus: {e}"));
            // Fall back to the WMI list.
            info.gpus = controllers
                .iter()
                .filter_map(|c| {
                    let (vendor_id, device_id) = c.pnp_device_id.as_deref().and_then(pci_ids)?;
                    Some(Gpu {
                        name: c.name.clone().unwrap_or_default(),
                        vendor: GpuVendor::from_pci_id(vendor_id),
                        vendor_id,
                        device_id,
                        dedicated_mb: None,
                        integrated: None,
                        driver_version: c.driver_version.clone(),
                        driver_date: c.driver_date.as_deref().and_then(cim_date),
                    })
                })
                .collect();
        }
    }
}

struct Adapter {
    name: String,
    vendor_id: u16,
    device_id: u16,
    dedicated_mb: u64,
    integrated: Option<bool>,
}

/// Hardware adapters from DXGI (software renderers skipped).
fn dxgi_adapters() -> Result<Vec<Adapter>, String> {
    // SAFETY: plain factory creation.
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(|e| e.message().to_string())?;
    let mut out = Vec::new();
    for i in 0.. {
        // SAFETY: enumeration stops at the first error (DXGI_ERROR_NOT_FOUND).
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else {
            break;
        };
        // SAFETY: `adapter` is valid.
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        if desc.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 || desc.VendorId == 0x1414 {
            continue; // Microsoft Basic Render Driver
        }
        out.push(Adapter {
            name: wide(&desc.Description).trim().to_owned(),
            vendor_id: desc.VendorId as u16,
            device_id: desc.DeviceId as u16,
            dedicated_mb: desc.DedicatedVideoMemory as u64 / (1024 * 1024),
            integrated: uses_system_memory(&adapter),
        });
    }
    Ok(out)
}

/// Direct3D 12 "UMA" = the GPU shares system memory, i.e. integrated graphics.
fn uses_system_memory(adapter: &IDXGIAdapter1) -> Option<bool> {
    let mut device: Option<ID3D12Device> = None;
    // SAFETY: valid adapter; `device` is a valid out-parameter.
    unsafe { D3D12CreateDevice(adapter, D3D_FEATURE_LEVEL_11_0, &mut device) }.ok()?;
    let device = device?;
    let mut arch = D3D12_FEATURE_DATA_ARCHITECTURE::default();
    // SAFETY: `arch` matches the feature and its size is passed.
    unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_ARCHITECTURE,
            (&raw mut arch).cast::<c_void>(),
            std::mem::size_of::<D3D12_FEATURE_DATA_ARCHITECTURE>() as u32,
        )
    }
    .ok()?;
    Some(arch.UMA.as_bool())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pci_ids_and_dates() {
        assert_eq!(
            pci_ids(r"PCI\VEN_10DE&DEV_2684&SUBSYS_16F110DE&REV_A1\4&1"),
            Some((0x10DE, 0x2684))
        );
        assert_eq!(pci_ids("ROOT\\BasicDisplay"), None);
        assert_eq!(
            cim_date("20240115000000.000000-000").as_deref(),
            Some("2024-01-15")
        );
        assert_eq!(cim_date("x"), None);
    }
}
