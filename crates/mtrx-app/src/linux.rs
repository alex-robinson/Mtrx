//! Linux desktop integration.

use iced::window::settings::PlatformSpecific;

/// The window's application id: the name of the desktop file,
/// `org.mtrx.Mtrx.desktop` (`cargo xtask bundle`). Wayland docks match the
/// two to show the app's name and logo; on X11 it is the window class.
const APP_ID: &str = "org.mtrx.Mtrx";

pub fn window_settings() -> PlatformSpecific {
    PlatformSpecific {
        application_id: APP_ID.into(),
        ..PlatformSpecific::default()
    }
}
