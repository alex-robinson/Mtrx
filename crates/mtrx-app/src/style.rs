//! Shared sizes and widget styles.

use iced::{
    Background, Border, Color, Theme,
    widget::{Container, Text, button, container, rule, text},
};

pub const SPACE_BAR_WIDTH: f32 = 56.0;
pub const ROOM_LIST_WIDTH: f32 = 260.0;
pub const TEXT_SIZE: f32 = 15.0;

pub const SPACE_AVATAR: f32 = 32.0;
pub const ROOM_AVATAR: f32 = 24.0;
pub const HEADER_AVATAR: f32 = 32.0;
pub const SENDER_AVATAR: f32 = 28.0;
/// Height of a room list entry.
pub const ROOM_ROW: f32 = 32.0;

/// Horizontal padding of the timeline column, per side.
pub const TIMELINE_PADDING: f32 = 12.0;
/// Horizontal padding inside a message row (its hover background), per side.
pub const MESSAGE_PADDING: f32 = 8.0;
/// Between the sender avatar column and the message text.
pub const GUTTER_SPACING: f32 = 10.0;
/// Width of the timeline that message text cannot use.
pub const MESSAGE_INSET: f32 =
    2.0 * (TIMELINE_PADDING + MESSAGE_PADDING) + SENDER_AVATAR + GUTTER_SPACING;

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

/// Subtle line under headers.
pub fn separator(theme: &Theme) -> rule::Style {
    rule::Style {
        color: theme
            .extended_palette()
            .background
            .strong
            .color
            .scale_alpha(0.6),
        ..rule::default(theme)
    }
}

/// A timeline row; the hovered one is highlighted.
pub fn message_row(hovered: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| container::Style {
        background: hovered.then(|| {
            Background::Color(
                theme
                    .extended_palette()
                    .background
                    .strong
                    .color
                    .scale_alpha(0.3),
            )
        }),
        border: Border::default().rounded(6),
        ..Default::default()
    }
}

/// Home in the space bar, shaped like a space avatar.
pub fn home_tile(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        text_color: Some(palette.background.base.text),
        border: Border::default().rounded(SPACE_AVATAR / 4.0),
        ..Default::default()
    }
}

/// Unread marker on a space.
pub fn unread_dot(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.text)),
        border: Border::default()
            .rounded(5)
            .width(2)
            .color(palette.background.base.color),
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

/// A list entry (room, space): flat, a grey background when selected or
/// hovered, like Element.
pub fn list_item(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let palette = theme.extended_palette();
        let background = match (selected, status) {
            (true, _) => Some(palette.background.strong.color.scale_alpha(0.75)),
            (false, button::Status::Hovered | button::Status::Pressed) => {
                Some(palette.background.strong.color.scale_alpha(0.4))
            }
            _ => None,
        };
        button::Style {
            background: background.map(Background::Color),
            text_color: palette.background.base.text,
            border: Border::default().rounded(8),
            ..Default::default()
        }
    }
}

/// Text on one line, cut off at the container's edge.
pub fn single_line<'a, M>(content: Text<'a>) -> Container<'a, M> {
    container(content.wrapping(text::Wrapping::None)).clip(true)
}

pub fn to_rgba8(color: Color) -> [u8; 4] {
    let [r, g, b, a] = color.into_rgba8();
    [r, g, b, a]
}
