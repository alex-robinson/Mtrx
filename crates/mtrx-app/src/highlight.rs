//! What a field one writes in marks while one types: emoji, drawn with the
//! emoji font; mentions; and misspelled words, with a wavy line.

use std::{ops::Range, sync::Arc};

use iced::advanced::text::highlighter::Format;

use crate::{emoji, spelling, style};

/// What [`Highlighter`] marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Highlight {
    /// Drawn with the emoji font (see [`emoji::runs`]).
    Emoji,
    Mention,
    Misspelled,
}

/// What a field marks besides emoji.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marks {
    /// The texts `@` may mention: `@` + one of them is a mention (see
    /// [`mtrx_format::find_mentions`]).
    pub mentions: Arc<[String]>,
    /// The misspelled words, as [`spelling::marking`] has them: none while
    /// spell checking is off.
    pub spelling: Option<u64>,
}

impl Marks {
    /// The marks of a field with `mentions`, misspelled words as Settings
    /// say.
    pub fn new(mentions: Arc<[String]>) -> Self {
        Self {
            mentions,
            spelling: spelling::marking(),
        }
    }
}

/// Marks a field's emoji, mentions and misspelled words.
pub struct Highlighter {
    marks: Marks,
    line: usize,
    /// For each line highlighted, whether the lines after it are in a block
    /// of code or math (see [`spelling::skip::is_fence`]), not checked.
    in_block: Vec<bool>,
}

impl iced::advanced::text::Highlighter for Highlighter {
    type Settings = Marks;
    type Highlight = Highlight;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Highlight)>;

    fn new(marks: &Self::Settings) -> Self {
        Self {
            marks: marks.clone(),
            line: 0,
            in_block: Vec::new(),
        }
    }

    fn update(&mut self, marks: &Self::Settings) {
        self.marks = marks.clone();
        self.change_line(0);
    }

    fn change_line(&mut self, line: usize) {
        self.line = self.line.min(line);
        self.in_block.truncate(self.line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let in_block = self.in_block.last().copied().unwrap_or(false);
        let fence = spelling::skip::is_fence(line);
        self.in_block.push(in_block != fence);
        self.line += 1;
        let check = self.marks.spelling.is_some() && !in_block && !fence;
        highlights(line, &self.marks.mentions, check).into_iter()
    }

    fn current_line(&self) -> usize {
        self.line
    }
}

/// The emoji and mentions of `line`, in order, emoji within a mention being
/// emoji; then, if `check`, its misspelled words outside them.
fn highlights(line: &str, mentions: &[String], check: bool) -> Vec<(Range<usize>, Highlight)> {
    let found = mtrx_format::find_mentions(line, mentions);
    let mut out = Vec::new();
    let mut at = 0;
    for (is_emoji, run) in emoji::runs(line) {
        let range = at..at + run.len();
        at = range.end;
        if is_emoji {
            out.push((range, Highlight::Emoji));
            continue;
        }
        for (mention, _) in &found {
            let (start, end) = (mention.start.max(range.start), mention.end.min(range.end));
            if start < end {
                out.push((start..end, Highlight::Mention));
            }
        }
    }
    if check {
        let words = spelling::misspelled(line)
            .into_iter()
            .filter(|word| {
                !out.iter()
                    .any(|(r, _)| r.start < word.end && word.start < r.end)
            })
            .map(|word| (word, Highlight::Misspelled))
            .collect::<Vec<_>>();
        out.extend(words);
    }
    out
}

/// How [`Highlighter`]'s highlights are drawn.
pub fn format(highlight: &Highlight, theme: &iced::Theme) -> Format<iced::Font> {
    match highlight {
        Highlight::Emoji => Format {
            font: Some(crate::fonts::EMOJI),
            ..Format::default()
        },
        Highlight::Mention => Format {
            color: Some(theme.extended_palette().primary.base.color),
            ..Format::default()
        },
        Highlight::Misspelled => Format {
            underline: Some(style::misspelled(theme)),
            ..Format::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::text::Highlighter as _;

    use super::*;

    #[test]
    fn highlights_mentions_and_emoji() {
        let mentions = ["Nils Weitzel".to_owned(), "room".to_owned()];
        assert_eq!(
            highlights("hi @Nils Weitzel 👍 @room", &mentions, false),
            [
                (3..16, Highlight::Mention),
                (17..21, Highlight::Emoji),
                (22..27, Highlight::Mention),
            ]
        );
    }

    #[test]
    fn blocks_of_code_are_followed_across_lines_and_edits() {
        let mut highlighter = Highlighter::new(&Marks::default());
        let blocks = |highlighter: &mut Highlighter, lines: &[&str]| {
            for line in lines {
                let _ = highlighter.highlight_line(line);
            }
            highlighter.in_block.clone()
        };
        assert_eq!(
            blocks(&mut highlighter, &["text", "```", "code", "```", "text"]),
            [false, true, true, false, false]
        );
        // The second fence edited away: the rest is code.
        highlighter.change_line(3);
        assert_eq!(
            blocks(&mut highlighter, &["code", "code"]),
            [false, true, true, true, true]
        );
    }

    /// The wavy lines, to look at: `target/ui-snapshots/spelling-*.png`.
    #[test]
    #[ignore = "a snapshot to look at"]
    fn misspelled_words_are_underlined() {
        use iced::{
            Size, Theme,
            widget::{column, text_editor, text_input},
        };
        use iced_test::simulator::Simulator;

        let _system = crate::spelling::tests::SYSTEM.lock();
        let content = text_editor::Content::<iced::Renderer>::with_text(
            "See you tommorow, I will chekc the draft.\n```\nlet wrod = 1;\n```\nThanks @Nils Weitzel, grate work 👍",
        );
        let name = "Meetngs and sundry";
        let view = || -> iced::Element<'_, ()> {
            column![
                text_editor(&content)
                    .highlight_with::<Highlighter>(
                        Marks::new(["Nils Weitzel".to_owned()].into()),
                        format,
                    )
                    .style(style::editor)
                    .padding(8),
                crate::spelling::checked(text_input("Name", name).on_input(|_| ()), name)
                    .style(style::input)
                    .padding(8),
            ]
            .spacing(12)
            .padding(20)
            .into()
        };
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/ui-snapshots/spelling");
        for theme in [Theme::Light, Theme::Dark] {
            let mut ui =
                Simulator::with_size(crate::fonts::settings(), Size::new(500.0, 220.0), view());
            let snapshot = ui.snapshot(&theme).unwrap();
            let path = path.with_file_name(format!("spelling-{theme}"));
            let _ = snapshot.matches_image(&path);
        }
    }
}
