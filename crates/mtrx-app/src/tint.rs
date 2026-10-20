//! Colours for the room list, chosen per space: light ones, each with a
//! dark counterpart for dark mode.

use iced::{Color, Theme};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tint {
    #[default]
    Grey,
    Green,
    Teal,
    Blue,
    Lavender,
    Rose,
    Peach,
    Yellow,
}

impl Tint {
    pub const ALL: [Tint; 8] = [
        Tint::Grey,
        Tint::Green,
        Tint::Teal,
        Tint::Blue,
        Tint::Lavender,
        Tint::Rose,
        Tint::Peach,
        Tint::Yellow,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tint::Green => "Green",
            Tint::Teal => "Teal",
            Tint::Blue => "Blue",
            Tint::Lavender => "Lavender",
            Tint::Rose => "Rose",
            Tint::Peach => "Peach",
            Tint::Yellow => "Yellow",
            Tint::Grey => "Grey",
        }
    }

    /// The colour for `theme`: light, or dark in dark mode.
    pub fn color(self, theme: &Theme) -> Color {
        let (light, dark) = match self {
            Tint::Green => (0xe3f1e1, 0x1e2a20),
            Tint::Teal => (0xdcf0ee, 0x1a2a29),
            Tint::Blue => (0xe1eaf6, 0x1c2430),
            Tint::Lavender => (0xebe5f6, 0x26212f),
            Tint::Rose => (0xf6e1e6, 0x2e1f24),
            Tint::Peach => (0xf8e7da, 0x2e241d),
            Tint::Yellow => (0xf5f0d6, 0x2b291b),
            Tint::Grey => (0xf5f5f4, 0x252628),
        };
        let rgb = if theme.extended_palette().is_dark {
            dark
        } else {
            light
        };
        Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }
}
