//! Editing in the composer and the message editor (`docs/native-behaviour.md`,
//! B2), done in the vendored `text_editor`: keys as the window gets them,
//! through an editor with the default bindings.

use iced::{
    Event, Theme,
    advanced::{clipboard, input_method, renderer::Headless, widget::operation::focusable},
    keyboard::{self, Key, Modifiers, key::Named},
    widget::text_editor::{Action, Binding, Content, Edit, Motion},
};
use iced_runtime::user_interface::{Cache, UserInterface};

use super::*;

const FIELD: &str = "field";

/// A focused editor with the default key bindings, its actions performed
/// on its content as the app does.
struct Field {
    content: Content,
    renderer: iced::Renderer,
    cache: Cache,
    cursor: iced::Point,
}

impl Field {
    /// `text` in the editor, the caret at its end.
    fn new(text: &str) -> Self {
        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            fonts::INTER,
            iced::Pixels(style::TEXT_SIZE),
            None,
        ))
        .unwrap();
        let mut field = Self {
            content: typed(text),
            renderer,
            cache: Cache::default(),
            cursor: iced::Point::ORIGIN,
        };
        let mut ui = ui(&field.content, &mut field.cache, &mut field.renderer);
        ui.operate(&field.renderer, &mut focusable::focus(FIELD.into()));
        field.cache = ui.into_cache();
        field
    }

    /// `events` through the editor, its actions performed.
    fn send(&mut self, events: &[Event]) {
        let cursor = mouse::Cursor::Available(self.cursor);
        let mut ui = ui(&self.content, &mut self.cache, &mut self.renderer);
        let mut actions = Vec::new();
        let _ = ui.update(
            events,
            cursor,
            &mut self.renderer,
            &mut clipboard::Null,
            &mut actions,
        );
        self.cache = ui.into_cache();
        for action in actions {
            self.content.perform(action);
        }
    }

    /// `key` pressed with `modifiers`, giving `text` (macOS gives a
    /// letter's also with ⌘, and a control character with ⌃).
    fn key(&mut self, key: impl Into<Key>, modifiers: Modifiers, text: Option<&str>) {
        let key = key.into();
        self.send(&[Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers,
            text: text.map(Into::into),
            repeat: false,
        })]);
    }

    /// A shortcut with a letter: ⌘ with it on macOS, Ctrl elsewhere.
    fn command(&mut self, letter: &str, shift: bool) {
        let modifiers = if shift {
            Modifiers::COMMAND | Modifiers::SHIFT
        } else {
            Modifiers::COMMAND
        };
        let text = shift.then(|| letter.to_uppercase());
        self.key(
            Key::Character(letter.into()),
            modifiers,
            Some(text.as_deref().unwrap_or(letter)),
        );
    }

    /// `text` typed, a line break with Shift+Enter (as in the composer).
    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            if c == '\n' {
                self.named(Named::Enter, Modifiers::SHIFT);
                continue;
            }
            let c = c.to_string();
            self.key(
                Key::Character(c.as_str().into()),
                Modifiers::empty(),
                Some(&c),
            );
        }
    }

    fn named(&mut self, key: Named, modifiers: Modifiers) {
        self.key(key, modifiers, None);
    }

    fn undo(&mut self) -> String {
        self.command("z", false);
        self.content.text()
    }

    fn redo(&mut self) -> String {
        self.command("z", true);
        self.content.text()
    }

    fn input_method(&mut self, event: input_method::Event) {
        self.send(&[Event::InputMethod(event)]);
    }
}

/// The editor, laid out.
fn ui<'a>(
    content: &'a Content,
    cache: &mut Cache,
    renderer: &mut iced::Renderer,
) -> UserInterface<'a, Action, Theme, iced::Renderer> {
    let editor = iced::widget::text_editor(content)
        .id(FIELD)
        .on_action(|action| action)
        .key_binding(Binding::from_key_press);
    UserInterface::build(
        editor,
        Size::new(400.0, 300.0),
        std::mem::take(cache),
        renderer,
    )
}

#[test]
fn typing_undoes_by_words() {
    let mut field = Field::new("");
    field.type_text("Crème brûlée au café");
    assert_eq!(field.undo(), "Crème brûlée au ");
    assert_eq!(field.undo(), "Crème brûlée ");
    assert_eq!(field.undo(), "Crème ");
    assert_eq!(field.undo(), "");
    // Nothing more to undo.
    assert_eq!(field.undo(), "");
    assert_eq!(field.redo(), "Crème ");
    assert_eq!(field.redo(), "Crème brûlée ");
    // A line break is a space too.
    let mut field = Field::new("");
    field.type_text("Melt  \nrates");
    assert_eq!(field.undo(), "Melt  \n");
    assert_eq!(field.undo(), "");
}

#[test]
fn backspace_and_delete_undo_by_words() {
    let mut field = Field::new("Calving fronts");
    for _ in 0.."fronts ".len() + 1 {
        field.named(Named::Backspace, Modifiers::empty());
    }
    assert_eq!(field.content.text(), "Calvin");
    // "g" deleted after the space that followed "fronts".
    assert_eq!(field.undo(), "Calving");
    assert_eq!(field.undo(), "Calving fronts");
    // Forward, from the start.
    let mut field = Field::new("Sea ice");
    field.named(Named::Home, Modifiers::empty());
    for _ in 0.."Sea i".len() {
        field.named(Named::Delete, Modifiers::empty());
    }
    assert_eq!(field.content.text(), "ce");
    assert_eq!(field.undo(), "ice");
    assert_eq!(field.undo(), "Sea ice");
}

#[test]
fn a_caret_move_or_switching_between_typing_and_deleting_starts_a_step() {
    let mut field = Field::new("");
    field.type_text("ab");
    field.named(Named::ArrowLeft, Modifiers::empty());
    field.type_text("c");
    assert_eq!(field.content.text(), "acb");
    assert_eq!(field.undo(), "ab");
    assert_eq!(field.undo(), "");

    let mut field = Field::new("");
    field.type_text("abc");
    field.named(Named::Backspace, Modifiers::empty());
    field.type_text("d");
    assert_eq!(field.undo(), "ab");
    assert_eq!(field.undo(), "abc");
    assert_eq!(field.undo(), "");
    // A selection deleted is a step of its own.
    let mut field = Field::new("Ice shelf");
    field.named(Named::Home, Modifiers::empty());
    for _ in 0.."Ice ".len() {
        field.named(Named::ArrowRight, Modifiers::SHIFT);
    }
    field.named(Named::Delete, Modifiers::empty());
    field.named(Named::Delete, Modifiers::empty());
    assert_eq!(field.content.text(), "helf");
    assert_eq!(field.undo(), "shelf");
    assert_eq!(field.undo(), "Ice shelf");
}

#[test]
fn a_paste_and_an_input_methods_commit_are_steps_of_their_own() {
    let mut field = Field::new("");
    field.type_text("ab");
    field
        .content
        .perform(Action::Edit(Edit::Paste(Arc::new("ΔT".into()))));
    field.type_text("cd");
    assert_eq!(field.undo(), "abΔT");
    assert_eq!(field.undo(), "ab");
    assert_eq!(field.undo(), "");

    // Holding "e" and picking "é": macOS deletes the "e", then commits;
    // one step, after the typing.
    let mut field = Field::new("");
    field.type_text("Crème");
    field.input_method(input_method::Event::DeleteSurrounding {
        before_bytes: 1,
        after_bytes: 0,
    });
    field.input_method(input_method::Event::Commit("é".into()));
    assert_eq!(field.content.text(), "Crèmé");
    assert_eq!(field.undo(), "Crème");
    assert_eq!(field.undo(), "");
    // And a commit alone (a dead key: ⌥E, E).
    field.type_text("caf");
    field.input_method(input_method::Event::Commit("é".into()));
    field.type_text("s");
    assert_eq!(field.undo(), "café");
    assert_eq!(field.undo(), "caf");
}

#[test]
fn undo_brings_back_the_selection_and_an_edit_ends_redo() {
    let mut field = Field::new("Basal melt");
    field.command("a", false);
    field.type_text("x");
    assert_eq!(field.undo(), "Basal melt");
    assert_eq!(field.content.selection().as_deref(), Some("Basal melt"));
    assert_eq!(field.redo(), "x");
    assert_eq!(field.content.selection(), None);

    let mut field = Field::new("");
    field.type_text("ab");
    assert_eq!(field.undo(), "");
    field.type_text("c");
    assert_eq!(field.redo(), "c");
}

#[test]
fn a_new_content_has_no_history_and_the_history_is_bounded() {
    let mut field = Field::new("");
    field.type_text("Draft");
    field.content = typed("Sent");
    assert_eq!(field.undo(), "Sent");

    let mut content = typed("");
    for _ in 0..1000 {
        content.perform(Action::Edit(Edit::Paste(Arc::new("x".into()))));
    }
    for _ in 0..1000 {
        content.perform(Action::Edit(Edit::Undo));
    }
    let kept = content.text().len();
    assert!(0 < kept && kept < 1000, "{kept}");
}

/// The app completes a mention by pasting over the partial one: one step.
#[test]
fn a_completed_mention_undoes_in_one_step() {
    let mut content = typed("Ask @ali");
    let query = mention::completion(&content).unwrap();
    mention::insert(&mut content, &query, "@alice:example.org");
    assert_eq!(content.text(), "Ask @alice:example.org ");
    content.perform(Action::Edit(Edit::Undo));
    assert_eq!(content.text(), "Ask @ali");
    content.perform(Action::Move(Motion::DocumentEnd));
    content.perform(Action::Edit(Edit::Undo));
    assert_eq!(content.text(), "Ask @ali");
}
