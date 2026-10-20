//! Windows integration.

use std::{path::PathBuf, sync::LazyLock};

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
