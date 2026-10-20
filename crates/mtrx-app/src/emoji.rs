//! Choosing an emoji: the ones used most, then all of them by group or by
//! name.

use emojis::Group;
use iced::{
    Alignment, Element, Length,
    widget::{Id, button, column, container, row, scrollable, text, text_input},
};

use crate::{app, fonts, style};

/// Offered before any have been used, and to fill up the most used.
const COMMON: [&str; 8] = ["👍", "❤️", "😂", "🎉", "😮", "😢", "🙏", "👀"];
/// Emoji in the quick row.
const QUICK: usize = 8;
const CELL: f32 = 34.0;
const COLUMNS: usize = 9;
/// Search results shown.
const MAX_FOUND: usize = 9 * 12;
/// Newer emoji are left out: fonts of older systems cannot draw them.
const MAX_VERSION: (u32, u32) = (15, 1);
/// The search field, focused when all emoji open.
pub const SEARCH: &str = "emoji-search";

/// Each group, by the emoji on its tab.
const GROUPS: [(Group, &str); 9] = [
    (Group::SmileysAndEmotion, "😀"),
    (Group::PeopleAndBody, "👋"),
    (Group::AnimalsAndNature, "🐻"),
    (Group::FoodAndDrink, "🍔"),
    (Group::TravelAndPlaces, "✈️"),
    (Group::Activities, "⚽"),
    (Group::Objects, "💡"),
    (Group::Symbols, "❤️"),
    (Group::Flags, "🏁"),
];

#[derive(Debug, Clone)]
pub enum Message {
    /// Show all emoji, or only the most used.
    ToggleAll,
    Search(String),
    Group(Group),
}

/// All emoji, while shown.
pub struct Panel {
    pub query: String,
    pub group: Group,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            query: String::new(),
            group: Group::SmileysAndEmotion,
        }
    }
}

/// The quick row: the most used, filled up with common ones.
pub fn quick(frequent: &[String]) -> Vec<&str> {
    let mut quick: Vec<&str> = frequent.iter().map(String::as_str).take(QUICK).collect();
    for emoji in COMMON {
        if quick.len() == QUICK {
            break;
        }
        if !quick.contains(&emoji) {
            quick.push(emoji);
        }
    }
    quick
}

impl Panel {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::Search(query) => self.query = query,
            Message::Group(group) => {
                self.group = group;
                self.query.clear();
            }
            Message::ToggleAll => {}
        }
    }

    /// The emoji shown: matching the search, else the chosen group's.
    fn shown(&self) -> Vec<&'static str> {
        let query = self.query.trim().to_lowercase();
        if query.is_empty() {
            return self
                .group
                .emojis()
                .filter(|e| drawable(e))
                .map(|e| e.as_str())
                .collect();
        }
        emojis::iter()
            .filter(|e| drawable(e))
            .filter(|e| {
                e.name().contains(&query) || e.shortcodes().any(|code| code.contains(&query))
            })
            .take(MAX_FOUND)
            .map(|e| e.as_str())
            .collect()
    }

    /// Search, group tabs and a grid of emoji; `pick` makes the message
    /// for a chosen one.
    pub fn view<'a>(
        &'a self,
        pick: impl Fn(String) -> app::Message,
        wrap: fn(Message) -> app::Message,
    ) -> Element<'a, app::Message> {
        let search = text_input("Search emoji", &self.query)
            .id(Id::new(SEARCH))
            .on_input(move |query| wrap(Message::Search(query)))
            .style(style::input)
            .padding([4, 8])
            .size(13);
        let searching = !self.query.trim().is_empty();
        let tabs = row(GROUPS.map(|(group, icon)| {
            button(
                text(icon)
                    .size(16)
                    .font(fonts::EMOJI)
                    .center()
                    .width(Length::Fill),
            )
            .width(CELL)
            .padding([2, 0])
            .style(style::list_item(!searching && group == self.group))
            .on_press(wrap(Message::Group(group)))
            .into()
        }));
        let shown = self.shown();
        let grid: Element<'a, app::Message> = if shown.is_empty() {
            text("No emoji found")
                .size(12)
                .style(text::secondary)
                .into()
        } else {
            column(shown.chunks(COLUMNS).map(|line| {
                row(line.iter().map(|emoji| {
                    button(
                        text(*emoji)
                            .size(20)
                            .font(fonts::EMOJI)
                            .center()
                            .width(Length::Fill),
                    )
                    .width(CELL)
                    .height(CELL)
                    .padding(0)
                    .style(style::list_item(false))
                    .on_press(pick((*emoji).to_owned()))
                    .into()
                }))
                .into()
            }))
            .into()
        };
        let width = CELL * COLUMNS as f32 + 12.0;
        container(
            column![
                search.width(width),
                tabs,
                scrollable(grid).height(CELL * 6.0).width(width),
            ]
            .spacing(6)
            .align_x(Alignment::Start),
        )
        .padding(6)
        .style(style::toolbar)
        .into()
    }
}

fn drawable(emoji: &emojis::Emoji) -> bool {
    let version = emoji.unicode_version();
    (version.major(), version.minor()) <= MAX_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn most_used_then_common() {
        assert_eq!(quick(&[]), COMMON);
        let used = ["🦀".to_owned(), "👍".to_owned()];
        assert_eq!(
            quick(&used),
            ["🦀", "👍", "❤️", "😂", "🎉", "😮", "😢", "🙏"]
        );
    }

    #[test]
    fn finds_emoji_by_name_and_shortcode() {
        let mut panel = Panel::default();
        panel.update(Message::Search("crab".into()));
        assert!(panel.shown().contains(&"🦀"));
        panel.update(Message::Search("tada".into()));
        assert!(panel.shown().contains(&"🎉"));
        // Too new for most fonts.
        panel.update(Message::Search("cracking face".into()));
        assert!(panel.shown().is_empty());
        panel.update(Message::Group(Group::Flags));
        assert!(panel.query.is_empty() && panel.shown().len() > 100);
    }
}
