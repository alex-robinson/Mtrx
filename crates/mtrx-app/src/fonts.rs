//! Bundled Inter, used for the UI and for typeset (math) messages alike, so
//! both look the same.

use iced::{
    Font, Settings,
    font::{Style, Weight},
};

use crate::style::TEXT_SIZE;

pub const FILES: &[&[u8]] = &[
    include_bytes!("../fonts/Inter-Regular.ttf"),
    include_bytes!("../fonts/Inter-Italic.ttf"),
    include_bytes!("../fonts/Inter-SemiBold.ttf"),
    include_bytes!("../fonts/Inter-Bold.ttf"),
    include_bytes!("../fonts/Inter-BoldItalic.ttf"),
];

pub const FAMILY: &str = "Inter";
pub const INTER: Font = Font::with_name(FAMILY);
pub const SEMIBOLD: Font = Font {
    weight: Weight::Semibold,
    ..INTER
};

pub fn styled(bold: bool, italic: bool) -> Font {
    Font {
        weight: if bold { Weight::Bold } else { Weight::Normal },
        style: if italic { Style::Italic } else { Style::Normal },
        ..INTER
    }
}

/// App settings: bundled fonts and text size, shared with UI tests.
pub fn settings() -> Settings {
    Settings {
        fonts: FILES.iter().map(|f| (*f).into()).collect(),
        default_font: INTER,
        default_text_size: TEXT_SIZE.into(),
        ..Settings::default()
    }
}
