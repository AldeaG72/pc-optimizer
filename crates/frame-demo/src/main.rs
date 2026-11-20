//! `pco-frame-demo [seconds]`: a small window that clears and presents a Direct3D 11 frame
//! about 100 times a second, so PresentMon (and `pco bench`) can be tested end to end without
//! a game. Uses the GPU if there is one, otherwise the WARP software renderer (CI runners).

#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    let seconds = std::env::args()
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60);
    demo::run(seconds)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("pco-frame-demo runs on Windows only");
}

#[cfg(windows)]
mod demo {
    use std::time::{Duration, Instant};

    use windows::core::{w, Result};
    use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDeviceAndSwapChain, ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView,
        ID3D11Texture2D, D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION,
    };
    use windows::Win32::Graphics::Dxgi::Common::{
        DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC, DXGI_SAMPLE_DESC,
    };
    use windows::Win32::Graphics::Dxgi::{
        IDXGISwapChain, DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_FLIP_DISCARD,
        DXGI_USAGE_RENDER_TARGET_OUTPUT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, PeekMessageW, RegisterClassW,
        TranslateMessage, CW_USEDEFAULT, MSG, PM_REMOVE, WINDOW_EX_STYLE, WNDCLASSW,
        WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };

    const WIDTH: u32 = 640;
    const HEIGHT: u32 = 360;

    extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // SAFETY: default handling for every message.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    pub fn run(seconds: u64) -> Result<()> {
        // SAFETY: standard Win32 window and Direct3D 11 setup; every pointer passed is valid
        // for the duration of its call.
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = w!("PcoFrameDemo");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("pco frame demo"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                WIDTH as i32,
                HEIGHT as i32,
                None,
                None,
                Some(instance.into()),
                None,
            )?;

            let desc = DXGI_SWAP_CHAIN_DESC {
                BufferDesc: DXGI_MODE_DESC {
                    Width: WIDTH,
                    Height: HEIGHT,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    ..Default::default()
                },
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                OutputWindow: hwnd,
                Windowed: true.into(),
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                Flags: 0,
            };

            let mut created = None;
            for driver in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
                let mut swap: Option<IDXGISwapChain> = None;
                let mut device: Option<ID3D11Device> = None;
                let mut context: Option<ID3D11DeviceContext> = None;
                let ok = D3D11CreateDeviceAndSwapChain(
                    None,
                    driver,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_FLAG(0),
                    None,
                    D3D11_SDK_VERSION,
                    Some(&desc),
                    Some(&mut swap),
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )
                .is_ok();
                if let (true, Some(s), Some(d), Some(c)) = (ok, swap, device, context) {
                    created = Some((s, d, c));
                    break;
                }
            }
            let Some((swap, device, context)) = created else {
                return Err(windows::core::Error::from_thread());
            };

            let back: ID3D11Texture2D = swap.GetBuffer(0)?;
            let mut rtv: Option<ID3D11RenderTargetView> = None;
            device.CreateRenderTargetView(&back, None, Some(&mut rtv))?;
            let rtv = rtv.ok_or_else(windows::core::Error::from_thread)?;

            let start = Instant::now();
            let mut frame: u64 = 0;
            while start.elapsed() < Duration::from_secs(seconds) {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let shade = (frame % 256) as f32 / 255.0;
                context.ClearRenderTargetView(&rtv, &[shade, 0.25, 0.5, 1.0]);
                swap.Present(0, DXGI_PRESENT(0)).ok()?;
                frame += 1;
                // About 100 frames per second, steady.
                std::thread::sleep(Duration::from_millis(10));
            }
            println!("pco-frame-demo: {frame} frames in {seconds} s");
        }
        Ok(())
    }
}
