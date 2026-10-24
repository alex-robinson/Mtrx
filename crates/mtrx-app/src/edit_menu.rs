//! The menu of a field one writes in (right-click), the composer or a
//! message being edited: Cut, Copy, Paste, Select All and, on macOS,
//! Emoji & Symbols.

use iced::Element;

use crate::{
    app,
    clipboard::Field,
    context_menu::{self, context_menu},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Open,
    Close,
    Cut,
    Copy,
    Paste,
    SelectAll,
    /// The system's window of emoji and symbols; what is picked there goes
    /// into the field.
    #[cfg(target_os = "macos")]
    EmojiAndSymbols,
}

/// What the menu offers: Cut and Copy with text `selected`.
pub fn items(selected: bool) -> Vec<(&'static str, Message)> {
    let mut items = Vec::new();
    if selected {
        items.extend([("Cut", Message::Cut), ("Copy", Message::Copy)]);
    }
    items.extend([
        ("Paste", Message::Paste),
        ("Select All", Message::SelectAll),
    ]);
    #[cfg(target_os = "macos")]
    items.push(("Emoji & Symbols", Message::EmojiAndSymbols));
    items
}

/// `editor`, which is `field`, with its menu on right-click: shown while
/// `open`, Cut and Copy with text `selected`.
pub fn with_menu<'a>(
    field: Field,
    editor: impl Into<Element<'a, app::Message>>,
    open: bool,
    selected: bool,
) -> Element<'a, app::Message> {
    let menu = open.then(|| {
        context_menu::menu(
            items(selected)
                .into_iter()
                .map(|(label, message)| (label.to_owned(), app::Message::EditMenu(field, message))),
        )
    });
    context_menu(editor, menu, app::Message::EditMenu(field, Message::Open))
        .on_dismiss(app::Message::EditMenu(field, Message::Close))
        .into()
}

/// Open the system's Emoji & Symbols window, which types what is picked
/// into the focused field.
#[cfg(target_os = "macos")]
pub fn emoji_and_symbols() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    // AppKit only on the main thread, where iced updates the app (not in
    // tests).
    if let Some(main_thread) = MainThreadMarker::new() {
        NSApplication::sharedApplication(main_thread).orderFrontCharacterPalette(None);
    }
}

#[cfg(test)]
mod tests {
    use iced::{
        Event, Size, mouse,
        widget::text_editor::{self, Content},
    };
    use iced_test::simulator::Simulator;

    use super::*;

    fn labels(selected: bool) -> Vec<&'static str> {
        items(selected)
            .into_iter()
            .map(|(label, _)| label)
            .collect()
    }

    #[test]
    fn cut_and_copy_need_a_selection() {
        let mut expected = vec!["Cut", "Copy", "Paste", "Select All"];
        if cfg!(target_os = "macos") {
            expected.push("Emoji & Symbols");
        }
        assert_eq!(labels(true), expected);
        assert_eq!(labels(false), expected[2..]);
    }

    /// What an editor wrapped as a message being edited is, its menu `open`
    /// (at its corner, as nothing was right-clicked yet), sends for
    /// `button` clicked at `at`.
    fn sends(open: bool, button: mouse::Button, at: (f32, f32)) -> Vec<app::Message> {
        let content = Content::<iced::Renderer>::with_text("Melt");
        let editor = iced::widget::text_editor(&content)
            .on_action(app::Message::Composer)
            .key_binding(text_editor::Binding::from_key_press);
        let field = with_menu(Field::Edit, editor, open, true);
        let mut ui = Simulator::with_size(
            crate::fonts::settings(),
            Size::new(400.0, 300.0),
            iced::widget::column![field],
        );
        ui.point_at(at);
        let _ = ui.simulate([
            Event::Mouse(mouse::Event::ButtonPressed(button)),
            Event::Mouse(mouse::Event::ButtonReleased(button)),
        ]);
        ui.into_messages()
            .filter(|message| !matches!(message, app::Message::Composer(_)))
            .collect()
    }

    #[test]
    fn right_click_opens_it_and_an_item_is_chosen() {
        use app::Message::EditMenu as Menu;
        use mouse::Button::{Left, Right};
        let sent = sends(false, Right, (20.0, 10.0));
        assert!(
            matches!(sent[..], [Menu(Field::Edit, Message::Open)]),
            "{sent:?}"
        );
        // The third entry, Paste.
        let sent = sends(true, Left, (60.0, 4.0 + 2.5 * 26.0));
        assert!(
            matches!(sent[..], [Menu(Field::Edit, Message::Paste)]),
            "{sent:?}"
        );
        // A click beside it closes it.
        let sent = sends(true, Left, (380.0, 280.0));
        assert!(
            matches!(sent[..], [Menu(Field::Edit, Message::Close)]),
            "{sent:?}"
        );
    }
}
