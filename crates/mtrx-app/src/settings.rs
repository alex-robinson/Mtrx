//! App preferences, shared by all accounts, in `settings.json` next to the
//! profiles.

use std::{
    io,
    path::{Path, PathBuf},
};

use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, space, text, toggler},
};
use serde::{Deserialize, Serialize};

use crate::{app::Message, fonts, style};

const FILE: &str = "settings.json";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Senders' names in their colour (as in Element) instead of the text colour.
    pub colored_names: bool,
}

impl Settings {
    pub fn path(data_root: &Path) -> PathBuf {
        data_root.join(FILE)
    }

    /// The saved settings; defaults if there are none or they cannot be read.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)
    }

    /// The settings panel.
    pub fn view<'a>(&self, error: Option<&'a str>) -> Element<'a, Message> {
        let option = |label: &'a str, description: &'a str, toggle: Element<'a, Message>| {
            row![
                column![
                    text(label),
                    text(description).size(12).style(text::secondary)
                ]
                .spacing(2)
                .width(Length::Fill),
                toggle,
            ]
            .spacing(16)
            .align_y(Alignment::Center)
        };
        let mut panel = column![
            row![
                text("Settings").size(18).font(fonts::SEMIBOLD),
                space::horizontal(),
                button(text("Done"))
                    .style(style::primary_button)
                    .on_press(Message::ShowSettings(false)),
            ]
            .align_y(Alignment::Center),
            text("CHAT")
                .size(11)
                .font(fonts::SEMIBOLD)
                .style(text::secondary),
            option(
                "Coloured names",
                "Show people's names in their own colour, as Element does.",
                toggler(self.colored_names)
                    .on_toggle(Message::ColoredNames)
                    .into(),
            ),
        ]
        .spacing(14);
        if let Some(error) = error {
            panel = panel.push(
                text(format!("Could not save settings: {error}"))
                    .size(12)
                    .style(text::danger),
            );
        }
        container(panel)
            .padding(20)
            .width(440)
            .style(style::dialog)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = Settings::path(dir.path());
        assert_eq!(Settings::load(&path), Settings::default(), "missing file");
        let settings = Settings {
            colored_names: true,
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(Settings::load(&path), Settings::default(), "older file");
    }
}
