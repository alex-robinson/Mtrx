//! Keys as the window gets them: through the widgets first, then, unless
//! one took them, to the app's shortcuts; the tasks they start run on the
//! window's widgets.

use iced::{
    Event,
    advanced::{clipboard, renderer::Headless, widget::operation::Outcome},
    event::Status,
    keyboard::{self, Key, Modifiers, key::Named},
};
use iced_runtime::{
    Action,
    user_interface::{Cache, UserInterface},
};

use super::*;

const SIZE: Size = Size::new(1200.0, 800.0);

/// The app in its window.
struct Ui {
    app: App,
    renderer: iced::Renderer,
    cache: Cache,
    cursor: iced::Point,
}

impl Ui {
    fn new(app: App) -> Self {
        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            fonts::INTER,
            iced::Pixels(style::TEXT_SIZE),
            None,
        ))
        .unwrap();
        let mut ui = Self {
            app,
            renderer,
            cache: Cache::default(),
            cursor: iced::Point::ORIGIN,
        };
        // Laid out once, as when shown.
        ui.send(&[]);
        ui
    }

    fn main(&mut self) -> &mut Main {
        let Screen::Main(main) = &mut self.app.screen else {
            panic!("not the main screen")
        };
        main
    }

    fn room(&mut self) -> &mut Room {
        self.main().room.as_mut().expect("a room is open")
    }

    /// `events` through the widgets, then to the shortcuts with whether a
    /// widget took them; returns that.
    fn send(&mut self, events: &[Event]) -> Vec<Status> {
        let (statuses, messages) = self.messages(events);
        for message in messages {
            self.update(message);
        }
        statuses
    }

    /// What [`Ui::send`] does without updating the app: whether a widget
    /// took each event, and the messages.
    fn messages(&mut self, events: &[Event]) -> (Vec<Status>, Vec<Message>) {
        let mut ui = UserInterface::build(
            self.app.view(),
            SIZE,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut messages = Vec::new();
        let (_, statuses) = ui.update(
            events,
            mouse::Cursor::Available(self.cursor),
            &mut self.renderer,
            &mut clipboard::Null,
            &mut messages,
        );
        self.cache = ui.into_cache();
        for (event, status) in events.iter().zip(&statuses) {
            messages.extend(shortcuts(event.clone(), *status, window::Id::unique()));
        }
        (statuses, messages)
    }

    /// A key pressed, as [`Ui::send`].
    fn press(&mut self, key: impl Into<Key>, modifiers: Modifiers) -> Status {
        self.send(&[press(key.into(), modifiers)])[0]
    }

    fn update(&mut self, message: Message) {
        let task = self.app.update(message);
        self.run(task);
    }

    /// Run `task` to its end: its messages to the app, its operations on
    /// the widgets.
    fn run(&mut self, task: Task<Message>) {
        let Some(mut actions) = iced_runtime::task::into_stream(task) else {
            return;
        };
        while let Some(action) = iced::futures::executor::block_on(actions.next()) {
            match action {
                Action::Output(message) => self.update(message),
                Action::Widget(operation) => self.operate(operation),
                _ => {}
            }
        }
    }

    fn operate(&mut self, operation: Box<dyn iced::advanced::widget::Operation>) {
        let mut ui = UserInterface::build(
            self.app.view(),
            SIZE,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut next = Some(operation);
        while let Some(mut operation) = next.take() {
            ui.operate(&self.renderer, operation.as_mut());
            if let Outcome::Chain(chained) = operation.finish() {
                next = Some(chained);
            }
        }
        self.cache = ui.into_cache();
    }
}

/// `key` pressed with `modifiers`, with its character as text if it has one
/// (macOS gives it also with ⌘).
fn press(key: Key, modifiers: Modifiers) -> Event {
    let text = match key.as_ref() {
        Key::Character(c) => Some(c.into()),
        _ => None,
    };
    Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text,
        repeat: false,
    })
}

/// A room of ours with `messages`, open in the app.
async fn room_with(prefix: &str, messages: &[&str]) -> (Ui, TempDir) {
    let dir = TempDir::new().unwrap();
    let (_, alice) = user(prefix, &dir).await;
    let room = alice
        .create_room(NewRoom {
            name: Some(prefix.into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let timeline = Arc::new(alice.timeline(&room).await.unwrap());
    for text in messages {
        send(&timeline, text).await;
    }
    while !timeline.paginate_back(50).await.unwrap() {}
    let mut main = Main::new(Arc::new(alice));
    main.verified = Some(true);
    let mut open = Room::new(room, timeline.clone());
    open.entries = timeline.current();
    with_start(&mut open.entries);
    main.room = Some(open);
    (Ui::new(app(Screen::Main(Box::new(main)), Mode::Light)), dir)
}

/// [`Event::id`] of the message with `body`.
fn row_of(room: &Room, body: &str) -> String {
    room.entries
        .iter()
        .find_map(|entry| match entry {
            TimelineEntry::Event(event)
                if matches!(&event.content, EventContent::Message(m) if m.body == body) =>
            {
                Some(event.id.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no message {body:?}"))
}

/// The shortcuts give `expected` for each key with its modifiers when no
/// widget took it, and nothing when one did.
fn assert_shortcuts(cases: Vec<(Key, Modifiers, Message)>) {
    for (key, modifiers, expected) in cases {
        let message = shortcuts(
            press(key.clone(), modifiers),
            Status::Ignored,
            window::Id::unique(),
        );
        assert_eq!(
            format!("{message:?}"),
            format!("{:?}", Some(expected)),
            "{key:?} with {modifiers:?}"
        );
        let taken = shortcuts(
            press(key.clone(), modifiers),
            Status::Captured,
            window::Id::unique(),
        );
        assert!(taken.is_none(), "{key:?} with {modifiers:?}");
    }
}

#[test]
fn shift_and_arrows_move_the_end_of_the_selection() {
    use selection::{Message::*, Step};

    let (shift, command) = (Modifiers::SHIFT, Modifiers::SHIFT | Modifiers::COMMAND);
    let arrow = Key::Named;
    let cases = [
        (arrow(Named::ArrowLeft), shift, Extend(Step::Back)),
        (
            arrow(Named::ArrowRight),
            shift | Modifiers::ALT,
            Extend(Step::WordForward),
        ),
        (arrow(Named::ArrowUp), shift, ExtendLine(true)),
        (arrow(Named::ArrowLeft), command, ExtendToLineEdge(false)),
        (arrow(Named::ArrowRight), command, ExtendToLineEdge(true)),
        (arrow(Named::ArrowUp), command, ExtendToScreenEdge(true)),
        (arrow(Named::ArrowDown), command, ExtendToScreenEdge(false)),
    ];
    assert_shortcuts(
        cases
            .into_iter()
            .map(|(key, modifiers, message)| (key, modifiers, Message::Select(message)))
            .collect(),
    );
}

/// ⇧⌘← and ⇧⌘→ move the end of the selection to the edges of its line,
/// ⇧⌘↑ and ⇧⌘↓ to the first and last message on screen.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn shift_command_arrows_extend_to_the_edges() {
    let (mut ui, _dir) = room_with("edges", &["Calving fronts", "Basal melt", "Sea ice"]).await;
    let room = ui.room();
    let (first, middle, last) = (
        row_of(room, "Calving fronts"),
        row_of(room, "Basal melt"),
        row_of(room, "Sea ice"),
    );
    let at = |event: &str, offset| Point {
        event: event.to_owned(),
        pos: Pos { unit: 0, offset },
    };
    room.selection = Some(Selection {
        anchor: at(&middle, 2),
        focus: at(&middle, 4),
        dragging: false,
    });
    ui.send(&[]);
    let focus = |ui: &mut Ui| ui.room().selection.as_ref().unwrap().focus.clone();

    ui.press(Named::ArrowRight, Modifiers::SHIFT | Modifiers::COMMAND);
    assert_eq!(focus(&mut ui), at(&middle, "Basal melt".len()));
    ui.press(Named::ArrowLeft, Modifiers::SHIFT | Modifiers::COMMAND);
    assert_eq!(focus(&mut ui), at(&middle, 0));
    ui.press(Named::ArrowUp, Modifiers::SHIFT | Modifiers::COMMAND);
    assert_eq!(focus(&mut ui), at(&first, 0));
    ui.press(Named::ArrowDown, Modifiers::SHIFT | Modifiers::COMMAND);
    assert_eq!(focus(&mut ui), at(&last, "Sea ice".len()));
    // The anchor stays.
    assert_eq!(ui.room().selection.as_ref().unwrap().anchor, at(&middle, 2));
}

#[test]
fn page_keys_and_arrows_scroll_the_timeline() {
    use room::Scroll;

    let key = Key::Named;
    let cases = [
        (key(Named::PageUp), Modifiers::empty(), Scroll::PageUp),
        (key(Named::PageDown), Modifiers::empty(), Scroll::PageDown),
        (key(Named::ArrowUp), Modifiers::ALT, Scroll::PageUp),
        (key(Named::ArrowDown), Modifiers::ALT, Scroll::PageDown),
        (key(Named::ArrowUp), Modifiers::COMMAND, Scroll::Oldest),
        (key(Named::ArrowDown), Modifiers::COMMAND, Scroll::Newest),
    ];
    assert_shortcuts(
        cases
            .into_iter()
            .map(|(key, modifiers, scroll)| (key, modifiers, Message::ScrollKey(scroll)))
            .collect(),
    );
}

/// The timeline's view height, content height and how far it is scrolled
/// from the top, once operated.
#[derive(Default)]
struct Timeline(Option<(f32, f32, f32)>);

impl iced::advanced::widget::Operation for Timeline {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: iced::Rectangle,
        content_bounds: iced::Rectangle,
        translation: iced::Vector,
        _state: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        if id == Some(&iced::advanced::widget::Id::new(room::TIMELINE)) {
            self.0 = Some((bounds.height, content_bounds.height, translation.y));
        }
    }
}

impl Ui {
    /// The timeline's view height, content height and how far it is
    /// scrolled from the top.
    fn timeline(&mut self) -> (f32, f32, f32) {
        let mut timeline = Timeline::default();
        let mut ui = UserInterface::build(
            self.app.view(),
            SIZE,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        ui.operate(&self.renderer, &mut timeline);
        self.cache = ui.into_cache();
        timeline.0.expect("a timeline")
    }

    fn scrolled(&mut self) -> f32 {
        self.timeline().2
    }
}

/// Page Up and ⌥↑ scroll the timeline up a page, Page Down and ⌥↓ down;
/// ⌘↑ goes to the oldest loaded message, ⌘↓ to the newest. Not while the
/// composer has the keys.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn keys_scroll_the_timeline() {
    let messages: Vec<String> = (0..90).map(|i| format!("Message {i}")).collect();
    let messages: Vec<&str> = messages.iter().map(String::as_str).collect();
    let (mut ui, _dir) = room_with("pages", &messages).await;
    let (view, height, bottom) = ui.timeline();
    assert_near(bottom, height - view);
    let page = view * 0.9;

    ui.press(Named::PageUp, Modifiers::empty());
    assert_near(ui.scrolled(), bottom - page);
    ui.press(Named::ArrowUp, Modifiers::ALT);
    assert_near(ui.scrolled(), bottom - 2.0 * page);
    ui.press(Named::ArrowDown, Modifiers::ALT);
    assert_near(ui.scrolled(), bottom - page);
    ui.press(Named::PageDown, Modifiers::empty());
    assert_near(ui.scrolled(), bottom);

    // All loaded messages are shown, from the top.
    ui.press(Named::ArrowUp, Modifiers::COMMAND);
    let (_, all, top) = ui.timeline();
    assert_near(top, 0.0);
    assert!(all > height, "{all} {height}");
    ui.press(Named::ArrowDown, Modifiers::COMMAND);
    assert_near(ui.scrolled(), all - view);

    // The composer takes them.
    ui.run(iced::widget::operation::focus(room::COMPOSER));
    assert_eq!(
        ui.press(Named::PageUp, Modifiers::empty()),
        Status::Captured
    );
    assert_near(ui.scrolled(), all - view);
}

/// Offsets, to within rounding.
#[track_caller]
fn assert_near(offset: f32, expected: f32) {
    assert!((offset - expected).abs() < 0.5, "{offset}, not {expected}");
}

/// The keys going to another room do so also while a field has them.
#[test]
fn keys_go_to_rooms_from_fields_too() {
    use sidebar::Go;

    let cases = [
        (
            Key::Named(Named::ArrowUp),
            Modifiers::COMMAND | Modifiers::ALT,
            Go::Above,
        ),
        (Key::Named(Named::Tab), Modifiers::CTRL, Go::Below),
        (Key::Character("3".into()), Modifiers::COMMAND, Go::Row(2)),
    ];
    for (key, modifiers, go) in cases {
        for status in [Status::Ignored, Status::Captured] {
            let message = shortcuts(press(key.clone(), modifiers), status, window::Id::unique());
            assert_eq!(
                format!("{message:?}"),
                format!("{:?}", Some(Message::GoToRoom(go))),
                "{key:?} with {modifiers:?}"
            );
        }
    }
}

/// A room in the room list.
fn summary(id: &OwnedRoomId, name: &str) -> mtrx_core::RoomSummary {
    mtrx_core::RoomSummary {
        id: id.clone(),
        name: name.into(),
        avatar: None,
        topic: None,
        kind: RoomKind::Room,
        invited: false,
        encrypted: false,
        public: false,
        unread_messages: 0,
        unread_mentions: 0,
        unread_notifications: 0,
        last_activity: None,
        parents: Vec::new(),
        tag: RoomTag::None,
        direct_user: None,
        alias: None,
        joined_members: 1,
    }
}

/// ⌥⌘↓ and ⌘1 open the room below and the first, also from the composer,
/// which leaves them: ⌘1 types no 1.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn keys_go_to_other_rooms_from_the_composer() {
    let (mut ui, _dir) = room_with("goto", &[]).await;
    let open = ui.room().id.clone();
    let other: OwnedRoomId = "!other:localhost".try_into().unwrap();
    ui.main().rooms.rooms = vec![summary(&other, "other"), summary(&open, "open")];
    ui.run(iced::widget::operation::focus(room::COMPOSER));

    let keys = [
        (
            Key::Named(Named::ArrowUp),
            Modifiers::COMMAND | Modifiers::ALT,
        ),
        (Key::Character("1".into()), Modifiers::COMMAND),
    ];
    for (key, modifiers) in keys {
        let (statuses, messages) = ui.messages(&[press(key.clone(), modifiers)]);
        assert_eq!(statuses, [Status::Ignored], "{key:?}");
        // Opening it (its tasks are not run: rooms load their timeline by
        // a subscription, which this window has not).
        for message in messages {
            let _ = ui.app.update(message);
        }
        assert_eq!(ui.main().opening.as_ref(), Some(&other), "{key:?}");
        ui.main().opening = None;
        assert_eq!(ui.room().composer.text(), "");
    }
}

#[test]
fn characters_typed_with_nothing_focused_go_to_the_composer() {
    let typed = |key: &str, modifiers, status| {
        let key = Key::Character(key.into());
        format!(
            "{:?}",
            shortcuts(press(key, modifiers), status, window::Id::unique())
        )
    };
    let to_composer = |text: &str| format!("{:?}", Some(Message::TypeInComposer(text.into())));
    assert_eq!(
        typed("h", Modifiers::empty(), Status::Ignored),
        to_composer("h")
    );
    assert_eq!(
        typed("H", Modifiers::SHIFT, Status::Ignored),
        to_composer("H")
    );
    // ⌥E, E: é.
    assert_eq!(
        typed("é", Modifiers::empty(), Status::Ignored),
        to_composer("é")
    );
    // A field took it.
    assert_eq!(typed("h", Modifiers::empty(), Status::Captured), "None");
    // Shortcuts.
    assert_eq!(typed("j", Modifiers::COMMAND, Status::Ignored), "None");
    assert_eq!(typed("j", Modifiers::CTRL, Status::Ignored), "None");
    // Not a character.
    let enter = press(Key::Named(Named::Enter), Modifiers::empty());
    assert!(shortcuts(enter, Status::Ignored, window::Id::unique()).is_none());
}

impl Ui {
    /// Whether the composer has the focus.
    fn composer_focused(&mut self) -> bool {
        let mut focused = super::ComposerFocused(None);
        let mut ui = UserInterface::build(
            self.app.view(),
            SIZE,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        ui.operate(&self.renderer, &mut focused);
        self.cache = ui.into_cache();
        focused.0 == Some(true)
    }
}

/// Typing with nothing focused goes to the composer, from the first
/// character on; not while something shows over the room.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn typing_with_nothing_focused_goes_to_the_composer() {
    let (mut ui, _dir) = room_with("typing", &["Hello"]).await;
    assert!(!ui.composer_focused());
    assert_eq!(
        ui.press(Key::Character("O".into()), Modifiers::SHIFT),
        Status::Ignored
    );
    assert!(ui.composer_focused());
    // From here the composer has the keys.
    assert_eq!(
        ui.press(Key::Character("k".into()), Modifiers::empty()),
        Status::Captured
    );
    assert_eq!(ui.room().composer.text(), "Ok");

    // Nothing focused, under the settings: nothing.
    ui.app.settings_panel = Some(settings::Panel::default());
    ui.run(iced::widget::operation::focus("nothing"));
    assert_eq!(
        ui.press(Key::Character("x".into()), Modifiers::empty()),
        Status::Ignored
    );
    assert_eq!(ui.room().composer.text(), "Ok");
}
