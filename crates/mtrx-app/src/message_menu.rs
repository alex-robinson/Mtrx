//! A message's menu (right-click, or "⋯" in its toolbar): copying the
//! selection or the message, math as a picture, and a file or image.

use iced::Element;
use mtrx_core::{MediaKind, Message, MessageKind};

use crate::{
    actions::{self, act},
    app, context_menu, message,
    selection::Pos,
};

/// Where a message's menu floats from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAt {
    /// Where the message was right-clicked.
    Cursor,
    /// The "⋯" of the hover toolbar.
    Toolbar,
}

/// What the menu offers for a message: label and action, in order.
/// `in_selection`: the message is (partly) in the timeline's selection,
/// which Copy then copies; else Copy copies the message, as a selection of
/// all of it would.
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
        items.push(("Copy".to_owned(), actions::Message::CopySelection));
    } else if is_text {
        let all = message::copied(document, Pos::default(), Pos::END);
        items.push(("Copy".to_owned(), actions::Message::Copy(all)));
    }
    if let Some(media) = &message.media {
        if media.kind == MediaKind::Image {
            items.push((
                "Copy image".to_owned(),
                actions::Message::CopyMediaImage(media.clone()),
            ));
        }
        let save = match media.kind {
            MediaKind::Image => "Save image…",
            _ => "Save…",
        };
        items.push((save.to_owned(), actions::Message::SaveMedia(media.clone())));
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
    context_menu::menu(
        items
            .into_iter()
            .map(|(label, message)| (label, act(message))),
    )
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

    fn media(filename: &str, kind: MediaKind) -> mtrx_core::Media {
        mtrx_core::Media {
            kind,
            filename: filename.into(),
            size: None,
            mimetype: None,
            width: None,
            height: None,
            source: mtrx_core::MediaRef::plain("mxc://example.org/a"),
        }
    }

    fn labels(items: &[(String, actions::Message)]) -> Vec<&str> {
        items.iter().map(|(label, _)| label.as_str()).collect()
    }

    #[test]
    fn offers_copy_and_the_picture() {
        let html = r#"Flux <span data-mx-maths="q=-k\nabla T"><code>q</code></span>, see <a href="https://example.org/a">data</a>"#;
        let text = message(MessageKind::Text, html);
        let items = items(&text, true, false);
        assert_eq!(labels(&items), ["Copy", "Copy image", "Save image…"]);
        assert!(
            matches!(&items[0].1, actions::Message::Copy(all) if all.text == r"Flux $q=-k\nabla T$, see data"),
            "as shown, math as LaTeX"
        );
        assert!(
            matches!(&items[0].1, actions::Message::Copy(all) if all.html == r#"<p>Flux $q=-k\nabla T$, see <a href="https://example.org/a">data</a></p>"#),
            "and as HTML, with the link"
        );
        // Until the math is typeset, there is no picture.
        assert_eq!(labels(&super::items(&text, false, false)), ["Copy"]);
        // In the selection, Copy copies the selection.
        assert!(matches!(
            super::items(&text, false, true)[0].1,
            actions::Message::CopySelection
        ));
        // A file can be saved; an image also copied.
        let mut file = message(MessageKind::File, "report.pdf");
        file.media = Some(media("report.pdf", MediaKind::File));
        assert_eq!(labels(&super::items(&file, false, false)), ["Save…"]);
        file.media = Some(media("ice.png", MediaKind::Image));
        assert_eq!(
            labels(&super::items(&file, false, false)),
            ["Copy image", "Save image…"]
        );
    }
}
