//! Windows integration.

use std::{error::Error, path::PathBuf, sync::LazyLock};

/// The AppUserModelID of the installer's Start menu shortcut (`crates/xtask`).
const APP_ID: &str = "org.mtrx.Mtrx";

/// Show `notification` as coming from Mtrx, with its name and icon.
/// Windows shows toasts only for an app ID that a Start menu shortcut
/// registered, so without the installer's shortcut (portable zip) the
/// notification library keeps its default sender (PowerShell).
pub fn notification_sender(notification: &mut notify_rust::Notification) {
    static INSTALLED: LazyLock<bool> = LazyLock::new(|| shortcut().is_some_and(|p| p.exists()));
    if *INSTALLED {
        notification.app_id(APP_ID);
    }
}

/// The installer's per-user Start menu shortcut.
fn shortcut() -> Option<PathBuf> {
    let roaming = PathBuf::from(std::env::var_os("APPDATA")?);
    Some(roaming.join(r"Microsoft\Windows\Start Menu\Programs\Mtrx.lnk"))
}

/// Show `count` as an overlay on the taskbar button of `window`, as mail
/// clients do; none for 0. Called on the main thread, which owns the
/// window.
pub fn taskbar_badge(window: &dyn iced::window::Window, count: u64) {
    use ::windows::Win32::{
        Foundation::HWND,
        System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
    };
    use iced::window::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let hwnd = match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => HWND(handle.hwnd.get() as *mut std::ffi::c_void),
        other => {
            tracing::warn!("no taskbar button for the unread count: {other:?}");
            return;
        }
    };
    // COM is usually set up on this thread already (winit's drag and drop);
    // this only makes sure, and then balances its count.
    // SAFETY: no arguments to get wrong.
    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    if let Err(e) = overlay(hwnd, count) {
        tracing::warn!("no unread count on the taskbar button: {e}");
    }
    if com {
        // SAFETY: matches the successful initialisation above.
        unsafe { CoUninitialize() };
    }
}

/// Set (or for 0 clear) the overlay icon on `hwnd`'s taskbar button.
fn overlay(hwnd: ::windows::Win32::Foundation::HWND, count: u64) -> Result<(), Box<dyn Error>> {
    use ::windows::{
        Win32::{
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            UI::{
                HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
                Shell::{ITaskbarList3, TaskbarList},
                WindowsAndMessaging::{HICON, SM_CXSMICON},
            },
        },
        core::{HSTRING, PCWSTR},
    };

    // SAFETY: COM is initialised on this thread and `hwnd` is the live
    // window the runtime handed over.
    unsafe {
        let taskbar: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)?;
        taskbar.HrInit()?;
        if count == 0 {
            taskbar.SetOverlayIcon(hwnd, HICON::default(), PCWSTR::null())?;
            return Ok(());
        }
        // A small icon at the window's DPI: 16 pixels at 100 %.
        let size = GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForWindow(hwnd)).max(16);
        let rgba = crate::badge::image(count, size as u32).ok_or("badge not drawn")?;
        let icon = icon(&rgba, size)?;
        // The taskbar keeps a copy; `icon` is destroyed on return.
        let description = HSTRING::from(format!("{count} unread"));
        taskbar.SetOverlayIcon(hwnd, *icon, &description)?;
    }
    Ok(())
}

/// An icon from straight RGBA pixels, `size` square, destroyed on drop.
fn icon(
    rgba: &[u8],
    size: i32,
) -> ::windows::core::Result<::windows::core::Owned<::windows::Win32::UI::WindowsAndMessaging::HICON>>
{
    use ::windows::{Win32::UI::WindowsAndMessaging::CreateIcon, core::Owned};

    // The colour bitmap is BGRA; its alpha channel overrides the 1-bit
    // mask, so that stays empty (rows padded to 16 bits).
    let bgra: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect();
    let side = size as usize;
    let mask = vec![0u8; side.div_ceil(16) * 2 * side];
    // SAFETY: both buffers hold `size` rows at the stated bit depths, and
    // the icon is owned (destroyed) by the result alone.
    unsafe {
        let icon = CreateIcon(None, size, size, 1, 32, mask.as_ptr(), bgra.as_ptr())?;
        Ok(Owned::new(icon))
    }
}
