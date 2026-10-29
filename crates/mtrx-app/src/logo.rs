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

pub fn view<'a, Message: 'a>(size: f32) -> Element<'a, Message> {
    svg(Handle::from_memory(SVG))
        .width(size)
        .height(size)
        .into()
}

/// The logo as a window icon (Windows and Linux; macOS uses the bundle's).
pub fn window_icon() -> Option<window::Icon> {
    let tree = usvg::Tree::from_data(SVG, &usvg::Options::default()).ok()?;
    let rgba = rasterize(&tree, ICON_SIZE)?;
    window::icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE).ok()
}

/// A square image `size` pixels wide, as straight (not premultiplied) RGBA.
pub fn rasterize(tree: &usvg::Tree, size: u32) -> Option<Vec<u8>> {
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let scale = size as f32 / tree.size().width();
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(
        pixmap
            .pixels()
            .iter()
            .flat_map(|p| {
                let c = p.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect(),
    )
}
