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
        for message in messages {
            self.update(message);
        }
        statuses
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

/// `key` pressed with `modifiers`, typing its character if it has one.
fn press(key: Key, modifiers: Modifiers) -> Event {
    let text = match key.as_ref() {
        Key::Character(c) if !modifiers.command() => Some(c.into()),
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

#[test]
fn shift_and_arrows_move_the_end_of_the_selection() {
    use selection::{Message::*, Step};

    let shift = Modifiers::SHIFT;
    let cases = [
        (Named::ArrowLeft, shift, Extend(Step::Back)),
        (
            Named::ArrowRight,
            shift | Modifiers::ALT,
            Extend(Step::WordForward),
        ),
        (Named::ArrowUp, shift, ExtendLine(true)),
        (
            Named::ArrowLeft,
            shift | Modifiers::COMMAND,
            ExtendToLineEdge(false),
        ),
        (
            Named::ArrowRight,
            shift | Modifiers::COMMAND,
            ExtendToLineEdge(true),
        ),
        (
            Named::ArrowUp,
            shift | Modifiers::COMMAND,
            ExtendToScreenEdge(true),
        ),
        (
            Named::ArrowDown,
            shift | Modifiers::COMMAND,
            ExtendToScreenEdge(false),
        ),
    ];
    for (arrow, modifiers, expected) in cases {
        let message = shortcuts(
            press(Key::Named(arrow), modifiers),
            Status::Ignored,
            window::Id::unique(),
        );
        assert_eq!(
            format!("{message:?}"),
            format!("{:?}", Some(Message::Select(expected))),
            "{arrow:?} with {modifiers:?}"
        );
        // Not when a field took it.
        let taken = shortcuts(
            press(Key::Named(arrow), modifiers),
            Status::Captured,
            window::Id::unique(),
        );
        assert!(taken.is_none(), "{arrow:?} with {modifiers:?}");
    }
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
