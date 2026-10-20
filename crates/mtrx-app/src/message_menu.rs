//! A message's menu (right-click, or "⋯" in its toolbar): copying the
//! selection, its text, formulas and links, and math as a picture.

use iced::{
    Element, Length,
    widget::{button, column, container},
};
use mtrx_core::{Message, MessageKind};

use crate::{
    actions::{self, act},
    app,
    ellipsis::ellipsis,
    style,
};

/// Where a message's menu floats from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAt {
    /// Where the message was right-clicked.
    Cursor,
    /// The "⋯" of the hover toolbar.
    Toolbar,
}

/// Width of the menu; long items are cut short.
const WIDTH: f32 = 280.0;

/// What the menu offers for a message: label and action, in order.
/// `in_selection`: the message is (partly) in the timeline's selection.
pub fn items(
    message: &Message,
    math_shown: bool,
    in_selection: bool,
) -> Vec<(String, actions::Message)> {
    let document = &message.document;
    let is_text = matches!(
        message.kind,
        MessageKind::Text | MessageKind::Notice | MessageKind::Emote
    );
    let mut items = Vec::new();
    if in_selection {
        items.push(("Copy selection".to_owned(), actions::Message::CopySelection));
    }
    if is_text {
        items.push((
            "Copy text".to_owned(),
            actions::Message::Copy(document.to_plain_text()),
        ));
    }
    let formulas = document.formulas();
    for tex in &formulas {
        let label = match formulas.len() {
            1 => "Copy LaTeX".to_owned(),
            _ => format!("Copy LaTeX: {tex}"),
        };
        items.push((label, actions::Message::Copy((*tex).to_owned())));
    }
    for link in document.links() {
        let shown = link
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        items.push((
            format!("Copy link: {shown}"),
            actions::Message::Copy(link.to_owned()),
        ));
    }
    if math_shown {
        items.push((
            "Copy image".to_owned(),
            actions::Message::CopyImage(document.clone()),
        ));
        items.push((
            "Save image…".to_owned(),
            actions::Message::SaveImage(document.clone()),
        ));
    }
    items
}

/// The menu showing `items`.
pub fn view<'a>(items: Vec<(String, actions::Message)>) -> Element<'a, app::Message> {
    let entries = items.into_iter().map(|(label, message)| {
        button(ellipsis(label).size(13))
            .width(Length::Fill)
            .padding([4, 10])
            .style(style::list_item(false))
            .on_press(act(message))
            .into()
    });
    container(column(entries).spacing(1))
        .width(WIDTH)
        .padding(4)
        .style(style::toolbar)
        .into()
}

#[cfg(test)]
mod tests {
    use mtrx_format::Document;

    use super::*;

    fn message(kind: MessageKind, html: &str) -> Message {
        Message {
            kind,
            body: String::new(),
            document: Document::from_html(html),
            edited: false,
            in_reply_to: None,
            reactions: Vec::new(),
            media: None,
        }
    }

    fn labels(items: &[(String, actions::Message)]) -> Vec<&str> {
        items.iter().map(|(label, _)| label.as_str()).collect()
    }

    #[test]
    fn offers_text_formulas_links_and_the_picture() {
        let html = r#"Flux <span data-mx-maths="q=-k\nabla T"><code>q</code></span>, see <a href="https://example.org/a">data</a>"#;
        let text = message(MessageKind::Text, html);
        let items = items(&text, true, false);
        assert_eq!(
            labels(&items),
            [
                "Copy text",
                "Copy LaTeX",
                "Copy link: example.org/a",
                "Copy image",
                "Save image…",
            ]
        );
        assert!(
            matches!(&items[1].1, actions::Message::Copy(tex) if tex == r"q=-k\nabla T"),
            "the formula without dollars"
        );
        // Until the math is typeset, there is no picture to save.
        assert!(!labels(&super::items(&text, false, false)).contains(&"Save image…"));

        let two = message(
            MessageKind::Text,
            r#"<span data-mx-maths="a"><code>a</code></span> and <span data-mx-maths="b"><code>b</code></span>"#,
        );
        assert_eq!(
            labels(&super::items(&two, true, false))[1..3],
            ["Copy LaTeX: a", "Copy LaTeX: b"]
        );

        // A file has nothing to copy.
        assert!(super::items(&message(MessageKind::File, "report.pdf"), false, false).is_empty());
    }
}
