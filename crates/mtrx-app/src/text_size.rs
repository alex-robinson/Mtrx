//! The text size: one scale for the whole window, so that text, pictures
//! and spacing grow together and the layout keeps its proportions.
//!
//! iced applies it on top of the display's own scale (the application's
//! `scale_factor`): a layout unit is `display scale × text size` physical
//! pixels, e.g. 2 × 1.25 on a Retina display at Larger.
//!
//! Chosen in Settings, or stepped with ⌘+ (or ⌘=) and ⌘− and reset with ⌘0;
//! Ctrl on Windows and Linux.

use std::fmt;

use iced::keyboard::{Key, Modifiers};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextSize {
    Small,
    #[default]
    Default,
    Large,
    Larger,
    Largest,
}

impl TextSize {
    /// Smallest first, as declared: the zoom steps.
    pub const ALL: [Self; 5] = [
        Self::Small,
        Self::Default,
        Self::Large,
        Self::Larger,
        Self::Largest,
    ];

    /// Layout units per logical pixel (browsers' zoom levels).
    pub fn scale(self) -> f32 {
        match self {
            Self::Small => 0.9,
            Self::Default => 1.0,
            Self::Large => 1.1,
            Self::Larger => 1.25,
            Self::Largest => 1.5,
        }
    }

    /// A step larger or smaller, staying at the ends; or the default.
    pub fn zoom(self, zoom: Zoom) -> Self {
        let step = self as usize;
        match zoom {
            Zoom::In => Self::ALL[(step + 1).min(Self::ALL.len() - 1)],
            Zoom::Out => Self::ALL[step.saturating_sub(1)],
            Zoom::Reset => Self::default(),
        }
    }
}

impl fmt::Display for TextSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Small => "Small",
            Self::Default => "Default",
            Self::Large => "Large",
            Self::Larger => "Larger",
            Self::Largest => "Largest",
        })
    }
}

/// A zoom shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zoom {
    In,
    Out,
    Reset,
}

impl Zoom {
    /// ⌘+, or ⌘= as + needs shift, ⌘− and ⌘0; Ctrl on Windows and Linux.
    pub fn from_key(key: &Key, modifiers: Modifiers) -> Option<Self> {
        if !modifiers.command() {
            return None;
        }
        match key.as_ref() {
            Key::Character("+" | "=") => Some(Self::In),
            Key::Character("-") => Some(Self::Out),
            Key::Character("0") => Some(Self::Reset),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_through_the_sizes() {
        assert_eq!(TextSize::default(), TextSize::Default);
        assert_eq!(TextSize::Default.scale(), 1.0);
        let mut size = TextSize::Small;
        for next in &TextSize::ALL[1..] {
            assert!(next.scale() > size.scale());
            size = size.zoom(Zoom::In);
            assert_eq!(size, *next);
        }
        for previous in TextSize::ALL[..4].iter().rev() {
            size = size.zoom(Zoom::Out);
            assert_eq!(size, *previous);
        }
        for size in TextSize::ALL {
            assert_eq!(size.zoom(Zoom::Reset), TextSize::Default);
        }
    }

    #[test]
    fn stays_at_the_ends() {
        assert_eq!(TextSize::Largest.zoom(Zoom::In), TextSize::Largest);
        assert_eq!(TextSize::Small.zoom(Zoom::Out), TextSize::Small);
    }

    #[test]
    fn zoom_keys() {
        let key = |c: &str| Key::Character(c.into());
        let command = Modifiers::COMMAND;
        assert_eq!(Zoom::from_key(&key("="), command), Some(Zoom::In));
        let shifted = command | Modifiers::SHIFT;
        assert_eq!(Zoom::from_key(&key("+"), shifted), Some(Zoom::In));
        assert_eq!(Zoom::from_key(&key("-"), command), Some(Zoom::Out));
        assert_eq!(Zoom::from_key(&key("0"), command), Some(Zoom::Reset));
        assert_eq!(Zoom::from_key(&key("="), Modifiers::empty()), None);
        assert_eq!(Zoom::from_key(&key("k"), command), None);
    }
}
