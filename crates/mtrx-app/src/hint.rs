//! Hover help: a short label over a control, so people find their way
//! without having to guess what a symbol does.

use iced::{
    Element,
    widget::{container, text, tooltip},
};

use crate::style;

pub use iced::widget::tooltip::Position;

/// Longer labels wrap rather than make a very wide box.
const HINT_MAX_WIDTH: f32 = 260.0;

pub fn hint<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    label: impl text::IntoFragment<'a>,
    position: Position,
) -> Element<'a, M> {
    hint_with(content, text(label).size(12), position)
}

/// A hint whose label is more than one text, e.g. with an emoji.
pub fn hint_with<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    label: impl Into<Element<'a, M>>,
    position: Position,
) -> Element<'a, M> {
    tooltip(
        content,
        container(label)
            .padding(6)
            .max_width(HINT_MAX_WIDTH)
            .style(style::hint),
        position,
    )
    .into()
}

/// A keyboard shortcut as the platform writes it: "⌘K" on macOS, "Ctrl+K"
/// elsewhere (iced's command modifier is Ctrl there).
pub fn shortcut(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{key}")
    } else {
        format!("Ctrl+{key}")
    }
}
