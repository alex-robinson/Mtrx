//! Shared sizes and widget styles.

use crate::tint::Tint;

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
/// Kept free right of message text, so lines stay shorter and easier to
/// read.
pub const TEXT_MARGIN: f32 = 72.0;
/// A message's hover toolbar: its icons, the padding of each, and around
/// them all. Messages are at least as tall, so it always fits.
pub const TOOL_ICON: f32 = 16.0;
pub const TOOL_PADDING: f32 = 3.0;
pub const TOOLBAR_PADDING: f32 = 1.0;
pub const TOOLBAR_HEIGHT: f32 = TOOL_ICON + 2.0 * (TOOL_PADDING + TOOLBAR_PADDING);
/// Width of the timeline that message text cannot use.
pub const MESSAGE_INSET: f32 =
    2.0 * (TIMELINE_PADDING + MESSAGE_PADDING) + SENDER_AVATAR + GUTTER_SPACING + TEXT_MARGIN;

/// Text contrast text needs against its background (WCAG AA).
pub const MIN_CONTRAST: f32 = 4.5;

/// WCAG contrast ratio, from 1 (none) to 21.
pub fn contrast(a: Color, b: Color) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// WCAG relative luminance of an sRGB colour.
pub fn luminance(color: Color) -> f32 {
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

/// Behind selected text.
pub fn selection(theme: &Theme) -> Color {
    theme
        .extended_palette()
        .primary
        .weak
        .color
        .scale_alpha(0.55)
}

/// Behind text found (⌘F), and the current match: stronger and outlined.
pub fn found(theme: &Theme, current: bool) -> (Color, Border) {
    let palette = theme.extended_palette().warning;
    if current {
        (
            palette.base.color.scale_alpha(0.6),
            Border::default()
                .rounded(2)
                .width(1.5)
                .color(palette.strong.color),
        )
    } else {
        (palette.base.color.scale_alpha(0.3), Border::default())
    }
}

pub fn muted(theme: &Theme) -> Color {
    theme.extended_palette().background.strong.color
}

/// The room list, in its colour.
pub fn room_list(tint: Tint) -> impl Fn(&Theme) -> container::Style {
    move |theme| container::Style {
        background: Some(Background::Color(tint.color(theme))),
        ..Default::default()
    }
}

/// A colour to choose, ringed when chosen.
pub fn swatch(theme: &Theme, status: button::Status, tint: Tint, selected: bool) -> button::Style {
    let palette = theme.extended_palette();
    let ring = if selected {
        Border::default()
            .rounded(11)
            .width(2)
            .color(palette.primary.base.color)
    } else if matches!(status, button::Status::Hovered) {
        Border::default()
            .rounded(11)
            .width(2)
            .color(palette.background.strong.color)
    } else {
        Border::default()
            .rounded(11)
            .width(1)
            .color(palette.background.strong.color)
    };
    button::Style {
        background: Some(Background::Color(tint.color(theme))),
        border: ring,
        ..Default::default()
    }
}

pub fn panel(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        ..Default::default()
    }
}

/// Hover help: iced's rounded box, rounder.
pub fn hint(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        text_color: Some(palette.background.weak.text),
        border: Border::default().rounded(6),
        ..Default::default()
    }
}

/// The address of a hovered link, in the window's bottom left corner: as
/// hover help, rounded only at its inner corner.
pub fn link_status(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        text_color: Some(palette.background.weak.text),
        border: Border::default()
            .rounded(iced::border::top_right(4))
            .width(1)
            .color(palette.background.strong.color),
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

/// A timeline row; the hovered one is highlighted, one gone to in colour.
pub fn message_row(hovered: bool, marked: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: if marked {
                Some(Background::Color(
                    palette.primary.weak.color.scale_alpha(0.3),
                ))
            } else {
                hovered.then(|| Background::Color(palette.background.strong.color.scale_alpha(0.3)))
            },
            border: Border::default().rounded(6),
            ..Default::default()
        }
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

/// On the settings gear: a new version of the app. In the accent colour,
/// apart from the black dots of unread spaces.
pub fn update_dot(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(
            theme.extended_palette().primary.base.color,
        )),
        border: Border::default().rounded(4),
        ..Default::default()
    }
}

/// An unread count: red for mentions, else a grey darker (in dark mode
/// lighter) than the room list and its selected row.
pub fn badge(theme: &Theme, highlight: bool) -> container::Style {
    let palette = theme.extended_palette();
    let pair = if highlight {
        palette.danger.base
    } else {
        palette.background.strongest
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

/// The "add reaction" button after a message's reactions, shaped like a
/// reaction pill.
pub fn reaction(own: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let palette = theme.extended_palette();
        let (background, border) = reaction_colors(theme, own);
        let background = match status {
            button::Status::Hovered => palette.background.strong.color,
            _ => background,
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: palette.background.base.text,
            border: Border::default()
                .rounded(REACTION_RADIUS)
                .width(1)
                .color(border),
            ..Default::default()
        }
    }
}

const REACTION_RADIUS: f32 = 10.0;

/// A reaction pill's background and border; highlighted if it includes
/// ours.
fn reaction_colors(theme: &Theme, own: bool) -> (Color, Color) {
    let palette = theme.extended_palette();
    if own {
        (
            palette.primary.weak.color.scale_alpha(0.4),
            palette.primary.base.color,
        )
    } else {
        (
            palette.background.weak.color,
            palette.background.strong.color,
        )
    }
}

/// A reaction pill, around its two halves (see [`reaction_half`]) inside
/// its 1px border.
pub fn reaction_pill(own: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let (background, border) = reaction_colors(theme, own);
        container::Style {
            background: Some(Background::Color(background)),
            text_color: Some(theme.extended_palette().background.base.text),
            border: Border::default()
                .rounded(REACTION_RADIUS)
                .width(1)
                .color(border),
            ..Default::default()
        }
    }
}

/// The line between a reaction pill's halves, in its border's colour.
pub fn reaction_divider(own: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| container::Style {
        background: Some(Background::Color(reaction_colors(theme, own).1)),
        ..Default::default()
    }
}

/// One half of a reaction pill, the `left` (the emoji) or right (the
/// count): clear, darker on hover, rounded on its outer side.
pub fn reaction_half(left: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let palette = theme.extended_palette();
        let inner = REACTION_RADIUS - 1.0;
        let radius = if left {
            iced::border::Radius::default().left(inner)
        } else {
            iced::border::Radius::default().right(inner)
        };
        button::Style {
            background: matches!(status, button::Status::Hovered)
                .then_some(Background::Color(palette.background.strong.color)),
            text_color: palette.background.base.text,
            border: Border {
                radius,
                ..Default::default()
            },
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

/// Where a dragged section would land, between two others: a line while
/// `active`, else nothing.
pub fn drop_line(theme: &Theme, active: bool) -> container::Style {
    container::Style {
        background: active.then(|| Background::Color(theme.extended_palette().primary.base.color)),
        border: Border::default().rounded(1),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tint::Tint;

    /// `top`, with its alpha, over `bottom`.
    fn over(bottom: Color, top: Color) -> Color {
        let mix = |b: f32, t: f32| b * (1.0 - top.a) + t * top.a;
        Color::from_rgb(
            mix(bottom.r, top.r),
            mix(bottom.g, top.g),
            mix(bottom.b, top.b),
        )
    }

    #[test]
    fn unread_counts_are_readable() {
        for theme in [Theme::Light, Theme::Dark] {
            for highlight in [false, true] {
                let style = badge(&theme, highlight);
                let Some(Background::Color(bubble)) = style.background else {
                    panic!("a badge has a colour");
                };
                let text = style.text_color.unwrap();
                assert!(
                    contrast(text, bubble) >= MIN_CONTRAST,
                    "{theme} highlight {highlight}: {}",
                    contrast(text, bubble)
                );
            }
            // The grey bubble stands out from every room list colour, and
            // from the selected row.
            let Some(Background::Color(bubble)) = badge(&theme, false).background else {
                unreachable!()
            };
            let Some(Background::Color(selected)) =
                list_item(true)(&theme, button::Status::Active).background
            else {
                unreachable!()
            };
            for tint in Tint::ALL {
                let list = tint.color(&theme);
                assert!(contrast(bubble, list) >= 1.3, "{theme} {tint:?}");
                let row = over(list, selected);
                assert!(contrast(bubble, row) >= 1.1, "{theme} {tint:?} selected");
            }
        }
    }
}
