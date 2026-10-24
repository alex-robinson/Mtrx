//! The composer's menu (right-click): Cut, Copy, Paste, Select All and,
//! on macOS, Emoji & Symbols.

use iced::Element;

use crate::{app, context_menu};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Open,
    Close,
    Cut,
    Copy,
    Paste,
    SelectAll,
    /// The system's window of emoji and symbols; what is picked there goes
    /// into the composer.
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

/// The menu, with text `selected` or not.
pub fn view<'a>(selected: bool) -> Element<'a, app::Message> {
    context_menu::menu(
        items(selected)
            .into_iter()
            .map(|(label, message)| (label.to_owned(), app::Message::ComposerMenu(message))),
    )
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
    use crate::context_menu::context_menu;

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

    /// What an editor wrapped as the composer is, its menu `open` (at its
    /// corner, as nothing was right-clicked yet), sends for `button`
    /// clicked at `at`.
    fn sends(open: bool, button: mouse::Button, at: (f32, f32)) -> Vec<app::Message> {
        let content = Content::<iced::Renderer>::with_text("Melt");
        let editor = iced::widget::text_editor(&content)
            .on_action(app::Message::Composer)
            .key_binding(text_editor::Binding::from_key_press);
        let composer = context_menu(
            editor,
            open.then(|| view(true)),
            app::Message::ComposerMenu(Message::Open),
        )
        .on_dismiss(app::Message::ComposerMenu(Message::Close));
        let mut ui = Simulator::with_size(
            crate::fonts::settings(),
            Size::new(400.0, 300.0),
            iced::widget::column![composer],
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
        use app::Message::ComposerMenu as Menu;
        use mouse::Button::{Left, Right};
        let sent = sends(false, Right, (20.0, 10.0));
        assert!(matches!(sent[..], [Menu(Message::Open)]), "{sent:?}");
        // The third entry, Paste.
        let sent = sends(true, Left, (60.0, 4.0 + 2.5 * 26.0));
        assert!(matches!(sent[..], [Menu(Message::Paste)]), "{sent:?}");
        // A click beside it closes it.
        let sent = sends(true, Left, (380.0, 280.0));
        assert!(matches!(sent[..], [Menu(Message::Close)]), "{sent:?}");
    }
}
