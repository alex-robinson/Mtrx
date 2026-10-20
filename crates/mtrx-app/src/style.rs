//! Shared sizes and widget styles.

use iced::{
    Background, Border, Color, Theme,
    widget::{button, container, rule, text_editor, text_input},
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

/// The bar along the bottom while offline.
pub fn offline_bar(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.warning.weak.color)),
        text_color: Some(palette.warning.weak.text),
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

/// Corner radius of buttons.
const BUTTON_RADIUS: f32 = 6.0;
/// Corner radius of text fields.
const INPUT_RADIUS: f32 = 8.0;

fn rounded_button(
    base: fn(&Theme, button::Status) -> button::Style,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let style = base(theme, status);
        button::Style {
            border: style.border.rounded(BUTTON_RADIUS),
            ..style
        }
    }
}

pub fn primary_button(theme: &Theme, status: button::Status) -> button::Style {
    rounded_button(button::primary)(theme, status)
}

/// Grey; in light mode a light grey, so the dark text on it stays readable
/// (iced's is a mid grey there).
pub fn secondary_button(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    if palette.is_dark {
        return rounded_button(button::secondary)(theme, status);
    }
    let shade = |amount: f32| {
        let (background, text) = (palette.background.base.color, palette.background.base.text);
        let mix = |a: f32, b: f32| a + (b - a) * amount;
        Color::from_rgb(
            mix(background.r, text.r),
            mix(background.g, text.g),
            mix(background.b, text.b),
        )
    };
    let base = button::Style {
        background: Some(Background::Color(shade(0.1))),
        text_color: palette.background.base.text,
        border: Border::default().rounded(BUTTON_RADIUS),
        ..Default::default()
    };
    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(shade(0.16))),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(shade(0.22))),
            ..base
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(shade(0.06))),
            text_color: base.text_color.scale_alpha(0.4),
            ..base
        },
    }
}

pub fn text_button(theme: &Theme, status: button::Status) -> button::Style {
    rounded_button(button::text)(theme, status)
}

pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let style = text_input::default(theme, status);
    text_input::Style {
        border: style.border.rounded(INPUT_RADIUS),
        ..style
    }
}

pub fn editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let style = text_editor::default(theme, status);
    text_editor::Style {
        border: style.border.rounded(INPUT_RADIUS),
        ..style
    }
}

/// A panel floating above the window, e.g. settings.
pub fn dialog(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        border: Border::default()
            .rounded(10)
            .width(1)
            .color(palette.background.strong.color),
        shadow: iced::Shadow {
            color: Color::BLACK.scale_alpha(0.25),
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 16.0,
        },
        ..Default::default()
    }
}

/// The "+" tile below the spaces: an outline of a space avatar.
pub fn new_space_tile(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        text_color: Some(palette.background.base.text.scale_alpha(0.7)),
        border: Border::default()
            .rounded(SPACE_AVATAR / 4.0)
            .width(1.5)
            .color(palette.background.strong.color),
        ..Default::default()
    }
}

pub fn danger_button(theme: &Theme, status: button::Status) -> button::Style {
    rounded_button(button::danger)(theme, status)
}

/// The quoted start of a replied-to message, and the composer's reply bar.
pub fn reply_quote(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        border: Border::default()
            .rounded(4)
            .width(0)
            .color(palette.primary.base.color),
        ..Default::default()
    }
}

/// The bar left of a reply quote.
pub fn quote_accent(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(
            theme.extended_palette().primary.base.color,
        )),
        border: Border::default().rounded(2),
        ..Default::default()
    }
}

/// Buttons floating over a hovered message.
pub fn toolbar(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        border: Border::default()
            .rounded(6)
            .width(1)
            .color(palette.background.strong.color),
        shadow: iced::Shadow {
            color: Color::BLACK.scale_alpha(0.12),
            offset: iced::Vector::new(0.0, 1.0),
            blur_radius: 4.0,
        },
        ..Default::default()
    }
}

/// A reaction pill; highlighted if it includes ours.
pub fn reaction(own: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let palette = theme.extended_palette();
        let (background, border) = if own {
            (
                palette.primary.weak.color.scale_alpha(0.4),
                palette.primary.base.color,
            )
        } else {
            (
                palette.background.weak.color,
                palette.background.strong.color,
            )
        };
        let background = match status {
            button::Status::Hovered => palette.background.strong.color,
            _ => background,
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: palette.background.base.text,
            border: Border::default().rounded(10).width(1).color(border),
            ..Default::default()
        }
    }
}

/// A call card in the timeline.
/// A card that opens something: a [`card`], darker on hover.
pub fn card_button(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let card = card(theme);
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => palette.background.strong.color,
        _ => palette.background.weak.color,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: palette.background.base.text,
        border: card.border,
        ..Default::default()
    }
}

pub fn card(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        border: Border::default()
            .rounded(8)
            .width(1)
            .color(palette.background.strong.color),
        ..Default::default()
    }
}

/// The round icon of a call card.
pub fn call_icon(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(
            theme.extended_palette().primary.base.color,
        )),
        border: Border::default().rounded(16),
        ..Default::default()
    }
}

/// Dims the window behind a dialog.
pub fn backdrop(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::BLACK.scale_alpha(0.35))),
        ..Default::default()
    }
}

pub fn to_rgba8(color: Color) -> [u8; 4] {
    let [r, g, b, a] = color.into_rgba8();
    [r, g, b, a]
}

/// Corner radius of images in messages.
pub const MEDIA_RADIUS: f32 = 8.0;

/// Where an image shows once loaded.
pub fn media_placeholder(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(
            theme
                .extended_palette()
                .background
                .strong
                .color
                .scale_alpha(0.4),
        )),
        border: Border::default().rounded(MEDIA_RADIUS),
        ..Default::default()
    }
}

/// A PDF page in the timeline: a thin edge, so white pages stand out.
pub fn pdf_page(theme: &Theme) -> container::Style {
    container::Style {
        border: Border::default()
            .width(1)
            .color(theme.extended_palette().background.strong.color),
        shadow: iced::Shadow {
            color: Color::BLACK.scale_alpha(0.1),
            offset: iced::Vector::new(0.0, 1.0),
            blur_radius: 3.0,
        },
        ..Default::default()
    }
}

/// Darkens the window behind the image viewer.
pub fn viewer_backdrop(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::BLACK.scale_alpha(0.8))),
        ..Default::default()
    }
}

/// A room list section while a room is dragged; highlighted under the cursor.
pub fn drop_target(theme: &Theme, active: bool) -> container::Style {
    let palette = theme.extended_palette();
    if !active {
        return container::Style::default();
    }
    container::Style {
        background: Some(Background::Color(
            palette.primary.weak.color.scale_alpha(0.2),
        )),
        border: Border::default()
            .rounded(8)
            .width(1)
            .color(palette.primary.base.color.scale_alpha(0.6)),
        ..Default::default()
    }
}

/// A day of the calendar or an entry of a time wheel; filled when picked,
/// faint when not available.
pub fn wheel_item(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let palette = theme.extended_palette();
    let (background, text_color) = match (selected, status) {
        (true, _) => (Some(palette.primary.base.color), palette.primary.base.text),
        (false, button::Status::Disabled) => (None, palette.background.base.text.scale_alpha(0.3)),
        (false, button::Status::Hovered | button::Status::Pressed) => (
            Some(palette.background.strong.color.scale_alpha(0.5)),
            palette.background.base.text,
        ),
        (false, button::Status::Active) => (None, palette.background.base.text),
    };
    button::Style {
        background: background.map(Background::Color),
        text_color,
        border: Border::default().rounded(6),
        ..Default::default()
    }
}
