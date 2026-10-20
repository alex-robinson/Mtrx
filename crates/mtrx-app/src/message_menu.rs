//! A message's menu (right-click, or "⋯" in its toolbar): copying its text,
//! formulas and links, saving math as a picture, and selecting its text in
//! place. Selecting in the timeline itself is planned, see
//! `docs/text-selection.md`.

use iced::{
    Alignment, Element, Length,
    keyboard::{Key, key::Named},
    widget::{button, column, container, row, space, text, text_editor},
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

/// The read-only field for selecting a message's text.
const SELECT: &str = "select-text";

/// What the menu offers for a message: label and action, in order.
/// `in_selection`: the message is (partly) in the timeline's selection.
pub fn items(
    message: &Message,
    event_id: &str,
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
    if is_text {
        items.push((
            "Select text…".to_owned(),
            actions::Message::SelectText(event_id.to_owned(), document.to_plain_text()),
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

/// A message's text shown in place of it, to select parts and copy them.
pub struct Selecting {
    /// [`mtrx_core::Event::id`] of the message.
    pub id: String,
    content: text_editor::Content,
}

impl Selecting {
    /// All of `text` selected, so ⌘C copies it at once.
    pub fn new(id: String, text: &str) -> Self {
        let mut content = text_editor::Content::with_text(text);
        content.perform(text_editor::Action::SelectAll);
        Self { id, content }
    }

    /// Put the cursor into the field.
    pub fn focus<T>() -> iced::Task<T> {
        iced::widget::operation::focus(SELECT)
    }

    /// Selecting and moving; the text cannot be changed.
    pub fn perform(&mut self, action: text_editor::Action) {
        if !action.is_edit() {
            self.content.perform(action);
        }
    }

    pub fn view<'a>(&'a self) -> Element<'a, app::Message> {
        let field = text_editor(&self.content)
            .id(SELECT)
            .style(style::editor)
            .on_action(|action| act(actions::Message::SelectAction(action)))
            .key_binding(|press| match press.key {
                Key::Named(Named::Escape) => Some(text_editor::Binding::Custom(act(
                    actions::Message::CloseSelect,
                ))),
                _ => text_editor::Binding::from_key_press(press),
            })
            .padding(6)
            .max_height(400);
        column![
            field,
            row![
                text("Select, then ⌘C copies · Esc closes")
                    .size(11)
                    .style(text::secondary),
                space::horizontal(),
                button(text("Copy all").size(12))
                    .padding([2, 8])
                    .style(style::text_button)
                    .on_press(act(actions::Message::Copy(self.content.text()))),
                button(text("Done").size(12))
                    .padding([2, 10])
                    .style(style::primary_button)
                    .on_press(act(actions::Message::CloseSelect)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(4)
        .into()
    }
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
        let items = items(&text, "$1", true, false);
        assert_eq!(
            labels(&items),
            [
                "Copy text",
                "Copy LaTeX",
                "Copy link: example.org/a",
                "Copy image",
                "Save image…",
                "Select text…"
            ]
        );
        assert!(
            matches!(&items[1].1, actions::Message::Copy(tex) if tex == r"q=-k\nabla T"),
            "the formula without dollars"
        );
        // Until the math is typeset, there is no picture to save.
        assert!(!labels(&super::items(&text, "$1", false, false)).contains(&"Save image…"));

        let two = message(
            MessageKind::Text,
            r#"<span data-mx-maths="a"><code>a</code></span> and <span data-mx-maths="b"><code>b</code></span>"#,
        );
        assert_eq!(
            labels(&super::items(&two, "$2", true, false))[1..3],
            ["Copy LaTeX: a", "Copy LaTeX: b"]
        );

        // A file has nothing to copy.
        assert!(
            super::items(
                &message(MessageKind::File, "report.pdf"),
                "$3",
                false,
                false
            )
            .is_empty()
        );
    }

    #[test]
    fn selecting_cannot_change_the_text() {
        let mut selecting = Selecting::new("$1".into(), "keep me");
        selecting.perform(text_editor::Action::Edit(text_editor::Edit::Insert('x')));
        assert_eq!(selecting.content.text(), "keep me");
        assert_eq!(selecting.content.selection().as_deref(), Some("keep me"));
    }
}
