//! Native rendering of a message [`Document`], with selectable text: each
//! paragraph, heading and code block is a unit of [`crate::selection`].
//! Messages with math use a typeset image instead (see [`crate::math`]);
//! this view is the fallback while that image is not ready, showing math as
//! LaTeX code.

use std::rc::Rc;

use iced::{
    Element, Font, Length, Pixels, Theme,
    widget::{column, container, row, rule, span, text, text::Span},
};
use mtrx_format::{Block, Document, Inline, Style};

use crate::{
    emoji, fonts,
    selectable::selectable,
    selection::{Gesture, Pos, Unit, unit_range},
    style,
};

#[derive(Clone, Copy, Default)]
struct Flags {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    code: bool,
}

/// How a message's text can be selected.
pub struct Select<'a, M> {
    /// The part of this message that is selected.
    pub range: Option<(Pos, Pos)>,
    /// A selection is being dragged, so moving over the text extends it.
    pub dragging: bool,
    pub on_gesture: Rc<dyn Fn(Gesture) -> M + 'a>,
}

pub fn view<'a, M: Clone + 'a>(
    document: &Document,
    theme: &Theme,
    on_link: fn(String) -> M,
    select: Select<'a, M>,
) -> Element<'a, M> {
    let mut walk = Walk {
        theme,
        on_link: Some(on_link),
        select: Some(select),
        units: Vec::new(),
    };
    walk.blocks(&document.blocks, "", "\n\n")
}

/// The message's units, as shown, for copying a selection.
pub fn units(document: &Document) -> Vec<Unit> {
    let mut walk: Walk<'_, '_, ()> = Walk {
        theme: &Theme::Light,
        on_link: None,
        select: None,
        units: Vec::new(),
    };
    let _ = walk.blocks(&document.blocks, "", "\n\n");
    walk.units
}

/// Builds the view of blocks and records their units, in the same order.
struct Walk<'t, 'a, M> {
    theme: &'t Theme,
    on_link: Option<fn(String) -> M>,
    select: Option<Select<'a, M>>,
    units: Vec<Unit>,
}

impl<'a, M: Clone + 'a> Walk<'_, 'a, M> {
    /// `prefix` goes before each unit's text when copied (quote marks and
    /// list indents); `gap` between blocks.
    fn blocks(&mut self, list: &[Block], prefix: &str, gap: &'static str) -> Element<'a, M> {
        let mut out = column![].spacing(6);
        for block in list {
            out = out.push(self.block(block, prefix, gap));
        }
        out.into()
    }

    fn block(&mut self, block: &Block, prefix: &str, gap: &'static str) -> Element<'a, M> {
        match block {
            Block::Paragraph(content) => self.paragraph(content, None, prefix, gap),
            Block::Heading { content, .. } => self.paragraph(content, Some(16.0), prefix, gap),
            Block::Quote(inner) => row![
                container(text(""))
                    .width(3)
                    .height(Length::Fill)
                    .style(style::quote_bar),
                self.blocks(inner, &format!("{prefix}> "), gap),
            ]
            .spacing(8)
            .height(Length::Shrink)
            .into(),
            Block::List { start, items } => {
                let mut list = column![].spacing(2);
                for (i, item) in items.iter().enumerate() {
                    let (shown, copied) = match start {
                        Some(start) => {
                            let marker = format!("{}.", *start + i as u64);
                            (marker.clone(), format!("{marker} "))
                        }
                        None => ("•".to_owned(), "- ".to_owned()),
                    };
                    // The item's first unit gets the marker, the rest its
                    // indent; items are a line apart.
                    let first = self.units.len();
                    let indent = format!("{prefix}{}", " ".repeat(copied.len()));
                    let content = self.blocks(item, &indent, "\n");
                    if let Some(unit) = self.units.get_mut(first) {
                        unit.prefix = format!("{prefix}{copied}");
                        unit.gap = if i == 0 { gap } else { "\n" };
                    }
                    list = list.push(row![text(shown).width(20), content]);
                }
                list.into()
            }
            Block::CodeBlock { code, .. } => {
                let code = code.trim_end().to_owned();
                container(self.unit(
                    vec![span(code).font(Font::MONOSPACE)],
                    Some(13.0),
                    prefix,
                    gap,
                ))
                .padding(8)
                .width(Length::Fill)
                .style(style::code_block)
                .into()
            }
            Block::Math(tex) => self.unit(
                vec![span(format!("$${tex}$$")).font(Font::MONOSPACE)],
                Some(13.0),
                prefix,
                gap,
            ),
            Block::Rule => rule::horizontal(1).into(),
        }
    }

    fn paragraph(
        &mut self,
        content: &[Inline],
        size: Option<f32>,
        prefix: &str,
        gap: &'static str,
    ) -> Element<'a, M> {
        let mut out = Vec::new();
        let flags = Flags {
            bold: size.is_some(),
            ..Flags::default()
        };
        spans(content, flags, None, self.theme, &mut out);
        self.unit(out, size, prefix, gap)
    }

    /// One selectable unit of text.
    fn unit(
        &mut self,
        spans: Vec<Span<'static, String, Font>>,
        size: Option<f32>,
        prefix: &str,
        gap: &'static str,
    ) -> Element<'a, M> {
        let index = self.units.len();
        let text: String = spans.iter().map(|s| s.text.as_ref()).collect();
        let len = text.len();
        let gap = if index == 0 { "" } else { gap };
        self.units.push(Unit {
            text,
            prefix: prefix.to_owned(),
            gap,
        });
        let mut unit = selectable(spans, index);
        if let Some(size) = size {
            unit = unit.size(Pixels(size));
        }
        if let Some(on_link) = self.on_link {
            unit = unit.on_link(on_link);
        }
        if let Some(select) = &self.select {
            unit = unit
                .selected(select.range.and_then(|range| unit_range(range, index, len)))
                .dragging(select.dragging)
                .on_gesture(select.on_gesture.clone());
        }
        unit.into()
    }
}

fn spans(
    content: &[Inline],
    flags: Flags,
    link: Option<&str>,
    theme: &Theme,
    out: &mut Vec<Span<'static, String, Font>>,
) {
    for inline in content {
        match inline {
            Inline::Text(t) => {
                for (is_emoji, run) in emoji::runs(t) {
                    let run = styled(span(run), flags, link, theme);
                    out.push(if is_emoji {
                        run.font(fonts::EMOJI)
                    } else {
                        run
                    });
                }
            }
            Inline::Styled { style, content } => {
                let mut f = flags;
                match style {
                    Style::Bold => f.bold = true,
                    Style::Italic => f.italic = true,
                    Style::Strikethrough => f.strike = true,
                    Style::Underline => f.underline = true,
                    // No good native equivalent; keep the text.
                    Style::Superscript | Style::Subscript | Style::Spoiler => {}
                }
                spans(content, f, link, theme, out);
            }
            Inline::Code(code) => {
                out.push(styled(
                    span(code.clone()),
                    Flags {
                        code: true,
                        ..flags
                    },
                    link,
                    theme,
                ));
            }
            Inline::Link { url, content } => spans(content, flags, Some(url), theme, out),
            Inline::Math(tex) => {
                out.push(styled(
                    span(format!("${tex}$")),
                    Flags {
                        code: true,
                        ..flags
                    },
                    link,
                    theme,
                ));
            }
            Inline::LineBreak => out.push(span("\n".to_owned())),
        }
    }
}

fn styled(
    s: Span<'static, String, Font>,
    flags: Flags,
    link: Option<&str>,
    theme: &Theme,
) -> Span<'static, String, Font> {
    let palette = theme.extended_palette();
    let mut s = s
        .font(if flags.code {
            Font::MONOSPACE
        } else {
            fonts::styled(flags.bold, flags.italic)
        })
        .underline(flags.underline || link.is_some())
        .strikethrough(flags.strike);
    if flags.code {
        s = s.background(palette.background.strong.color.scale_alpha(0.5));
    }
    if let Some(url) = link {
        s = s.color(palette.primary.base.color).link(url.to_owned());
    }
    s
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use iced::{Event, Size, mouse};
    use iced_test::simulator::Simulator;

    use super::*;
    use crate::selection::{Message, Selection, units_text};

    /// Drives `document`'s view with mouse `steps` (a position and an
    /// event there) and returns the selected text.
    fn select(document: &Document, dragging: bool, steps: &[((f32, f32), mouse::Event)]) -> String {
        let view = view(
            document,
            &Theme::Light,
            |_| Gesture::Drag {
                unit: usize::MAX,
                offset: 0,
            },
            Select {
                range: None,
                dragging,
                on_gesture: Rc::new(|gesture| gesture),
            },
        );
        let mut ui = Simulator::with_size(fonts::settings(), Size::new(800.0, 400.0), view);
        for (at, event) in steps {
            ui.point_at(*at);
            let _ = ui.simulate([Event::Mouse(*event)]);
        }
        let mut selection = None;
        for gesture in ui.into_messages() {
            Selection::update(&mut selection, Message::Gesture("$e".into(), gesture));
        }
        let order = HashMap::from([("$e", 0)]);
        let Some(bounds) = selection.and_then(|s| s.ordered(&order)) else {
            return String::new();
        };
        let (from, to) = bounds.in_message(0).unwrap();
        units_text(&units(document), from, to)
    }

    #[test]
    fn selects_by_dragging_and_clicking() {
        use mouse::{Button::Left, Event::*};
        let document = Document::from_html("<p>Hello you there</p><p>Second line</p>");
        let moved = |x, y| CursorMoved {
            position: iced::Point::new(x, y),
        };
        // From the start of the first paragraph to right of the second.
        let dragged = select(
            &document,
            true,
            &[
                ((2.0, 8.0), ButtonPressed(Left)),
                ((700.0, 35.0), moved(700.0, 35.0)),
            ],
        );
        assert_eq!(dragged, "Hello you there\n\nSecond line");
        // Not dragging: moving selects nothing.
        assert_eq!(
            select(
                &document,
                false,
                &[
                    ((2.0, 8.0), ButtonPressed(Left)),
                    ((700.0, 35.0), moved(700.0, 35.0))
                ],
            ),
            ""
        );
        // A double click selects the word, a triple click the paragraph.
        let click = [
            ((2.0, 8.0), ButtonPressed(Left)),
            ((2.0, 8.0), ButtonReleased(Left)),
        ];
        assert_eq!(select(&document, false, &click.repeat(2)), "Hello");
        assert_eq!(
            select(&document, false, &click.repeat(3)),
            "Hello you there"
        );
    }

    #[test]
    fn units_follow_the_blocks_as_shown() {
        let document = Document::from_html(
            "<p>Hello <b>you</b><br>there</p><blockquote><p>quoted</p></blockquote>\
             <ol><li>one</li><li><p>two</p><p>more</p></li></ol><pre><code>let x = 1;\n</code></pre>",
        );
        let units = units(&document);
        let texts: Vec<_> = units.iter().map(|u| u.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Hello you\nthere",
                "quoted",
                "one",
                "two",
                "more",
                "let x = 1;"
            ]
        );
        assert_eq!(
            units_text(&units, Pos::default(), Pos::END),
            "Hello you\nthere\n\n> quoted\n\n1. one\n2. two\n   more\n\nlet x = 1;"
        );
    }
}
