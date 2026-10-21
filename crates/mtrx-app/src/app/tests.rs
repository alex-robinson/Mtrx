//! UI snapshots against the local Synapse (`testserver/run.sh`):
//! `cargo test -p mtrx-app -- --ignored`, images land in `target/ui-snapshots/`.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use futures_util::StreamExt;
use iced::Size;
use iced_test::simulator::Simulator;
use mtrx_core::{
    Attachment, Connection, EventContent, NewRoom, OwnedEventId, Permissions, Profile, RoomKind,
    RoomOrAliasId, RoomTag, ScheduledFile, SecretStore, Section, SendState, Session, TimelineEntry,
    test_support::{PASSWORD, homeserver, register, unique_name},
};
use tempfile::TempDir;

use super::*;
use crate::{
    fonts,
    message_menu::MenuAt,
    selection::{Point, Pos, Selection},
};

const TIMEOUT: Duration = Duration::from_secs(30);

fn snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots")
}

async fn user(prefix: &str, dir: &TempDir) -> (String, Session) {
    let name = unique_name(prefix);
    register(&name).await.unwrap();
    let profile = Profile::new(prefix, Some(dir.path()), SecretStore::File).unwrap();
    let session = Session::login(profile, &homeserver(), &name, PASSWORD)
        .await
        .unwrap();
    session.start_sync().await;
    (name, session)
}

/// Show the room's start, as once back-pagination reached it.
fn with_start(entries: &mut Vec<TimelineEntry>) {
    if !matches!(entries.first(), Some(TimelineEntry::Start)) {
        entries.insert(0, TimelineEntry::Start);
    }
}

fn snapshot(app: &App, name: &str) {
    // One at a time: renderers drawing at once in parallel tests crashed
    // on Windows CI (software Direct3D).
    static RENDERING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _rendering = RENDERING.lock().unwrap_or_else(|e| e.into_inner());
    // Snapshots are for looking at, not regression baselines: always rewrite.
    // iced_test appends the renderer name, e.g. `login-wgpu.png`.
    let path = snapshot_dir().join(name);
    for entry in std::fs::read_dir(snapshot_dir())
        .into_iter()
        .flatten()
        .flatten()
    {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(&format!("{name}-"))
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let mut ui = Simulator::with_size(fonts::settings(), Size::new(1200.0, 800.0), app.view());
    // Images too large for the renderer's texture atlas only show after a
    // few frames, as the atlas grows (the app draws many).
    for _ in 0..2 {
        let _ = ui.snapshot(&app.theme()).unwrap();
    }
    let snapshot = ui.snapshot(&app.theme()).unwrap();
    assert!(snapshot.matches_image(&path).unwrap());
}

fn app(screen: Screen, mode: Mode) -> App {
    App {
        profile: Profile::new("ui", None, SecretStore::File).unwrap(),
        focused: true,
        google: Arc::new(Google::new(&snapshot_dir(), SecretStore::File)),
        settings: Settings::default(),
        settings_file: None,
        view_file: None,
        resizing: false,
        settings_panel: None,
        screen,
        mode,
        window: Size::new(1200.0, 800.0),
        scale: 2.0,
        math: Math::new(),
    }
}

#[test]
fn login_screen() {
    snapshot(&app(Screen::Login(Login::default()), Mode::Light), "login");
}

/// Without a keyring (Linux: no Secret Service) the sign-in screen says so.
#[test]
fn login_without_keyring() {
    let mut app = app(Screen::Starting, Mode::Light);
    let error = mtrx_core::Error::NoSecretStore("no secrets service".into());
    let _ = app.update(Message::Restored(Err(Shared(Arc::new(error)))));
    let Screen::Login(login) = &app.screen else {
        panic!("not the sign-in screen");
    };
    assert!(login.error.is_some());
    snapshot(&app, "login-no-keyring");
}

/// A diagonal gradient as PNG, for avatars.
fn picture(from: [u8; 3], to: [u8; 3]) -> Vec<u8> {
    gradient(96, 96, from, to)
}

/// A `width`×`height` diagonal gradient as PNG.
fn gradient(width: u32, height: u32, from: [u8; 3], to: [u8; 3]) -> Vec<u8> {
    let img = ::image::RgbImage::from_fn(width, height, |x, y| {
        let t = (x + y) as f32 / (width + height) as f32;
        ::image::Rgb(std::array::from_fn(|i| {
            (from[i] as f32 * (1.0 - t) + to[i] as f32 * t) as u8
        }))
    });
    let mut png = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png),
        ::image::ImageFormat::Png,
    )
    .unwrap();
    png
}

/// Wait for the invite to `room`, then join.
async fn accept_invite(session: &Session, room: &OwnedRoomId) {
    let list = session.room_list().await.unwrap();
    let mut updates = std::pin::pin!(list.updates());
    tokio::time::timeout(TIMEOUT, async {
        while let Some(state) = updates.next().await {
            if state.rooms.iter().any(|r| &r.id == room) {
                break;
            }
        }
    })
    .await
    .unwrap();
    session.join(<&RoomOrAliasId>::from(&**room)).await.unwrap();
}

/// Send and wait until the server has it.
async fn send(timeline: &RoomTimeline, text: &str) -> OwnedEventId {
    send_reply(timeline, text, None).await
}

async fn send_reply(
    timeline: &RoomTimeline,
    text: &str,
    to: Option<&OwnedEventId>,
) -> OwnedEventId {
    tokio::time::timeout(
        TIMEOUT,
        timeline
            .send_markdown(text, to.map(|id| &**id))
            .await
            .unwrap()
            .sent(),
    )
    .await
    .unwrap()
    .unwrap()
}

/// The key of the message with `body`, once the timeline has it.
async fn key_of(timeline: &RoomTimeline, body: &str) -> mtrx_core::EventKey {
    tokio::time::timeout(TIMEOUT, async {
        let mut updates = std::pin::pin!(timeline.updates());
        while let Some(entries) = updates.next().await {
            let found = entries.iter().find_map(|e| match e {
                TimelineEntry::Event(ev)
                    if matches!(&ev.content, EventContent::Message(m) if m.body == body) =>
                {
                    Some(ev.key.clone())
                }
                _ => None,
            });
            if let Some(key) = found {
                return key;
            }
        }
        unreachable!()
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn main_screen_with_math() {
    let dir = TempDir::new().unwrap();
    let (_, alice) = user("alice", &dir).await;
    let (_, bob) = user("bob", &dir).await;
    let (_, carol) = user("carol", &dir).await;
    // Alice and the rooms keep the initial-letter fallback.
    bob.set_avatar("image/png", picture([40, 90, 200], [120, 220, 255]))
        .await
        .unwrap();
    carol
        .set_avatar("image/png", picture([230, 120, 40], [250, 210, 90]))
        .await
        .unwrap();

    let space = alice
        .create_room(NewRoom {
            name: Some("Climate".into()),
            space: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let room = alice
        .create_room(NewRoom {
            name: Some("ice-sheets".into()),
            topic: Some("Basal melt and ice flux: drafts, figures and model runs".into()),
            encrypted: true,
            invite: vec![bob.user_id(), carol.user_id()],
            ..Default::default()
        })
        .await
        .unwrap();
    let general = alice
        .create_room(NewRoom {
            name: Some("general".into()),
            encrypted: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let dm = alice
        .create_room(NewRoom {
            invite: vec![carol.user_id()],
            direct: true,
            encrypted: true,
            ..Default::default()
        })
        .await
        .unwrap();
    alice.add_to_space(&space, &room).await.unwrap();
    alice.add_to_space(&space, &general).await.unwrap();

    accept_invite(&bob, &room).await;
    accept_invite(&carol, &room).await;
    accept_invite(&carol, &dm).await;

    let alice_timeline = alice.timeline(&room).await.unwrap();
    // Wait for the joins so bob and carol receive the room keys.
    let mut entries = std::pin::pin!(alice_timeline.updates());
    tokio::time::timeout(TIMEOUT, async {
        while let Some(entries) = entries.next().await {
            let joined = |user: &Session| {
                entries
                    .iter()
                    .any(|e| matches!(e, TimelineEntry::Event(ev) if ev.sender == user.user_id()))
            };
            if joined(&bob) && joined(&carol) {
                break;
            }
        }
    })
    .await
    .unwrap();
    let mut sent = Vec::new();
    for text in [
        "Morning! Draft of the **ice flux** section is up.",
        r"The basal melt rate follows $\dot m = \frac{G + \tau_b u_b}{\rho_i L}$ where $G$ is the geothermal flux.",
        "Steady state requires\n$$\\frac{\\partial H}{\\partial t} = -\\nabla \\cdot (H \\bar{\\mathbf{u}}) + \\dot a - \\dot m = 0$$",
    ] {
        sent.push(send(&alice_timeline, text).await);
    }
    let bob_timeline = bob.timeline(&room).await.unwrap();
    send(
        &bob_timeline,
        "Looks good, but check the sign of $\\dot m$ in eq. 2 🙂",
    )
    .await;
    send_reply(
        &bob_timeline,
        "Which geothermal flux dataset is it?",
        Some(&sent[0]),
    )
    .await;
    let carol_timeline = carol.timeline(&room).await.unwrap();
    send(
        &carol_timeline,
        "I can run the sensitivity tests tonight. Quick call first? https://meet.google.com/abc-defg-hij",
    )
    .await;
    send(
        &carol.timeline(&dm).await.unwrap(),
        "Coffee before the meeting?",
    )
    .await;
    let draft = "Morning! Draft of the **ice flux** section is up.";
    carol_timeline
        .toggle_reaction(&key_of(&carol_timeline, draft).await, "🎉")
        .await
        .unwrap();
    bob_timeline
        .toggle_reaction(&key_of(&bob_timeline, draft).await, "🎉")
        .await
        .unwrap();
    alice_timeline
        .toggle_reaction(
            &key_of(&alice_timeline, "Which geothermal flux dataset is it?").await,
            "👍",
        )
        .await
        .unwrap();
    bob_timeline.mark_read().await.unwrap();

    // Alice's view once the room list knows the space and the DM's picture.
    let list = Arc::new(alice.room_list().await.unwrap());
    let rooms = tokio::time::timeout(TIMEOUT, async {
        let mut updates = std::pin::pin!(list.updates());
        while let Some(state) = updates.next().await {
            let in_space = state
                .rooms
                .iter()
                .any(|r| r.id == room && r.parents.contains(&space));
            let dm_avatar = state.rooms.iter().any(|r| r.id == dm && r.avatar.is_some());
            if state.loaded && in_space && dm_avatar {
                return state;
            }
        }
        unreachable!()
    })
    .await
    .unwrap();
    let timeline = Arc::new(alice.timeline(&room).await.unwrap());
    while !timeline.paginate_back(50).await.unwrap() {}
    let entries = tokio::time::timeout(TIMEOUT, async {
        let mut updates = std::pin::pin!(timeline.updates());
        while let Some(entries) = updates.next().await {
            let events = || {
                entries.iter().filter_map(|e| match e {
                    TimelineEntry::Event(ev) => Some(ev),
                    _ => None,
                })
            };
            let messages = || {
                events().filter_map(|ev| match &ev.content {
                    EventContent::Message(m) => Some(m),
                    _ => None,
                })
            };
            let from_carol = events().any(|ev| {
                ev.sender == carol.user_id() && matches!(ev.content, EventContent::Message(_))
            });
            let reply_loaded =
                messages().any(|m| m.in_reply_to.as_ref().is_some_and(|r| r.preview.is_some()));
            let reactions = messages().filter(|m| !m.reactions.is_empty()).count() == 2
                && messages().any(|m| m.reactions.iter().any(|r| r.count == 2));
            let read = events().any(|ev| ev.read_by.iter().any(|r| r.id == bob.user_id()));
            if from_carol && reply_loaded && reactions && read {
                return entries;
            }
        }
        unreachable!()
    })
    .await
    .unwrap();
    // Shown hovered, with its time in the gutter.
    let hovered = entries
        .iter()
        .find_map(|e| match e {
            TimelineEntry::Event(ev) => match &ev.content {
                EventContent::Message(m) if m.body.starts_with("The basal melt") => {
                    Some(ev.id.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .unwrap();

    for mode in [Mode::Light, Mode::Dark] {
        let session = Arc::new(Session::restore(alice.profile().clone()).await.unwrap());
        let mut main = Main::new(session);
        main.rooms = rooms.clone();
        if mode == Mode::Dark {
            // Notes to self, started and unread; pinned above the sections.
            let mut notes = (rooms.rooms.iter())
                .find(|r| r.direct_user.is_some())
                .unwrap()
                .clone();
            notes.id = "!notes:localhost".try_into().unwrap();
            notes.direct_user = Some(alice.user_id());
            notes.unread_messages = 2;
            main.rooms.rooms.push(notes);
        }
        main.verified = Some(true);
        let mut open = Room::new(room.clone(), timeline.clone());
        open.entries = entries.clone();
        with_start(&mut open.entries);
        open.hover(hovered.clone(), true);
        open.calls.link = Some(Url::parse("https://meet.jit.si/ice-sheets-weekly").unwrap());
        open.calls.space_service =
            Some(CallService::jitsi("Lab Jitsi", "https://video.lab.example").unwrap());
        // The start menu and reaction picker in one shot, the link editor
        // and a reply in the other.
        open.calls.menu = mode == Mode::Dark;
        if mode == Mode::Light {
            open.calls.editor = Some("https://meet.jit.si/ice-sheets-weekly".into());
            open.calls.announce = true;
            // The hovered message's menu, from its toolbar.
            open.menu = Some((hovered.clone(), MenuAt::Toolbar));
            // Text selected from the first message into bob's reply.
            let id_of = |start: &str| {
                entries
                    .iter()
                    .find_map(|e| match e {
                        TimelineEntry::Event(ev) => match &ev.content {
                            EventContent::Message(m) if m.body.starts_with(start) => {
                                Some(ev.id.clone())
                            }
                            _ => None,
                        },
                        _ => None,
                    })
                    .unwrap()
            };
            open.selection = Some(Selection {
                anchor: Point {
                    event: id_of("The basal"),
                    pos: Pos { unit: 0, offset: 4 },
                },
                focus: Point {
                    event: id_of("I can run"),
                    pos: Pos { unit: 0, offset: 9 },
                },
                dragging: false,
            });
            // From inside typeset math, formulas as LaTeX; each sender's
            // name and time first.
            let copied = open.selected_text().unwrap();
            let name = |user: &mtrx_core::OwnedUserId| user.localpart().to_owned();
            assert!(copied.starts_with(&format!("{}, ", name(&alice.user_id()))));
            assert!(copied.contains(
                "\nbasal melt rate follows $\\dot m = \\frac{G + \\tau_b u_b}{\\rho_i L}$ where"
            ));
            assert!(copied.contains(&format!("\n\n{}, ", name(&bob.user_id()))));
            assert!(copied.contains("\nWhich geothermal flux dataset is it?\n\n"));
            assert!(copied.ends_with("\nI can run"));
            assert!(copied.contains(&format!("\n\n{}, ", name(&carol.user_id()))));
        }
        open.typing = vec!["bob".into()];
        if mode == Mode::Dark {
            // From the smiley after alice's reactions.
            let reacted = entries
                .iter()
                .find_map(|e| match e {
                    TimelineEntry::Event(ev) => match &ev.content {
                        EventContent::Message(m) if !m.reactions.is_empty() => Some(ev.id.clone()),
                        _ => None,
                    },
                    _ => None,
                })
                .unwrap();
            open.picker = Some((reacted, actions::PickerAt::Reactions));
        } else {
            let carol_message = entries.iter().rev().find_map(|e| match e {
                TimelineEntry::Event(ev) if ev.sender == carol.user_id() => Some(ev),
                _ => None,
            });
            open.compose = Compose::Reply(
                actions::Target::of(
                    carol_message.unwrap(),
                    "carol".into(),
                    "I can run the sensitivity tests tonight.".into(),
                )
                .unwrap(),
            );
        }
        main.room = Some(open);
        for (mxc, size) in main.avatars_on_screen(&Default::default()) {
            main.avatars.load_now(&alice, &mxc, size).await;
        }

        let mut app = app(Screen::Main(Box::new(main)), mode);
        // Both name styles: plain (the default) in light, coloured in dark.
        app.settings.colored_names = mode == Mode::Dark;
        let params = app.math_params();
        if let Screen::Main(main) = &app.screen {
            for document in main.room.as_ref().unwrap().math_documents() {
                app.math.render_now(document, &params);
            }
        }
        let name = if mode == Mode::Dark {
            "main-dark"
        } else {
            "main-light"
        };
        snapshot(&app, name);
        // A click elsewhere closes the call menu (dark) or link editor (light).
        let mut ui = Simulator::with_size(fonts::settings(), Size::new(1200.0, 800.0), app.view());
        ui.point_at(iced::Point::new(400.0, 130.0));
        ui.simulate(iced_test::simulator::click());
        let messages: Vec<_> = ui.into_messages().collect();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::Calls(_, calls::Message::Close))),
            "{messages:?}"
        );
        if mode == Mode::Light {
            composer_keeps_focus(&mut app);
        }
    }
}

/// The composer keeps its focus, so typing goes on, while what is around
/// it comes and goes: someone typing, bars, banners, the member list,
/// panels over the window.
fn composer_keeps_focus(app: &mut App) {
    use iced::advanced::{renderer::Headless, widget::operation::focusable};
    use iced_runtime::user_interface::{Cache, UserInterface};

    fn main(app: &mut App) -> &mut Main {
        let Screen::Main(main) = &mut app.screen else {
            unreachable!()
        };
        main
    }
    fn room(app: &mut App) -> &mut Room {
        main(app).room.as_mut().unwrap()
    }
    // Nothing else open that could take the focus.
    room(app).menu = None;
    room(app).calls.editor = None;
    room(app).typing.clear();

    let mut renderer = iced::futures::executor::block_on(iced::Renderer::new(
        fonts::INTER,
        iced::Pixels(style::TEXT_SIZE),
        None,
    ))
    .unwrap();
    let size = Size::new(1200.0, 800.0);
    let mut ui = UserInterface::build(app.view(), size, Cache::default(), &mut renderer);
    ui.operate(&renderer, &mut focusable::focus(room::COMPOSER.into()));
    let mut cache = ui.into_cache();

    type Change = (&'static str, fn(&mut App));
    let changes: [Change; 14] = [
        ("someone types", |app| room(app).typing = vec!["bob".into()]),
        ("they stop", |app| room(app).typing.clear()),
        ("an error below", |app| {
            room(app).error = Some("Not sent".into())
        }),
        ("it goes", |app| room(app).error = None),
        ("members open", |app| {
            room(app).members = Some(Default::default())
        }),
        ("members close", |app| room(app).members = None),
        ("offline", |app| main(app).connection = Connection::Offline),
        ("online", |app| main(app).connection = Connection::Online),
        ("a banner", |app| {
            main(app).error = Some("Something failed".into())
        }),
        ("it goes", |app| main(app).error = None),
        ("settings open", |app| {
            app.settings_panel = Some(settings::Panel::default());
        }),
        ("settings close", |app| app.settings_panel = None),
        ("a message is queued", |app| {
            let room_id = room(app).id.clone();
            main(app).scheduled = vec![ScheduledMessage {
                id: "q".into(),
                room_id,
                text: "Later".into(),
                send_at: SystemTime::now() + Duration::from_secs(3600),
                in_reply_to: None,
                overdue: false,
                files: Vec::new(),
            }];
        }),
        ("and sent", |app| main(app).scheduled.clear()),
    ];
    for (what, change) in changes {
        change(app);
        let mut ui = UserInterface::build(app.view(), size, cache, &mut renderer);
        let mut focused = ComposerFocused(None);
        ui.operate(&renderer, &mut focused);
        cache = ui.into_cache();
        // Panels over the window hide it; it is focused again once they close.
        if what == "settings open" {
            continue;
        }
        assert_eq!(
            focused.0,
            Some(true),
            "the composer lost its focus when {what}"
        );
    }
}

/// Whether the composer is focused, once operated.
struct ComposerFocused(Option<bool>);

impl iced::advanced::widget::Operation for ComposerFocused {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
        operate(self);
    }

    fn focusable(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        _bounds: iced::Rectangle,
        state: &mut dyn iced::advanced::widget::operation::Focusable,
    ) {
        if id == Some(&iced::advanced::widget::Id::new(room::COMPOSER)) {
            self.0 = Some(state.is_focused());
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn send_later_bar() {
    let dir = TempDir::new().unwrap();
    let (_, alice) = user("later", &dir).await;
    let room = alice
        .create_room(NewRoom {
            name: Some("ice-sheets".into()),
            encrypted: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let long_name = "Working group on ice-sheet model intercomparison and data";
    let other = alice
        .create_room(NewRoom {
            name: Some(long_name.into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let list = alice.room_list().await.unwrap();
    let rooms = tokio::time::timeout(TIMEOUT, async {
        let mut updates = std::pin::pin!(list.updates());
        while let Some(state) = updates.next().await {
            let named = |id: &OwnedRoomId, name: &str| {
                state.rooms.iter().any(|r| &r.id == id && r.name == name)
            };
            if state.loaded && named(&room, "ice-sheets") && named(&other, long_name) {
                return state;
            }
        }
        unreachable!()
    })
    .await
    .unwrap();
    let timeline = Arc::new(alice.timeline(&room).await.unwrap());
    tokio::time::timeout(
        TIMEOUT,
        timeline
            .send_markdown("Reviews are due **Friday** ❤ 👍🏽 ☀️", None)
            .await
            .unwrap()
            .sent(),
    )
    .await
    .unwrap()
    .unwrap();
    while !timeline.paginate_back(50).await.unwrap() {}

    let now = SystemTime::now();
    let scheduled = |id: &str, text: &str, send_at, overdue| ScheduledMessage {
        id: id.into(),
        room_id: room.clone(),
        text: text.into(),
        send_at,
        in_reply_to: None,
        overdue,
        files: Vec::new(),
    };
    let mut main = Main::new(Arc::new(alice));
    main.rooms = rooms;
    main.verified = Some(true);
    main.now = now;
    main.scheduled = vec![
        scheduled(
            "a",
            "Reminder: send me your figures before the meeting.",
            now - Duration::from_secs(3 * 3600),
            true,
        ),
        ScheduledMessage {
            files: vec![ScheduledFile {
                filename: "draft-v3.pdf".into(),
                mimetype: "application/pdf".into(),
                size: 182_000,
            }],
            ..scheduled(
                "b",
                "Good morning! The draft is ready for review: $\\dot m$ is fixed.",
                now + Duration::from_secs(2 * 3600 + 600),
                false,
            )
        },
        ScheduledMessage {
            room_id: other.clone(),
            ..scheduled(
                "c",
                "Agenda for Thursday",
                now + Duration::from_secs(86400),
                false,
            )
        },
    ];
    let mut open = Room::new(room.clone(), timeline.clone());
    open.entries = timeline.current();
    with_start(&mut open.entries);
    open.composer = text_editor::Content::with_text("Who can take the Monday slot? ❤");
    open.later.picker = Some(send_later::Picker::new(
        now,
        mtrx_core::parse_send_time("tomorrow 8:00", now),
    ));
    main.room = Some(open);
    let mut app = app(Screen::Main(Box::new(main)), Mode::Light);
    snapshot(&app, "send-later");

    // Editing in place: a sent message, then a scheduled one, while the
    // composer keeps its draft.
    if let Screen::Main(main) = &mut app.screen
        && let Some(open) = main.room.as_mut()
    {
        open.later = Default::default();
        let sent = open
            .entries
            .iter()
            .find_map(|e| match e {
                TimelineEntry::Event(ev) if ev.editable && ev.event_id.is_some() => {
                    Some(ev.key.clone())
                }
                _ => None,
            })
            .unwrap();
        open.editing = Some(actions::InlineEdit::new(
            actions::Editing::Sent(sent),
            "Reviews are due **Monday**.",
        ));
    }
    snapshot(&app, "edit-sent");
    if let Screen::Main(main) = &mut app.screen
        && let Some(open) = main.room.as_mut()
    {
        open.editing = Some(actions::InlineEdit::new(
            actions::Editing::Scheduled("b".into()),
            "Good morning! The draft is ready.",
        ));
    }
    snapshot(&app, "edit-scheduled");
    if let Screen::Main(main) = &mut app.screen
        && let Some(open) = main.room.as_mut()
    {
        open.editing = None;
        open.later = Default::default();
        let message = open
            .entries
            .iter()
            .find_map(|e| match e {
                TimelineEntry::Event(ev) if ev.editable => Some(ev.id.clone()),
                _ => None,
            })
            .unwrap();
        open.picker = Some((message, actions::PickerAt::Toolbar));
        open.emoji = Some(emoji::Panel::default());
        main.frequent_emoji = vec!["🦀".into(), "🧊".into()];
    }
    snapshot(&app, "emoji");
    if let Screen::Main(main) = &mut app.screen
        && let Some(open) = main.room.as_mut()
    {
        open.picker = None;
        open.emoji = None;
    }
    if let Screen::Main(main) = &mut app.screen
        && let Some(open) = main.room.as_mut()
    {
        open.composer_emoji = Some(emoji::Panel::default());
    }
    snapshot(&app, "composer-emoji");
    // A click outside the panel closes it; one on an emoji inserts it.
    let messages = |at: iced::Point| {
        let mut ui = Simulator::with_size(fonts::settings(), Size::new(1200.0, 800.0), app.view());
        ui.point_at(at);
        ui.simulate(iced_test::simulator::click());
        ui.into_messages().collect::<Vec<_>>()
    };
    let outside = messages(iced::Point::new(700.0, 150.0));
    assert!(
        outside
            .iter()
            .any(|m| matches!(m, Message::ComposerEmoji(emoji::Message::Close))),
        "{outside:?}"
    );
    let on_crab = messages(iced::Point::new(782.0, 447.0));
    assert!(
        on_crab
            .iter()
            .any(|m| matches!(m, Message::InsertEmoji(e) if e == "🦀")),
        "{on_crab:?}"
    );
    if let Screen::Main(main) = &mut app.screen
        && let Some(open) = main.room.as_mut()
    {
        open.composer_emoji = None;
        let sent = open
            .entries
            .iter()
            .find_map(|e| match e {
                TimelineEntry::Event(ev) if ev.editable => Some(ev.key.clone()),
                _ => None,
            })
            .unwrap();
        let mut edit =
            actions::InlineEdit::new(actions::Editing::Sent(sent), "Reviews are due Monday 🎉");
        let mut panel = emoji::Panel::default();
        panel.update(emoji::Message::Search("ice".into()));
        edit.emoji = Some(panel);
        open.editing = Some(edit);
    }
    snapshot(&app, "edit-emoji");
    if let Screen::Main(main) = &mut app.screen
        && let Some(open) = main.room.as_mut()
    {
        open.editing = None;
    }
    if let Screen::Main(main) = &mut app.screen
        && let Some(open) = main.room.as_mut()
    {
        open.editing = None;
    }
    let mut panel = settings::Panel::default();
    panel.room = Some(settings::Access::new(
        room.clone(),
        "ice-sheets".into(),
        false,
    ));
    panel.update(
        &mut app.settings,
        settings::Message::AccessLoaded(room.clone(), Some(Permissions::Moderators), Some(true)),
    );
    app.settings_panel = Some(panel);
    snapshot(&app, "settings");
}

/// Timings for a big encrypted room opened on a new, recovered device:
/// `cargo test --release -p mtrx-app big_room -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh); slow"]
async fn big_room_benchmark() {
    const MESSAGES: usize = 1500;
    let dir = TempDir::new().unwrap();
    let (name, first) = user("bench", &dir).await;
    let room = first
        .create_room(NewRoom {
            name: Some("big".into()),
            encrypted: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let timeline = first.timeline(&room).await.unwrap();
    let start = std::time::Instant::now();
    let mut last = None;
    for i in 0..MESSAGES {
        let text = if i % 10 == 0 {
            format!("message {i} with math $x_{{{i}}} = \\frac{{a}}{{b}}$")
        } else {
            format!("message {i}: some **text** to read")
        };
        last = Some(timeline.send_markdown(&text, None).await.unwrap());
    }
    tokio::time::timeout(Duration::from_secs(300), last.unwrap().sent())
        .await
        .unwrap()
        .unwrap();
    println!("sent {MESSAGES} messages in {:.1?}", start.elapsed());
    let key = first.enable_recovery().await.unwrap();

    // New device, recovered: like a fresh install.
    let dir2 = TempDir::new().unwrap();
    let profile = Profile::new("second", Some(dir2.path()), SecretStore::File).unwrap();
    let second = Session::login(profile, &homeserver(), &name, PASSWORD)
        .await
        .unwrap();
    second.start_sync().await;
    second.room_list().await.unwrap().wait_loaded().await;
    second.recover(&key, |_| {}).await.unwrap();

    let start = std::time::Instant::now();
    let timeline = Arc::new(second.timeline(&room).await.unwrap());
    let mut updates = std::pin::pin!(timeline.updates());
    let paginate = {
        let timeline = timeline.clone();
        tokio::spawn(async move { while !timeline.paginate_back(50).await.unwrap() {} })
    };
    let mut count = 0;
    let decrypted = |entries: &[TimelineEntry]| {
        let messages = entries
            .iter()
            .filter(|e| matches!(e, TimelineEntry::Event(ev) if matches!(ev.content, EventContent::Message(_))))
            .count();
        let utd = entries
            .iter()
            .filter(|e| matches!(e, TimelineEntry::Event(ev) if ev.content == EventContent::UnableToDecrypt))
            .count();
        (messages, utd)
    };
    let mut paginated_at = None;
    while let Some(entries) = tokio::time::timeout(Duration::from_secs(120), updates.next())
        .await
        .unwrap()
    {
        count += 1;
        if paginate.is_finished() && paginated_at.is_none() {
            paginated_at = Some(start.elapsed());
        }
        let (messages, utd) = decrypted(&entries);
        if messages == MESSAGES && utd == 0 {
            break;
        }
    }
    println!(
        "new device: history loaded after {:.1?}, all decrypted after {:.1?} ({count} timeline updates)",
        paginated_at.unwrap_or_default(),
        start.elapsed()
    );

    let start = std::time::Instant::now();
    for _ in 0..100 {
        std::hint::black_box(timeline.current());
    }
    println!(
        "timeline snapshot ({} entries): {:.2?} each",
        timeline.current().len(),
        start.elapsed() / 100
    );

    // View cost with every entry in the room view.
    let session = Arc::new(second);
    let mut main = Main::new(session);
    let mut open = Room::new(room.clone(), timeline.clone());
    with_start(&mut open.entries);
    main.room = Some(open);
    let app = app(Screen::Main(Box::new(main)), Mode::Light);
    let start = std::time::Instant::now();
    for _ in 0..5 {
        let mut ui = Simulator::with_size(fonts::settings(), Size::new(1200.0, 800.0), app.view());
        std::hint::black_box(ui.snapshot(&app.theme()).unwrap());
    }
    println!(
        "view + layout + draw: {:.1?} per frame",
        start.elapsed() / 5
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn room_management_screens() {
    let dir = TempDir::new().unwrap();
    let (_, alice) = user("manage", &dir).await;
    let (_, bob) = user("member", &dir).await;
    let space = alice
        .create_room(NewRoom {
            name: Some("Climate".into()),
            space: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let room = alice
        .create_room(NewRoom {
            name: Some("ice-sheets".into()),
            parent: Some(space.clone()),
            invite: vec![bob.user_id()],
            ..Default::default()
        })
        .await
        .unwrap();
    let list = alice.room_list().await.unwrap();
    let rooms = tokio::time::timeout(TIMEOUT, async {
        let mut updates = std::pin::pin!(list.updates());
        while let Some(state) = updates.next().await {
            if state.loaded
                && state
                    .rooms
                    .iter()
                    .any(|r| r.id == room && r.parents.contains(&space) && r.name == "ice-sheets")
            {
                return state;
            }
        }
        unreachable!()
    })
    .await
    .unwrap();
    let timeline = Arc::new(alice.timeline(&room).await.unwrap());
    let members = alice.members(&room).await.unwrap();
    let session = Arc::new(alice);

    let main_with = |configure: &dyn Fn(&mut Main)| {
        let mut main = Main::new(session.clone());
        main.rooms = rooms.clone();
        main.space = Some(space.clone());
        main.verified = Some(true);
        let mut open = Room::new(room.clone(), timeline.clone());
        open.entries = timeline.current();
        with_start(&mut open.entries);
        main.room = Some(open);
        configure(&mut main);
        app(Screen::Main(Box::new(main)), Mode::Light)
    };

    snapshot(
        &main_with(&|main| {
            main.add_menu = true;
        }),
        "menus",
    );
    snapshot(
        &main_with(&|main| main.connection = Connection::Offline),
        "offline",
    );
    snapshot(
        &main_with(&|main| main.connection = Connection::SignedOut),
        "signed-out",
    );
    snapshot(
        &main_with(&|main| {
            main.out_of_view = sidebar::OutOfView {
                above: Some((1, 0.0)),
                below: Some((3, 400.0)),
            }
        }),
        "unread-arrows",
    );
    snapshot(
        &main_with(&|main| {
            let mut dialog = Dialog::new(dialog::Kind::NewRoom {
                space: Some((space.clone(), "Climate".into())),
            });
            dialog.update(dialog::Message::Name("model runs".into()));
            dialog.update(dialog::Message::Submit);
            dialog.tick(Instant::now() + Duration::from_secs(20));
            main.dialog = Some(dialog);
        }),
        "dialog-slow",
    );
    snapshot(
        &main_with(&|main| {
            main.room = None;
            main.invite = Some((room.clone(), false));
            main.notice = Some(Notice {
                text: "The room “model runs” is ready.".into(),
                room: Some(room.clone()),
                error: false,
            });
        }),
        "invite",
    );
    snapshot(
        &main_with(&|main| {
            main.filter = "ice".into();
            main.room.as_mut().unwrap().members = Some(Members {
                list: Some(Ok(members.clone())),
            });
        }),
        "members-search",
    );

    let bob_info = session.user_info(&bob.user_id()).await.unwrap();
    let contacts = session.contacts().await.unwrap();
    snapshot(
        &main_with(&|main| {
            let mut card = info::Card::new(info::Target::User(bob_info.user.clone()), &[]);
            card.update(info::Message::UserLoaded(
                bob.user_id(),
                Ok(bob_info.clone()),
            ));
            card.update(info::Message::Copy(bob.user_id().to_string()));
            main.info = Some(card);
        }),
        "info-user",
    );
    let settings = session.room_settings(&room).await.unwrap();
    snapshot(
        &main_with(&|main| {
            let mut card = info::Card::new(info::Target::Room(room.clone()), &main.rooms.rooms);
            card.update(info::Message::SettingsLoaded(
                room.clone(),
                Ok(settings.clone()),
            ));
            card.update(info::Message::Topic("Ice sheet runs, weekly".into()));
            main.info = Some(card);
        }),
        "room-settings",
    );
    snapshot(
        &main_with(&|main| {
            let mut dialog = Dialog::new(dialog::Kind::NewRoom {
                space: Some((space.clone(), "Climate".into())),
            });
            dialog.prefer_spaces(vec![space.clone()]);
            dialog.update(dialog::Message::Contacts(contacts.clone()));
            main.dialog = Some(dialog);
        }),
        "new-room-people",
    );
    snapshot(
        &main_with(&|main| {
            let mut dialog = Dialog::new(dialog::Kind::DirectChat);
            dialog.offer_self(main.own_user.clone());
            dialog.update(dialog::Message::Contacts(contacts.clone()));
            main.dialog = Some(dialog);
        }),
        "direct-chat-people",
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn media_messages() {
    let dir = TempDir::new().unwrap();
    let (_, alice) = user("media", &dir).await;
    let room = alice
        .create_room(NewRoom {
            name: Some("ice-sheets".into()),
            encrypted: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let list = alice.room_list().await.unwrap();
    let rooms = tokio::time::timeout(TIMEOUT, async {
        let mut updates = std::pin::pin!(list.updates());
        while let Some(state) = updates.next().await {
            if state.loaded
                && state
                    .rooms
                    .iter()
                    .any(|r| r.id == room && r.name == "ice-sheets")
            {
                return state;
            }
        }
        unreachable!()
    })
    .await
    .unwrap();
    let timeline = Arc::new(alice.timeline(&room).await.unwrap());
    let map = gradient(640, 400, [16, 52, 120], [110, 205, 225]);
    let files = vec![
        Attachment {
            filename: "melt-rate-map.png".into(),
            mimetype: "image/png".into(),
            bytes: map,
        },
        Attachment {
            filename: "ice-flux-draft.pdf".into(),
            mimetype: "application/pdf".into(),
            bytes: crate::pdf::sample(),
        },
    ];
    timeline
        .send(
            "Melt-rate map from last night's run, and the draft:",
            files,
            None,
        )
        .await
        .unwrap();
    while !timeline.paginate_back(50).await.unwrap() {}
    let entries = tokio::time::timeout(TIMEOUT, async {
        let mut updates = std::pin::pin!(timeline.updates());
        while let Some(entries) = updates.next().await {
            let sent = entries
                .iter()
                .filter(|e| match e {
                    TimelineEntry::Event(ev) => {
                        ev.send_state == SendState::Sent
                            && matches!(&ev.content, EventContent::Message(m) if m.media.is_some())
                    }
                    _ => false,
                })
                .count();
            if sent == 2 {
                return entries;
            }
        }
        unreachable!()
    })
    .await
    .unwrap();
    let file = |extension: &str| {
        entries
            .iter()
            .find_map(|e| match e {
                TimelineEntry::Event(ev) => match &ev.content {
                    EventContent::Message(m) => {
                        m.media.clone().filter(|m| m.filename.ends_with(extension))
                    }
                    _ => None,
                },
                _ => None,
            })
            .unwrap()
    };
    let (image, pdf) = (file(".png"), file(".pdf"));

    let session = Arc::new(alice);
    let main_with = async |viewer: Option<Viewer>, pdf: Option<pdf_viewer::Viewer>| {
        let mut main = Main::new(session.clone());
        main.rooms = rooms.clone();
        main.verified = Some(true);
        let mut open = Room::new(room.clone(), timeline.clone());
        open.entries = entries.clone();
        with_start(&mut open.entries);
        let images: Vec<Media> = open.previewed().cloned().collect();
        for media in &images {
            main.previews.load_now(&session, media).await;
        }
        main.room = Some(open);
        main.viewer = viewer;
        main.pdf = pdf;
        Box::new(main)
    };
    for (mode, name) in [(Mode::Light, "media-light"), (Mode::Dark, "media-dark")] {
        snapshot(&app(Screen::Main(main_with(None, None).await), mode), name);
    }
    // Files attached to a message being written sit in the composer.
    let files = dir.path().join("attach");
    std::fs::create_dir(&files).unwrap();
    let paths = vec![files.join("terminus.png"), files.join("notes.pdf")];
    std::fs::write(
        &paths[0],
        gradient(300, 200, [200, 90, 40], [250, 220, 120]),
    )
    .unwrap();
    std::fs::write(&paths[1], crate::pdf::sample()).unwrap();
    let mut main = main_with(None, None).await;
    let open = main.room.as_mut().unwrap();
    open.attachments = crate::media::read_attached(paths)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();
    open.composer = text_editor::Content::with_text("The calving front this morning:");
    snapshot(
        &app(Screen::Main(main), Mode::Light),
        "composer-attachments",
    );
    let mut viewer = Viewer::new(image);
    viewer.load_now(&session).await;
    snapshot(
        &app(
            Screen::Main(main_with(Some(viewer), None).await),
            Mode::Light,
        ),
        "image-viewer",
    );
    let mut viewer = pdf_viewer::Viewer::new(pdf.clone(), Size::new(1200.0, 800.0), 2.0);
    viewer.open_now(&session).await;
    snapshot(
        &app(
            Screen::Main(main_with(None, Some(viewer)).await),
            Mode::Light,
        ),
        "pdf-viewer",
    );
    let mut viewer = pdf_viewer::Viewer::new(pdf, Size::new(1200.0, 800.0), 2.0);
    viewer.set_zoom(0.5);
    viewer.open_now(&session).await;
    snapshot(
        &app(
            Screen::Main(main_with(None, Some(viewer)).await),
            Mode::Dark,
        ),
        "pdf-viewer-zoomed-out",
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn room_categories() {
    let dir = TempDir::new().unwrap();
    let (_, alice) = user("arrange", &dir).await;
    let (_, bob) = user("colleague", &dir).await;
    let mut ids = HashMap::new();
    for name in ["ice-sheets", "ocean", "admin", "archive", "seminar"] {
        let id = alice
            .create_room(NewRoom {
                name: Some(name.into()),
                ..Default::default()
            })
            .await
            .unwrap();
        ids.insert(name, id);
    }
    alice.direct_chat(&bob.user_id()).await.unwrap();
    alice
        .set_room_tag(&ids["ocean"], RoomTag::Favourite)
        .await
        .unwrap();
    alice
        .set_room_tag(&ids["archive"], RoomTag::LowPriority)
        .await
        .unwrap();
    let mut projects = String::new();
    alice
        .change_room_categories(|c| {
            let home = c.layout_mut(None);
            projects = home.add_category("Projects");
            home.place(&ids["ice-sheets"], Some(&projects));
            home.place(&ids["seminar"], Some(&projects));
            home.add_category("Teaching");
        })
        .await
        .unwrap();
    let list = alice.room_list().await.unwrap();
    let rooms = tokio::time::timeout(TIMEOUT, async {
        let mut updates = std::pin::pin!(list.updates());
        while let Some(state) = updates.next().await {
            let tag = |name| {
                state
                    .rooms
                    .iter()
                    .find(|r| r.id == ids[name])
                    .map(|r| r.tag)
            };
            if state.loaded
                && state.rooms.len() == 6
                && state.rooms.iter().any(|r| r.kind == RoomKind::Direct)
                && tag("ocean") == Some(RoomTag::Favourite)
                && tag("archive") == Some(RoomTag::LowPriority)
            {
                return state;
            }
        }
        unreachable!()
    })
    .await
    .unwrap();
    let session = Arc::new(alice);

    let main_with = |configure: &dyn Fn(&mut Main)| {
        let mut main = Main::new(session.clone());
        main.rooms = rooms.clone();
        main.verified = Some(true);
        configure(&mut main);
        app(Screen::Main(Box::new(main)), Mode::Light)
    };
    let send = |main: &mut Main, message| {
        let _ = main.arrange.update(
            message,
            arrange::Context {
                session: &session,
                space: None,
                rooms: &mut main.rooms.rooms,
                grouping: mtrx_core::Grouping::default(),
            },
        );
    };

    snapshot(
        &main_with(&|main| {
            send(main, arrange::Message::Row(ids["ice-sheets"].clone(), true));
            send(
                main,
                arrange::Message::RoomMenu(Some(ids["ice-sheets"].clone())),
            );
        }),
        "categories",
    );
    snapshot(
        &main_with(&|main| {
            send(main, arrange::Message::Row(ids["admin"].clone(), true));
            send(main, arrange::Message::Pressed);
            send(
                main,
                arrange::Message::Moved(iced::Point::new(120.0, 330.0)),
            );
            send(
                main,
                arrange::Message::Moved(iced::Point::new(130.0, 250.0)),
            );
            send(
                main,
                arrange::Message::Target(Section::Category(projects.clone()), true),
            );
        }),
        "categories-drag",
    );

    // Dropping moves the room into the category; the menu favourites one.
    let mut main = Main::new(session.clone());
    main.rooms = rooms.clone();
    for message in [
        arrange::Message::Row(ids["admin"].clone(), true),
        arrange::Message::Pressed,
        arrange::Message::Moved(iced::Point::new(120.0, 330.0)),
        arrange::Message::Moved(iced::Point::new(130.0, 250.0)),
        arrange::Message::Target(Section::Category(projects.clone()), true),
        arrange::Message::Released,
        arrange::Message::MoveTo(ids["seminar"].clone(), Section::Favourites),
    ] {
        send(&mut main, message);
    }
    let home = main.arrange.categories.layout(None);
    let in_projects = |name| home.category_of(&ids[name]).map(|c| c.id.clone());
    assert_eq!(in_projects("admin"), Some(projects.clone()));
    assert_eq!(in_projects("seminar"), None);
    let seminar = main.rooms.rooms.iter().find(|r| r.id == ids["seminar"]);
    assert_eq!(seminar.map(|r| r.tag), Some(RoomTag::Favourite));
    assert!(main.arrange.drag.is_none());
}

/// Screenshots for the website (`site/images/`): a lab's room with math, a
/// reply, reactions and a call; then sending later, and copying a selection.
/// `cargo test -p mtrx-app website -- --ignored`, then copy the `site-*`
/// snapshots from `target/ui-snapshots/`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn website_screenshots() {
    let dir = TempDir::new().unwrap();
    let (_, ana) = user("ana", &dir).await;
    let (_, ben) = user("ben", &dir).await;
    let (_, chloe) = user("chloe", &dir).await;
    for (session, name) in [
        (&ana, "Ana Sousa"),
        (&ben, "Ben Okafor"),
        (&chloe, "Chloé Martin"),
    ] {
        session.set_display_name(name).await.unwrap();
    }
    ben.set_avatar("image/png", picture([40, 90, 200], [120, 220, 255]))
        .await
        .unwrap();
    chloe
        .set_avatar("image/png", picture([230, 120, 40], [250, 210, 90]))
        .await
        .unwrap();

    let space = ana
        .create_room(NewRoom {
            name: Some("Glaciology Lab".into()),
            space: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let mut rooms_in_space = Vec::new();
    for (name, topic) in [
        (
            "ice-sheets",
            "Basal melt and ice flux: drafts, figures and model runs",
        ),
        ("field-campaign", "Greenland 2027 logistics"),
        ("seminars", "Talks and reading group"),
    ] {
        let room = ana
            .create_room(NewRoom {
                name: Some(name.into()),
                topic: Some(topic.into()),
                encrypted: true,
                invite: vec![ben.user_id(), chloe.user_id()],
                ..Default::default()
            })
            .await
            .unwrap();
        ana.add_to_space(&space, &room).await.unwrap();
        accept_invite(&ben, &room).await;
        accept_invite(&chloe, &room).await;
        rooms_in_space.push(room);
    }
    let room = rooms_in_space[0].clone();
    let dm = ana
        .create_room(NewRoom {
            invite: vec![chloe.user_id()],
            direct: true,
            encrypted: true,
            ..Default::default()
        })
        .await
        .unwrap();
    accept_invite(&chloe, &dm).await;
    ana.direct_chat(&ana.user_id()).await.unwrap();

    let ana_timeline = ana.timeline(&room).await.unwrap();
    let mut entries = std::pin::pin!(ana_timeline.updates());
    tokio::time::timeout(TIMEOUT, async {
        while let Some(entries) = entries.next().await {
            let joined = |user: &Session| {
                entries
                    .iter()
                    .any(|e| matches!(e, TimelineEntry::Event(ev) if ev.sender == user.user_id()))
            };
            if joined(&ben) && joined(&chloe) {
                break;
            }
        }
    })
    .await
    .unwrap();
    let first = send(
        &ana_timeline,
        "Morning! The draft of the **ice flux** section is up.",
    )
    .await;
    send(
        &ana_timeline,
        r"The basal melt rate follows $\dot m = \frac{G + \tau_b u_b}{\rho_i L}$ where $G$ is the geothermal flux.",
    )
    .await;
    let ben_timeline = ben.timeline(&room).await.unwrap();
    send(
        &ben_timeline,
        "Looks good, but check the sign of $\\dot m$ in the mass balance 🙂",
    )
    .await;
    send_reply(
        &ben_timeline,
        "Which geothermal flux dataset is it?",
        Some(&first),
    )
    .await;
    let chloe_timeline = chloe.timeline(&room).await.unwrap();
    send(
        &chloe_timeline,
        "I can run the sensitivity tests tonight. Quick call first? https://meet.jit.si/glaciology-lab",
    )
    .await;
    send(
        &chloe.timeline(&dm).await.unwrap(),
        "Coffee before the seminar?",
    )
    .await;
    for timeline in [&ben_timeline, &chloe_timeline] {
        timeline
            .toggle_reaction(
                &key_of(
                    timeline,
                    "Morning! The draft of the **ice flux** section is up.",
                )
                .await,
                "🎉",
            )
            .await
            .unwrap();
    }
    ana_timeline
        .toggle_reaction(
            &key_of(&ana_timeline, "Which geothermal flux dataset is it?").await,
            "👍",
        )
        .await
        .unwrap();
    ben_timeline.mark_read().await.unwrap();
    chloe_timeline.mark_read().await.unwrap();

    let list = Arc::new(ana.room_list().await.unwrap());
    let rooms = tokio::time::timeout(TIMEOUT, async {
        let mut updates = std::pin::pin!(list.updates());
        while let Some(state) = updates.next().await {
            let in_space = rooms_in_space.iter().all(|id| {
                state
                    .rooms
                    .iter()
                    .any(|r| &r.id == id && r.parents.contains(&space))
            });
            let dm_avatar = state.rooms.iter().any(|r| r.id == dm && r.avatar.is_some());
            if state.loaded && in_space && dm_avatar {
                return state;
            }
        }
        unreachable!()
    })
    .await
    .unwrap();
    let timeline = Arc::new(ana.timeline(&room).await.unwrap());
    while !timeline.paginate_back(50).await.unwrap() {}
    let entries = tokio::time::timeout(TIMEOUT, async {
        let mut updates = std::pin::pin!(timeline.updates());
        while let Some(entries) = updates.next().await {
            let messages = || {
                entries.iter().filter_map(|e| match e {
                    TimelineEntry::Event(ev) => match &ev.content {
                        EventContent::Message(m) => Some((ev, m)),
                        _ => None,
                    },
                    _ => None,
                })
            };
            let named = messages().all(|(ev, _)| ev.sender_name.is_some());
            let reply = messages()
                .any(|(_, m)| m.in_reply_to.as_ref().is_some_and(|r| r.preview.is_some()));
            let reactions = messages().filter(|(_, m)| !m.reactions.is_empty()).count() == 2;
            let read = messages().any(|(ev, _)| ev.read_by.len() == 2);
            if messages().count() == 5 && named && reply && reactions && read {
                return entries;
            }
        }
        unreachable!()
    })
    .await
    .unwrap();
    let id_of = |start: &str| {
        entries
            .iter()
            .find_map(|e| match e {
                TimelineEntry::Event(ev) => match &ev.content {
                    EventContent::Message(m) if m.body.starts_with(start) => Some(ev.id.clone()),
                    _ => None,
                },
                _ => None,
            })
            .unwrap()
    };

    let ana = Arc::new(ana);
    let now = SystemTime::now();
    let screen = |mode: Mode| {
        let mut main = Main::new(ana.clone());
        main.rooms = rooms.clone();
        main.space = Some(space.clone());
        main.verified = Some(true);
        main.now = now;
        main.colours = RoomListColours {
            home: None,
            spaces: [(space.clone(), "Teal".to_owned())].into(),
        };
        main.scheduled = vec![ScheduledMessage {
            id: "a".into(),
            room_id: room.clone(),
            text: "Reminder: figures for Thursday's meeting, please!".into(),
            send_at: now + Duration::from_secs(14 * 3600),
            in_reply_to: None,
            overdue: false,
            files: Vec::new(),
        }];
        let mut open = Room::new(room.clone(), timeline.clone());
        open.entries = entries.clone();
        with_start(&mut open.entries);
        main.room = Some(open);
        let mut app = app(Screen::Main(Box::new(main)), mode);
        app.settings.colored_names = mode == Mode::Dark;
        app
    };
    async fn finish(app: &mut App, ana: &Session) {
        let params = app.math_params();
        if let Screen::Main(main) = &mut app.screen {
            for document in main.room.as_ref().unwrap().math_documents() {
                app.math.render_now(document, &params);
            }
            for (mxc, size) in main.avatars_on_screen(&Default::default()) {
                main.avatars.load_now(ana, &mxc, size).await;
            }
        }
    }

    for (mode, name) in [
        (Mode::Light, "site-main-light"),
        (Mode::Dark, "site-main-dark"),
    ] {
        let mut app = screen(mode);
        finish(&mut app, &ana).await;
        snapshot(&app, name);
    }

    // Sending later: the picker, with a draft.
    let mut app = screen(Mode::Light);
    if let Screen::Main(main) = &mut app.screen
        && let Some(open) = main.room.as_mut()
    {
        open.composer = text_editor::Content::with_text("Results are in the shared folder 📈");
        open.later.picker = Some(send_later::Picker::new(
            now,
            mtrx_core::parse_send_time("tomorrow 8:00", now),
        ));
    }
    finish(&mut app, &ana).await;
    snapshot(&app, "site-send-later");

    // Copying: a selection through typeset math into the next message.
    let mut app = screen(Mode::Light);
    if let Screen::Main(main) = &mut app.screen
        && let Some(open) = main.room.as_mut()
    {
        let math = id_of("The basal melt");
        open.selection = Some(Selection {
            anchor: Point {
                event: math.clone(),
                pos: Pos { unit: 0, offset: 4 },
            },
            focus: Point {
                event: id_of("Looks good"),
                pos: Pos {
                    unit: 0,
                    offset: 10,
                },
            },
            dragging: false,
        });
        open.hover(math.clone(), true);
        open.menu = Some((math, MenuAt::Toolbar));
    }
    finish(&mut app, &ana).await;
    snapshot(&app, "site-copy");
}
