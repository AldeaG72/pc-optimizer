//! Physical disks (Windows Storage WMI), volumes and TRIM.

use serde::Deserialize;
use wmi::WMIConnection;

use super::{num, reg_dword, Num};
use crate::backend::windows::WindowsRegistry;
use crate::hw::{Disk, DiskMedia, HwInfo, Volume};
use crate::recipe::Hive;

#[derive(Deserialize)]
#[serde(rename = "MSFT_PhysicalDisk", rename_all = "PascalCase")]
struct PhysicalDisk {
    device_id: Option<String>,
    friendly_name: Option<String>,
    media_type: Option<u16>,
    bus_type: Option<u16>,
    size: Option<Num>,
    health_status: Option<u16>,
}

#[derive(Deserialize)]
#[serde(rename = "MSFT_Partition", rename_all = "PascalCase")]
struct Partition {
    disk_number: Option<u32>,
    drive_letter: Option<Num>,
}

#[derive(Deserialize)]
#[serde(rename = "Win32_LogicalDisk", rename_all = "PascalCase")]
struct LogicalDisk {
    #[serde(rename = "DeviceID")]
    device_id: Option<String>,
    drive_type: Option<u32>,
    size: Option<Num>,
    free_space: Option<Num>,
}

fn bus_name(code: u16) -> Option<&'static str> {
    Some(match code {
        1 => "SCSI",
        3 => "ATA",
        7 => "USB",
        8 => "RAID",
        10 => "SAS",
        11 => "SATA",
        12 => "SD",
        13 => "MMC",
        17 => "NVMe",
        _ => return None,
    })
}

pub(super) fn storage(
    reg: &WindowsRegistry,
    wmi: Option<&WMIConnection>,
    info: &mut HwInfo,
    errors: &mut Vec<String>,
) {
    // TRIM: DisableDeleteNotification = 0 (or absent) means TRIM is on.
    info.trim_enabled = Some(
        reg_dword(
            reg,
            Hive::LocalMachine,
            r"SYSTEM\CurrentControlSet\Control\FileSystem",
            "DisableDeleteNotification",
        )
        .unwrap_or(0)
            == 0,
    );

    let Some(cimv2) = wmi else { return };
    let system_letter = std::env::var("SystemDrive")
        .ok()
        .and_then(|s| s.chars().next())
        .map(|c| c.to_ascii_uppercase());

    let mut letter_to_disk = Vec::new();
    match WMIConnection::with_namespace_path(r"root\Microsoft\Windows\Storage") {
        Ok(storage_ns) => {
            match storage_ns.query::<PhysicalDisk>() {
                Ok(rows) => {
                    info.disks = rows
                        .into_iter()
                        .filter_map(|d| {
                            let number = d.device_id.as_deref()?.trim().parse().ok()?;
                            let bus = d.bus_type.and_then(bus_name);
                            let media = match d.media_type {
                                Some(3) => DiskMedia::Hdd,
                                Some(4 | 5) => DiskMedia::Ssd,
                                // NVMe without a media type is flash.
                                _ if bus == Some("NVMe") => DiskMedia::Ssd,
                                _ => DiskMedia::Unknown,
                            };
                            Some(Disk {
                                number,
                                model: d.friendly_name.unwrap_or_default().trim().to_owned(),
                                media,
                                bus: bus.map(str::to_owned),
                                size_bytes: num(&d.size).unwrap_or(0),
                                healthy: match d.health_status {
                                    Some(0) => Some(true),
                                    Some(1 | 2) => Some(false),
                                    _ => None,
                                },
                            })
                        })
                        .collect();
                    info.disks.sort_by_key(|d| d.number);
                }
                Err(e) => errors.push(format!("disks: {e}")),
            }
            match storage_ns.query::<Partition>() {
                Ok(rows) => {
                    for p in rows {
                        // A `char16` property: the WMI crate gives it as text ("C", or "\0"
                        // when the partition has no letter); accept a number too.
                        let letter = match &p.drive_letter {
                            Some(Num::S(s)) => s.chars().next(),
                            other => {
                                num(other).and_then(|c| char::from_u32(u32::try_from(c).ok()?))
                            }
                        }
                        .filter(char::is_ascii_alphabetic);
                        if let (Some(letter), Some(disk)) = (letter, p.disk_number) {
                            letter_to_disk.push((letter.to_ascii_uppercase(), disk));
                        }
                    }
                }
                Err(e) => errors.push(format!("partitions: {e}")),
            }
        }
        Err(e) => errors.push(format!("storage wmi: {e}")),
    }

    match cimv2.query::<LogicalDisk>() {
        Ok(rows) => {
            info.volumes = rows
                .into_iter()
                .filter(|v| v.drive_type == Some(3)) // local disks
                .filter_map(|v| {
                    let letter = v.device_id.as_deref()?.chars().next()?.to_ascii_uppercase();
                    Some(Volume {
                        letter,
                        size_bytes: num(&v.size).unwrap_or(0),
                        free_bytes: num(&v.free_space).unwrap_or(0),
                        disk: letter_to_disk
                            .iter()
                            .find(|(l, _)| *l == letter)
                            .map(|(_, d)| *d),
                        system: system_letter == Some(letter),
                    })
                })
                .collect();
        }
        Err(e) => errors.push(format!("volumes: {e}")),
    }
}
