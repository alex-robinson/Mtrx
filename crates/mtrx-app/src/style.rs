//! Shared sizes and widget styles.

use iced::{
    Background, Border, Color, Theme,
    widget::{button, container},
};

pub const SPACE_BAR_WIDTH: f32 = 64.0;
pub const ROOM_LIST_WIDTH: f32 = 280.0;
/// Horizontal padding of the timeline column, both sides together.
pub const TIMELINE_PADDING: f32 = 48.0;
pub const TEXT_SIZE: f32 = 15.0;

pub fn muted(theme: &Theme) -> Color {
    theme.extended_palette().background.strong.color
}

pub fn panel(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        ..Default::default()
    }
}

pub fn space_bar(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(
            palette.background.strong.color.scale_alpha(0.5),
        )),
        ..Default::default()
    }
}

pub fn banner(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.warning.weak.color)),
        text_color: Some(palette.warning.weak.text),
        border: Border::default().rounded(6),
        ..Default::default()
    }
}

pub fn code_block(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(
            palette.background.strong.color.scale_alpha(0.4),
        )),
        border: Border::default().rounded(4),
        ..Default::default()
    }
}

pub fn quote_bar(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(muted(theme))),
        ..Default::default()
    }
}

pub fn badge(theme: &Theme, highlight: bool) -> container::Style {
    let palette = theme.extended_palette();
    let pair = if highlight {
        palette.danger.base
    } else {
        palette.secondary.base
    };
    container::Style {
        background: Some(Background::Color(pair.color)),
        text_color: Some(pair.text),
        border: Border::default().rounded(8),
        ..Default::default()
    }
}

/// A list entry (room, space): flat, highlighted when selected or hovered.
pub fn list_item(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let palette = theme.extended_palette();
        let background = match (selected, status) {
            (true, _) => Some(palette.primary.weak.color),
            (false, button::Status::Hovered) => {
                Some(palette.background.strong.color.scale_alpha(0.6))
            }
            _ => None,
        };
        button::Style {
            background: background.map(Background::Color),
            text_color: if selected {
                palette.primary.weak.text
            } else {
                palette.background.base.text
            },
            border: Border::default().rounded(6),
            ..Default::default()
        }
    }
}

pub fn to_rgba8(color: Color) -> [u8; 4] {
    let [r, g, b, a] = color.into_rgba8();
    [r, g, b, a]
}
