//! Native rendering of a message [`Document`] with iced rich text. Messages
//! with math use a typeset image instead (see [`crate::math`]); this view is
//! the fallback while that image is not ready, showing math as LaTeX code.

use iced::{
    Element, Font, Length, Theme,
    widget::{column, container, rich_text, row, rule, span, text, text::Span},
};
use mtrx_format::{Block, Document, Inline, Style};

use crate::{emoji, fonts, style};

#[derive(Clone, Copy, Default)]
struct Flags {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    code: bool,
}

pub fn view<'a, M: Clone + 'a>(
    document: &Document,
    theme: &Theme,
    on_link: fn(String) -> M,
) -> Element<'a, M> {
    blocks(&document.blocks, theme, on_link)
}

fn blocks<'a, M: Clone + 'a>(
    list: &[Block],
    theme: &Theme,
    on_link: fn(String) -> M,
) -> Element<'a, M> {
    column(list.iter().map(|b| block(b, theme, on_link)))
        .spacing(6)
        .into()
}

fn block<'a, M: Clone + 'a>(
    block: &Block,
    theme: &Theme,
    on_link: fn(String) -> M,
) -> Element<'a, M> {
    match block {
        Block::Paragraph(content) => paragraph(content, theme, on_link, None),
        Block::Heading { content, .. } => paragraph(content, theme, on_link, Some(16.0)),
        Block::Quote(inner) => row![
            container(text(""))
                .width(3)
                .height(Length::Fill)
                .style(style::quote_bar),
            blocks(inner, theme, on_link),
        ]
        .spacing(8)
        .height(Length::Shrink)
        .into(),
        Block::List { start, items } => column(items.iter().enumerate().map(|(i, item)| {
            let marker = match start {
                Some(start) => format!("{}.", *start + i as u64),
                None => "•".to_owned(),
            };
            row![text(marker).width(20), blocks(item, theme, on_link)].into()
        }))
        .spacing(2)
        .into(),
        Block::CodeBlock { code, .. } => container(
            text(code.trim_end().to_owned())
                .font(Font::MONOSPACE)
                .size(13),
        )
        .padding(8)
        .width(Length::Fill)
        .style(style::code_block)
        .into(),
        Block::Math(tex) => text(format!("$${tex}$$"))
            .font(Font::MONOSPACE)
            .size(13)
            .into(),
        Block::Rule => rule::horizontal(1).into(),
    }
}

fn paragraph<'a, M: Clone + 'a>(
    content: &[Inline],
    theme: &Theme,
    on_link: fn(String) -> M,
    size: Option<f32>,
) -> Element<'a, M> {
    let mut out = Vec::new();
    let flags = Flags {
        bold: size.is_some(),
        ..Flags::default()
    };
    spans(content, flags, None, theme, &mut out);
    let mut rich = rich_text(out).on_link_click(on_link);
    if let Some(size) = size {
        rich = rich.size(size);
    }
    rich.into()
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
