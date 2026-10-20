//! Composer text (Markdown with `$…$` math) to a Matrix message body.

use pulldown_cmark::{CowStr, Event, Options, Parser, html};

/// The two bodies of a Matrix text message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formatted {
    /// Plain-text fallback: the text as typed.
    pub body: String,
    /// `org.matrix.custom.html`; `None` when the text has no formatting.
    pub html: Option<String>,
}

/// Convert composer text to Matrix bodies, like Element does: Markdown, single
/// newlines kept as line breaks, and `$…$` / `$$…$$` math sent as
/// `data-mx-maths` elements with the LaTeX as code fallback.
pub fn markdown_to_matrix(text: &str) -> Formatted {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_MATH;
    let events = Parser::new_ext(text, options).map(|event| match event {
        Event::SoftBreak => Event::HardBreak,
        Event::InlineMath(tex) => Event::InlineHtml(CowStr::from(format!(
            r#"<span data-mx-maths="{0}"><code>{1}</code></span>"#,
            escape(&tex, true),
            escape(&tex, false)
        ))),
        Event::DisplayMath(tex) => Event::InlineHtml(CowStr::from(format!(
            r#"<div data-mx-maths="{0}"><code>{1}</code></div>"#,
            escape(&tex, true),
            escape(&tex, false)
        ))),
        // Raw HTML typed by the user is shown as text, not interpreted.
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
        other => other,
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    let out = out.trim_end_matches('\n');

    // A single plain paragraph needs no HTML body.
    let plain = out
        .strip_prefix("<p>")
        .and_then(|s| s.strip_suffix("</p>"))
        .is_some_and(|inner| !inner.contains('<') && unescape(inner) == text.trim());
    Formatted {
        body: text.to_owned(),
        html: (!plain).then(|| out.to_owned()),
    }
}

fn escape(text: &str, attribute: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}
