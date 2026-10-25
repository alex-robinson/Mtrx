//! Copying a document, or the selected part of one, as HTML for the
//! clipboard: what other apps (Mail, a word processor) keep of its
//! formatting, links and lists. Math stays `$…$`, as in its plain text.

use std::ops::Range;

use crate::{
    document::{Block, Document, Inline, Style},
    markdown::escape,
};

impl Document {
    /// The part of the document selected by `keep`, which gets each leaf's
    /// index and gives the bytes of it kept, if any. Leaves are, in reading
    /// order, each text, inline code, mention and formula, and each code
    /// block and displayed formula, as shown: code blocks without trailing
    /// whitespace, formulas as `$…$` and `$$…$$`, mentions by name. Part of
    /// a formula or mention is kept as text. Blocks and styles left empty
    /// go, and rules with them.
    pub fn clipped(&self, keep: impl FnMut(usize) -> Option<Range<usize>>) -> Self {
        let mut clip = Clip { keep, leaf: 0 };
        Self {
            blocks: clip.blocks(&self.blocks),
        }
    }

    /// HTML for the clipboard: math as `$…$` and `$$…$$` text, mentions as
    /// `matrix.to` links.
    pub fn to_html(&self) -> String {
        let mut out = String::new();
        blocks_html(&self.blocks, &mut out);
        out
    }
}

struct Clip<F> {
    keep: F,
    /// The index of the next leaf.
    leaf: usize,
}

impl<F: FnMut(usize) -> Option<Range<usize>>> Clip<F> {
    /// What is kept of the next leaf, shown as `shown`, and whether that is
    /// all of it.
    fn leaf<'s>(&mut self, shown: &'s str) -> Option<(&'s str, bool)> {
        let index = self.leaf;
        self.leaf += 1;
        let range = (self.keep)(index)?;
        let whole = range.start == 0 && range.end >= shown.len();
        let kept = shown.get(range.start..range.end.min(shown.len()))?;
        (!kept.is_empty()).then_some((kept, whole))
    }

    fn blocks(&mut self, blocks: &[Block]) -> Vec<Block> {
        blocks
            .iter()
            .filter_map(|block| self.block(block))
            .collect()
    }

    fn block(&mut self, block: &Block) -> Option<Block> {
        match block {
            Block::Paragraph(content) => self.paragraph(content).map(Block::Paragraph),
            Block::Heading { level, content } => {
                self.paragraph(content).map(|content| Block::Heading {
                    level: *level,
                    content,
                })
            }
            Block::Quote(inner) => {
                let inner = self.blocks(inner);
                (!inner.is_empty()).then_some(Block::Quote(inner))
            }
            Block::List { start, items } => {
                // Numbered from the first item kept.
                let mut skipped = 0;
                let mut kept = Vec::new();
                for item in items {
                    let item = self.blocks(item);
                    if !item.is_empty() {
                        kept.push(item);
                    } else if kept.is_empty() {
                        skipped += 1;
                    }
                }
                (!kept.is_empty()).then(|| Block::List {
                    start: start.map(|start| start + skipped),
                    items: kept,
                })
            }
            Block::CodeBlock { language, code } => {
                let (kept, _) = self.leaf(code.trim_end())?;
                Some(Block::CodeBlock {
                    language: language.clone(),
                    code: kept.to_owned(),
                })
            }
            Block::Math(tex) => {
                let shown = format!("$${tex}$$");
                let (kept, whole) = self.leaf(&shown)?;
                Some(if whole {
                    Block::Math(tex.clone())
                } else {
                    Block::Paragraph(vec![Inline::Text(kept.to_owned())])
                })
            }
            Block::Rule => None,
        }
    }

    /// The inlines kept; line breaks at their edges go with what is not.
    fn paragraph(&mut self, content: &[Inline]) -> Option<Vec<Inline>> {
        let mut kept = self.inlines(content);
        let start = kept
            .iter()
            .position(|inline| *inline != Inline::LineBreak)?;
        kept.drain(..start);
        while kept.last() == Some(&Inline::LineBreak) {
            kept.pop();
        }
        Some(kept)
    }

    fn inlines(&mut self, content: &[Inline]) -> Vec<Inline> {
        let mut out = Vec::new();
        for inline in content {
            let kept = match inline {
                Inline::Text(text) => self
                    .leaf(text)
                    .map(|(kept, _)| Inline::Text(kept.to_owned())),
                Inline::Code(code) => self
                    .leaf(code)
                    .map(|(kept, _)| Inline::Code(kept.to_owned())),
                Inline::Mention { name, .. } => self.leaf(name).map(|(kept, whole)| {
                    if whole {
                        inline.clone()
                    } else {
                        Inline::Text(kept.to_owned())
                    }
                }),
                Inline::Math(tex) => {
                    let shown = format!("${tex}$");
                    self.leaf(&shown).map(|(kept, whole)| {
                        if whole {
                            inline.clone()
                        } else {
                            Inline::Text(kept.to_owned())
                        }
                    })
                }
                Inline::Styled { style, content } => {
                    let content = self.inlines(content);
                    has_text(&content).then_some(Inline::Styled {
                        style: *style,
                        content,
                    })
                }
                Inline::Link { url, content } => {
                    let content = self.inlines(content);
                    has_text(&content).then(|| Inline::Link {
                        url: url.clone(),
                        content,
                    })
                }
                Inline::LineBreak => Some(Inline::LineBreak),
            };
            out.extend(kept);
        }
        out
    }
}

fn has_text(inlines: &[Inline]) -> bool {
    inlines.iter().any(|inline| *inline != Inline::LineBreak)
}

fn blocks_html(blocks: &[Block], out: &mut String) {
    for block in blocks {
        match block {
            Block::Paragraph(content) => {
                out.push_str("<p>");
                inlines_html(content, out);
                out.push_str("</p>");
            }
            Block::Heading { level, content } => {
                out.push_str(&format!("<h{level}>"));
                inlines_html(content, out);
                out.push_str(&format!("</h{level}>"));
            }
            Block::Quote(inner) => {
                out.push_str("<blockquote>");
                blocks_html(inner, out);
                out.push_str("</blockquote>");
            }
            Block::List { start, items } => {
                let tag = if start.is_some() { "ol" } else { "ul" };
                match start {
                    Some(start) if *start != 1 => out.push_str(&format!("<ol start=\"{start}\">")),
                    _ => out.push_str(&format!("<{tag}>")),
                }
                for item in items {
                    out.push_str("<li>");
                    // A one-paragraph item, as most are, without the <p>.
                    match &item[..] {
                        [Block::Paragraph(content)] => inlines_html(content, out),
                        blocks => blocks_html(blocks, out),
                    }
                    out.push_str("</li>");
                }
                out.push_str(&format!("</{tag}>"));
            }
            Block::CodeBlock { language, code } => {
                match language {
                    Some(language) => out.push_str(&format!(
                        "<pre><code class=\"language-{}\">",
                        escape(language, true)
                    )),
                    None => out.push_str("<pre><code>"),
                }
                out.push_str(&escape(code, false));
                out.push_str("</code></pre>");
            }
            Block::Math(tex) => {
                out.push_str("<p>$$");
                out.push_str(&escape(tex, false));
                out.push_str("$$</p>");
            }
            Block::Rule => out.push_str("<hr>"),
        }
    }
}

fn inlines_html(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(&escape(text, false)),
            Inline::Styled { style, content } => {
                let tag = match style {
                    Style::Bold => "strong",
                    Style::Italic => "em",
                    Style::Strikethrough => "del",
                    Style::Underline => "u",
                    Style::Superscript => "sup",
                    Style::Subscript => "sub",
                    // Copied, it is no secret.
                    Style::Spoiler => {
                        inlines_html(content, out);
                        continue;
                    }
                };
                out.push_str(&format!("<{tag}>"));
                inlines_html(content, out);
                out.push_str(&format!("</{tag}>"));
            }
            Inline::Code(code) => {
                out.push_str("<code>");
                out.push_str(&escape(code, false));
                out.push_str("</code>");
            }
            Inline::Link { url, content } => {
                out.push_str(&format!("<a href=\"{}\">", escape(url, true)));
                inlines_html(content, out);
                out.push_str("</a>");
            }
            Inline::Math(tex) => {
                out.push('$');
                out.push_str(&escape(tex, false));
                out.push('$');
            }
            Inline::Mention { user_id, name } => out.push_str(&format!(
                "<a href=\"https://matrix.to/#/{}\">{}</a>",
                escape(user_id, true),
                escape(name, false)
            )),
            Inline::LineBreak => out.push_str("<br>"),
        }
    }
}
