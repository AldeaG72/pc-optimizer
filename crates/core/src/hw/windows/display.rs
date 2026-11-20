//! Monitors (Display Configuration API + GDI mode lists) and the DirectX graphics settings.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_PREFERRED_MODE,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_DEVICE_INFO_TYPE,
    DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_PREFERRED_MODE, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, LUID};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW,
    DISPLAY_DEVICE_PRIMARY_DEVICE, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE,
};

use super::{reg_string, wide};
use crate::backend::windows::WindowsRegistry;
use crate::hw::{DirectXSettings, Display, HwInfo};
use crate::recipe::Hive;

const DIRECTX_KEY: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";

pub(super) fn displays(reg: &WindowsRegistry, info: &mut HwInfo, errors: &mut Vec<String>) {
    match active_displays() {
        Ok(list) => info.displays = list,
        Err(e) => errors.push(format!("displays: {e}")),
    }
    if let Some(data) = reg_string(
        reg,
        Hive::CurrentUser,
        DIRECTX_KEY,
        "DirectXUserGlobalSettings",
    ) {
        info.directx = DirectXSettings::parse(&data);
    }
}

/// Fills a request header and asks for one piece of device information.
fn device_info<T>(
    packet: &mut T,
    kind: DISPLAYCONFIG_DEVICE_INFO_TYPE,
    adapter: LUID,
    id: u32,
) -> bool {
    // SAFETY: every DISPLAYCONFIG_* request starts with the header.
    let header = unsafe { &mut *(packet as *mut T).cast::<DISPLAYCONFIG_DEVICE_INFO_HEADER>() };
    header.r#type = kind;
    header.size = std::mem::size_of::<T>() as u32;
    header.adapterId = adapter;
    header.id = id;
    // SAFETY: the packet is the full struct for `kind`, with its size in the header.
    unsafe { DisplayConfigGetDeviceInfo(header) == 0 }
}

fn active_displays() -> Result<Vec<Display>, String> {
    let mut path_count = 0u32;
    let mut mode_count = 0u32;
    // SAFETY: valid out-pointers.
    let rc = unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
    };
    if rc != ERROR_SUCCESS {
        return Err(format!("GetDisplayConfigBufferSizes failed ({})", rc.0));
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
    // SAFETY: the buffers hold the counts passed.
    let rc = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(format!("QueryDisplayConfig failed ({})", rc.0));
    }
    paths.truncate(path_count as usize);

    let primary = primary_gdi_name();
    let mut out = Vec::new();
    for path in &paths {
        let src = &path.sourceInfo;
        let tgt = &path.targetInfo;

        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        if !device_info(
            &mut source,
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            src.adapterId,
            src.id,
        ) {
            continue;
        }
        let gdi = wide(&source.viewGdiDeviceName);

        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
        let name = if device_info(
            &mut target,
            DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            tgt.adapterId,
            tgt.id,
        ) {
            let n = wide(&target.monitorFriendlyDeviceName);
            if !n.trim().is_empty() {
                n.trim().to_owned()
            } else if target.outputTechnology == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL {
                "Built-in display".to_owned()
            } else {
                gdi.trim_start_matches(r"\\.\").to_owned()
            }
        } else {
            gdi.trim_start_matches(r"\\.\").to_owned()
        };

        let mut preferred = DISPLAYCONFIG_TARGET_PREFERRED_MODE::default();
        let native = device_info(
            &mut preferred,
            DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_PREFERRED_MODE,
            tgt.adapterId,
            tgt.id,
        )
        .then_some((preferred.width, preferred.height))
        .filter(|(w, h)| *w > 0 && *h > 0);

        let mut color = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO::default();
        let hdr = device_info(
            &mut color,
            DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
            tgt.adapterId,
            tgt.id,
        )
        .then(|| {
            // SAFETY: plain bitfield read.
            let bits = unsafe { color.Anonymous.value };
            let enabled = bits & 0b10 != 0;
            // "Wide color enforced" is SDR wide color (auto color management), not HDR.
            let wide_color_only = bits & 0b100 != 0;
            enabled && !wide_color_only
        });

        let Some((width, height, refresh)) = current_mode(&gdi) else {
            continue;
        };
        out.push(Display {
            name,
            primary: primary.as_deref() == Some(gdi.as_str()),
            width,
            height,
            refresh_hz: refresh,
            max_refresh_hz: max_refresh(&gdi, width, height),
            native_width: native.map(|n| n.0),
            native_height: native.map(|n| n.1),
            hdr_enabled: hdr,
        });
    }
    Ok(out)
}

fn devmode() -> DEVMODEW {
    DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    }
}

fn current_mode(gdi: &str) -> Option<(u32, u32, u32)> {
    let name = HSTRING::from(gdi);
    let mut dm = devmode();
    // SAFETY: `dm.dmSize` is set; `name` outlives the call.
    unsafe { EnumDisplaySettingsW(&name, ENUM_CURRENT_SETTINGS, &mut dm) }
        .as_bool()
        .then_some((dm.dmPelsWidth, dm.dmPelsHeight, dm.dmDisplayFrequency))
}

/// Highest refresh rate the display offers at this resolution.
fn max_refresh(gdi: &str, width: u32, height: u32) -> Option<u32> {
    let name = HSTRING::from(gdi);
    let mut best = None;
    for i in 0..10_000u32 {
        let mut dm = devmode();
        // SAFETY: as above; enumeration ends when it returns false.
        if !unsafe { EnumDisplaySettingsW(&name, ENUM_DISPLAY_SETTINGS_MODE(i), &mut dm) }.as_bool()
        {
            break;
        }
        if dm.dmPelsWidth == width && dm.dmPelsHeight == height {
            best = best.max(Some(dm.dmDisplayFrequency));
        }
    }
    best
}

fn primary_gdi_name() -> Option<String> {
    for i in 0..64u32 {
        let mut dd = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: `dd.cb` is set.
        if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), i, &mut dd, 0) }.as_bool() {
            break;
        }
        if dd.StateFlags.0 & DISPLAY_DEVICE_PRIMARY_DEVICE.0 != 0 {
            return Some(wide(&dd.DeviceName));
        }
    }
    None
}
