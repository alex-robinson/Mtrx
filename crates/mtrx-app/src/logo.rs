//! The Mtrx logo: on the sign-in screen, as the window icon, and (through
//! `cargo xtask bundle`) as the macOS app icon.

use iced::{
    Element,
    widget::{svg, svg::Handle},
    window,
};
use resvg::{tiny_skia, usvg};

const SVG: &[u8] = include_bytes!("../assets/logo.svg");
/// Window icons are scaled down from this size.
const ICON_SIZE: u32 = 256;
/// Also in the bundle's Info.plist (`crates/xtask`).
#[cfg(target_os = "macos")]
const BUNDLE_ID: &str = "org.mtrx.Mtrx";

pub fn view<'a, Message: 'a>(size: f32) -> Element<'a, Message> {
    svg(Handle::from_memory(SVG))
        .width(size)
        .height(size)
        .into()
}

/// The logo as a window icon (Windows and Linux; macOS uses the bundle's).
pub fn window_icon() -> Option<window::Icon> {
    let tree = usvg::Tree::from_data(SVG, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(ICON_SIZE, ICON_SIZE)?;
    let scale = ICON_SIZE as f32 / tree.size().width();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    window::icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE).ok()
}

/// Notifications come from the app when it runs as `Mtrx.app`, so they
/// show its name and icon and clicks reach it. Outside a bundle the
/// notification library keeps its default sender.
#[cfg(target_os = "macos")]
pub fn notifications_from_bundle() {
    let bundled = std::env::current_exe()
        .is_ok_and(|exe| exe.to_string_lossy().contains(".app/Contents/MacOS/"));
    if bundled && let Err(e) = notify_rust::set_application(BUNDLE_ID) {
        tracing::warn!("notifications keep the default sender: {e}");
    }
}
