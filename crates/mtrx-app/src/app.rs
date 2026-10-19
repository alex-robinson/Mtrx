//! Application state, messages and top-level layout.

use std::{
    fmt,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use futures_util::{SinkExt, StreamExt};
use iced::{
    Element, Length, Size, Subscription, Task, Theme,
    theme::Mode,
    widget::{
        button, column, container, image, mouse_area, opaque, row, scrollable::Viewport, stack,
        text, text_editor, text_input,
    },
    window,
};
use mtrx_core::{
    CallService, Google, OwnedMxcUri, OwnedRoomId, Profile, RecoveryStep, RoomList, RoomListState,
    RoomOrAliasId, RoomTimeline, ScheduledMessage, SecretStore, ServiceKind, Session,
    TimelineEntry, Url,
};

use crate::{
    avatar::{self, Avatars},
    calls,
    dialog::{self, Dialog},
    login::{self, Login},
    math::{self, Math, Rendered},
    room::{Look, Older, Room},
    send_later,
    settings::{self, Settings},
    sidebar,
    style::{self, MESSAGE_INSET, ROOM_LIST_WIDTH, SPACE_BAR_WIDTH},
};

/// How often times shown relative to now are refreshed.
const TICK: Duration = Duration::from_secs(20);
/// How often progress (elapsed seconds) is refreshed while something runs.
const PROGRESS_TICK: Duration = Duration::from_secs(1);

/// An `Arc` that identifies by pointer: lets live core handles be used as
/// subscription keys and carried in messages.
pub struct Shared<T>(pub Arc<T>);

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Shared<{}>", std::any::type_name::<T>())
    }
}

impl<T> Hash for Shared<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (Arc::as_ptr(&self.0) as usize).hash(state);
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Restored(Result<Shared<Session>, String>),
    Login(login::Message),
    LoggedIn(Result<Shared<Session>, String>),
    Ready(Result<(Shared<RoomList>, bool), String>),
    Rooms(RoomListState),
    SelectSpace(Option<OwnedRoomId>),
    OpenRoom(OwnedRoomId),
    RoomOpened(OwnedRoomId, Result<Shared<RoomTimeline>, String>),
    Timeline(OwnedRoomId, Vec<TimelineEntry>),
    Scrolled(Viewport),
    Paginated(OwnedRoomId, Result<bool, String>),
    Composer(text_editor::Action),
    Send,
    Sent(OwnedRoomId, Result<(), String>),
    Later(send_later::Message),
    Scheduled(Vec<ScheduledMessage>),
    /// Refreshes times shown relative to now.
    Tick,
    ShowSettings(bool),
    Settings(settings::Message),
    AddMenu(bool),
    RoomMenu(bool),
    OpenDialog(dialog::Kind),
    Dialog(dialog::Message),
    /// Accept (`true`) or decline an invite.
    AnswerInvite(OwnedRoomId, bool),
    InviteAnswered(OwnedRoomId, bool, Result<(), String>),
    CloseNotice,
    /// Fast ticks while something is in progress.
    Progress,
    Calls(OwnedRoomId, calls::Message),
    RecoveryKey(String),
    Recover,
    RecoveryProgress(RecoveryStep),
    Recovered(Result<bool, String>),
    MathRendered(math::Key, Rendered),
    LinkClicked(String),
    WindowResized(Size),
    ScaleFactor(f32),
    ThemeMode(Mode),
    AvatarLoaded(avatar::Key, Option<image::Handle>),
    /// The cursor entered (`true`) or left a timeline event.
    Hover(String, bool),
}

pub struct App {
    profile: Profile,
    /// Connection for creating Google Meet links, shared by all accounts.
    google: Arc<Google>,
    settings: Settings,
    /// Where settings are saved; `None` keeps them in memory (tests).
    settings_file: Option<PathBuf>,
    /// The settings panel, while open.
    settings_panel: Option<settings::Panel>,
    screen: Screen,
    mode: Mode,
    window_width: f32,
    scale: f32,
    math: Math,
}

enum Screen {
    Starting,
    Login(Login),
    Main(Box<Main>),
}

struct Main {
    session: Arc<Session>,
    room_list: Option<Arc<RoomList>>,
    rooms: RoomListState,
    space: Option<OwnedRoomId>,
    room: Option<Room>,
    /// `None` until known.
    verified: Option<bool>,
    recovery_key: String,
    /// Set while recovery runs.
    recovery_step: Option<RecoveryStep>,
    recovery_error: Option<String>,
    /// Scheduled messages of all rooms, earliest first.
    scheduled: Vec<ScheduledMessage>,
    /// When scheduled times were last compared to the clock.
    now: SystemTime,
    error: Option<String>,
    avatars: Avatars,
    dialog: Option<Dialog>,
    /// A dialog closed while its request runs: notices for success and failure.
    background: Option<(String, String)>,
    /// The room list's "+" menu is open.
    add_menu: bool,
    /// The invite shown instead of a room, and whether it is being answered.
    invite: Option<(OwnedRoomId, bool)>,
    /// A room being opened (its timeline loads).
    opening: Option<OwnedRoomId>,
    /// Something finished in the background, or failed.
    notice: Option<Notice>,
}

struct Notice {
    text: String,
    room: Option<OwnedRoomId>,
    error: bool,
}

impl Main {
    fn new(session: Arc<Session>) -> Self {
        Self {
            room_list: None,
            rooms: RoomListState::default(),
            space: None,
            room: None,
            verified: None,
            recovery_key: String::new(),
            recovery_step: None,
            recovery_error: None,
            scheduled: session.scheduled(),
            now: SystemTime::now(),
            session,
            error: None,
            avatars: Avatars::new(),
            dialog: None,
            background: None,
            add_menu: false,
            invite: None,
            opening: None,
            notice: None,
        }
    }
}

impl Main {
    /// Pictures of the spaces, listed rooms, and the open room's header and
    /// timeline senders, with their sizes.
    fn avatars_on_screen(&self) -> Vec<(OwnedMxcUri, f32)> {
        let rooms = &self.rooms.rooms;
        let mut wanted = sidebar::avatars(rooms, self.space.as_ref());
        if let Some(room) = &self.room {
            wanted.extend(room.avatars(rooms.iter().find(|r| r.id == room.id)));
        }
        wanted
            .into_iter()
            .map(|(mxc, size)| (mxc.to_owned(), size))
            .collect()
    }
}

/// Profile from the same environment variables the CLI reads.
/// Profile and data directory from the same environment variables the CLI reads.
fn environment() -> Result<(Profile, PathBuf, SecretStore), String> {
    let name = std::env::var("MTRX_PROFILE").unwrap_or_else(|_| "default".into());
    let data_root = match std::env::var_os("MTRX_DATA_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Profile::default_data_root().map_err(|e| e.to_string())?,
    };
    let secrets = match std::env::var("MTRX_SECRET_STORE").as_deref() {
        Ok("file") => SecretStore::File,
        _ => SecretStore::Keyring,
    };
    let profile = Profile::new(&name, Some(&data_root), secrets).map_err(|e| e.to_string())?;
    Ok((profile, data_root, secrets))
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let (profile, data_root, secrets) =
            environment().expect("valid MTRX_PROFILE and data directory");
        let settings_file = Settings::path(&data_root);
        let app = Self {
            profile: profile.clone(),
            google: Arc::new(Google::new(&data_root, secrets)),
            settings: Settings::load(&settings_file),
            settings_file: Some(settings_file),
            settings_panel: None,
            screen: Screen::Starting,
            mode: Mode::Light,
            window_width: 1200.0,
            scale: 1.0,
            math: Math::new(),
        };
        let restore = Task::perform(
            async move { Session::restore(profile).await.map(|s| Shared(Arc::new(s))) },
            |result| Message::Restored(result.map_err(|e| e.to_string())),
        );
        let scale = window::oldest().then(|id| match id {
            Some(id) => window::scale_factor(id).map(Message::ScaleFactor),
            None => Task::none(),
        });
        let mode = iced::system::theme().map(Message::ThemeMode);
        (app, Task::batch([restore, scale, mode]))
    }

    pub fn theme(&self) -> Theme {
        match self.mode {
            Mode::Dark => Theme::Dark,
            _ => Theme::Light,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Restored(Ok(Shared(session))) | Message::LoggedIn(Ok(Shared(session))) => {
                self.screen = Screen::Main(Box::new(Main::new(session.clone())));
                Task::perform(
                    async move {
                        session.start_sync().await;
                        session.start_scheduler();
                        let list = session.room_list().await?;
                        let verified = session.is_device_verified().await?;
                        Ok((Shared(Arc::new(list)), verified))
                    },
                    |result: mtrx_core::Result<_>| {
                        Message::Ready(result.map_err(|e| e.to_string()))
                    },
                )
            }
            Message::Restored(Err(_)) => {
                self.screen = Screen::Login(Login::default());
                Task::none()
            }
            Message::Login(message) => {
                let Screen::Login(login) = &mut self.screen else {
                    return Task::none();
                };
                let (profile, homeserver) = (self.profile.clone(), login.homeserver.clone());
                match login.update(message) {
                    login::Action::None => Task::none(),
                    login::Action::Password => {
                        let (user, password) = (login.user.clone(), login.password.clone());
                        Task::perform(
                            async move {
                                Session::login(profile, &homeserver, &user, &password)
                                    .await
                                    .map(|s| Shared(Arc::new(s)))
                            },
                            |result| Message::LoggedIn(result.map_err(|e| e.to_string())),
                        )
                    }
                    login::Action::Browser => {
                        let (task, handle) = Task::perform(
                            async move {
                                Session::login_oauth(profile, &homeserver, |url| {
                                    let _ = open::that_detached(url.as_str());
                                })
                                .await
                                .map(|s| Shared(Arc::new(s)))
                            },
                            |result| Message::LoggedIn(result.map_err(|e| e.to_string())),
                        )
                        .abortable();
                        login.browser_started(handle);
                        task
                    }
                }
            }
            Message::LoggedIn(Err(error)) => {
                if let Screen::Login(login) = &mut self.screen {
                    login.failed(error);
                }
                Task::none()
            }
            Message::Ready(result) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                match result {
                    Ok((Shared(list), verified)) => {
                        main.rooms = list.current();
                        main.room_list = Some(list);
                        main.verified = Some(verified);
                    }
                    Err(e) => main.error = Some(e),
                }
                self.load_avatars()
            }
            Message::Rooms(state) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.rooms = state;
                }
                self.load_avatars()
            }
            Message::SelectSpace(space) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.space = space;
                }
                self.load_avatars()
            }
            Message::OpenRoom(id) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                main.add_menu = false;
                if main.room.as_ref().is_some_and(|r| r.id == id) && main.invite.is_none() {
                    return Task::none();
                }
                // An invite shows Accept and Decline instead of a timeline.
                if main.rooms.rooms.iter().any(|r| r.id == id && r.invited) {
                    main.invite = Some((id, false));
                    main.room = None;
                    return Task::none();
                }
                main.invite = None;
                main.opening = Some(id.clone());
                let session = main.session.clone();
                Task::perform(
                    async move {
                        let timeline = session.timeline(&id).await.map(|t| Shared(Arc::new(t)));
                        (id, timeline.map_err(|e| e.to_string()))
                    },
                    |(id, result)| Message::RoomOpened(id, result),
                )
            }
            Message::RoomOpened(id, result) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                if main.opening.as_ref() != Some(&id) {
                    // Another room was chosen meanwhile.
                    return Task::none();
                }
                main.opening = None;
                match result {
                    Ok(Shared(timeline)) => {
                        let mut room = Room::new(id.clone(), timeline);
                        let paginate = paginate(&mut room);
                        let read = mark_read(&room);
                        main.room = Some(room);
                        Task::batch([
                            paginate,
                            read,
                            self.render_math(),
                            self.load_avatars(),
                            self.load_room_calls(&id),
                        ])
                    }
                    Err(e) => {
                        main.error = Some(e);
                        Task::none()
                    }
                }
            }
            Message::Timeline(id, entries) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let Some(room) = main.room.as_mut().filter(|r| r.id == id) else {
                    return Task::none();
                };
                let grew = entries.len() != room.entries.len();
                room.entries = entries;
                let read = if grew { mark_read(room) } else { Task::none() };
                Task::batch([read, self.render_math(), self.load_avatars()])
            }
            Message::Scrolled(viewport) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let Some(room) = main.room.as_mut() else {
                    return Task::none();
                };
                match room.older(&viewport) {
                    Older::None => Task::none(),
                    Older::ShowMore => {
                        room.show_more();
                        Task::batch([self.render_math(), self.load_avatars()])
                    }
                    Older::Paginate => paginate(room),
                }
            }
            Message::Paginated(id, result) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                if let Some(room) = main.room.as_mut().filter(|r| r.id == id) {
                    room.paginating = false;
                    match result {
                        Ok(at_start) => {
                            room.at_start = at_start;
                            room.show_more();
                        }
                        Err(e) => room.error = Some(e),
                    }
                }
                Task::batch([self.render_math(), self.load_avatars()])
            }
            Message::Composer(action) => {
                if let Screen::Main(main) = &mut self.screen
                    && let Some(room) = main.room.as_mut()
                {
                    room.composer.perform(action);
                }
                Task::none()
            }
            Message::Send => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let Some(room) = main.room.as_mut() else {
                    return Task::none();
                };
                let text = room.composer.text();
                if text.trim().is_empty() {
                    return Task::none();
                }
                if let Some(editing) = &room.later.editing {
                    // Saving keeps the time unless it has passed.
                    return if editing.send_at > SystemTime::now() {
                        let send_at = editing.send_at;
                        self.later(send_later::Message::At(send_at))
                    } else {
                        room.later.picker.get_or_insert_default();
                        room.error = Some("Its time has passed: pick a new one.".into());
                        Task::none()
                    };
                }
                room.composer = text_editor::Content::new();
                room.error = None;
                let (id, timeline) = (room.id.clone(), room.timeline.clone());
                // Queue only: the local echo shows the send state.
                Task::perform(
                    async move { timeline.send_markdown(text.trim_end()).await.map(|_| ()) },
                    move |result| Message::Sent(id.clone(), result.map_err(|e| e.to_string())),
                )
            }
            Message::Sent(id, result) => {
                if let Screen::Main(main) = &mut self.screen
                    && let Some(room) = main.room.as_mut().filter(|r| r.id == id)
                    && let Err(e) = result
                {
                    room.error = Some(e);
                }
                Task::none()
            }
            Message::Later(message) => self.later(message),
            Message::ShowSettings(false) => {
                self.settings_panel = None;
                Task::none()
            }
            Message::ShowSettings(true) => self.open_settings(),
            Message::Settings(message) => self.settings_update(message),
            Message::Calls(room, message) => self.calls(room, message),
            Message::AddMenu(open) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.add_menu = open;
                }
                Task::none()
            }
            Message::RoomMenu(open) => {
                if let Screen::Main(main) = &mut self.screen
                    && let Some(room) = &mut main.room
                {
                    room.menu = open;
                }
                Task::none()
            }
            Message::OpenDialog(kind) => {
                self.settings_panel = None;
                if let Screen::Main(main) = &mut self.screen {
                    main.add_menu = false;
                    if let Some(room) = &mut main.room {
                        room.menu = false;
                    }
                    main.dialog = Some(Dialog::new(kind));
                }
                Task::none()
            }
            Message::Dialog(message) => self.dialog(message),
            Message::AnswerInvite(id, accept) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                main.invite = Some((id.clone(), true));
                let session = main.session.clone();
                Task::perform(
                    {
                        let id = id.clone();
                        async move {
                            if accept {
                                session.join(<&RoomOrAliasId>::from(&*id)).await.map(|_| ())
                            } else {
                                session.leave(&id).await
                            }
                        }
                    },
                    move |result| {
                        Message::InviteAnswered(
                            id.clone(),
                            accept,
                            result.map_err(|e| e.to_string()),
                        )
                    },
                )
            }
            Message::InviteAnswered(id, accept, result) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                if main.invite.as_ref().is_some_and(|(i, _)| *i == id) {
                    main.invite = None;
                }
                match result {
                    Ok(()) if accept => self.update(Message::OpenRoom(id)),
                    Ok(()) => Task::none(),
                    Err(e) => {
                        main.notice = Some(Notice {
                            text: format!(
                                "Could not {} the invite: {e}",
                                if accept { "accept" } else { "decline" }
                            ),
                            room: None,
                            error: true,
                        });
                        Task::none()
                    }
                }
            }
            Message::CloseNotice => {
                if let Screen::Main(main) = &mut self.screen {
                    main.notice = None;
                }
                Task::none()
            }
            Message::Progress => {
                if let Screen::Main(main) = &mut self.screen
                    && let Some(dialog) = &mut main.dialog
                {
                    dialog.tick(Instant::now());
                }
                Task::none()
            }
            Message::Scheduled(scheduled) => {
                if let Screen::Main(main) = &mut self.screen {
                    if let Some(room) = main.room.as_mut()
                        && let Some(editing) = &room.later.editing
                        && !scheduled.iter().any(|m| m.id == editing.id)
                    {
                        // Sent (it became due) or cancelled elsewhere; the
                        // edits stay in the composer and can be sent anew.
                        room.later.editing = None;
                        room.error = Some(
                            "The scheduled message was sent before your changes were saved.".into(),
                        );
                    }
                    main.scheduled = scheduled;
                    main.now = SystemTime::now();
                }
                Task::none()
            }
            Message::Tick => {
                if let Screen::Main(main) = &mut self.screen {
                    main.now = SystemTime::now();
                }
                Task::none()
            }
            Message::RecoveryKey(key) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.recovery_key = key;
                }
                Task::none()
            }
            Message::Recover => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                if main.recovery_step.is_some() || main.recovery_key.trim().is_empty() {
                    return Task::none();
                }
                main.recovery_step = Some(RecoveryStep::CheckingKey);
                main.recovery_error = None;
                let (session, key) = (main.session.clone(), main.recovery_key.trim().to_owned());
                Task::run(
                    iced::stream::channel(16, async move |mut tx| {
                        let progress = |step| {
                            let _ = tx.clone().try_send(Message::RecoveryProgress(step));
                        };
                        let result = async {
                            session.recover(&key, progress).await?;
                            session.is_device_verified().await
                        }
                        .await;
                        let _ = tx
                            .send(Message::Recovered(result.map_err(|e| e.to_string())))
                            .await;
                    }),
                    |message| message,
                )
            }
            Message::RecoveryProgress(step) => {
                if let Screen::Main(main) = &mut self.screen
                    && main.recovery_step.is_some()
                {
                    main.recovery_step = Some(step);
                }
                Task::none()
            }
            Message::Recovered(result) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.recovery_step = None;
                    match result {
                        Ok(verified) => {
                            main.verified = Some(verified);
                            main.recovery_key.clear();
                        }
                        Err(e) => main.recovery_error = Some(e),
                    }
                }
                Task::none()
            }
            Message::MathRendered(key, rendered) => {
                self.math.insert(key, rendered);
                Task::none()
            }
            Message::LinkClicked(url) => {
                let _ = open::that_detached(url);
                Task::none()
            }
            Message::WindowResized(size) => {
                self.window_width = size.width;
                self.render_math()
            }
            Message::ScaleFactor(scale) => {
                self.scale = scale;
                self.render_math()
            }
            Message::ThemeMode(mode) => {
                self.mode = mode;
                self.render_math()
            }
            Message::AvatarLoaded(key, handle) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.avatars.insert(key, handle);
                }
                Task::none()
            }
            Message::Hover(id, entered) => {
                if let Screen::Main(main) = &mut self.screen
                    && let Some(room) = main.room.as_mut()
                {
                    room.hover(id, entered);
                }
                Task::none()
            }
        }
    }

    fn dialog(&mut self, message: dialog::Message) -> Task<Message> {
        let Screen::Main(main) = &mut self.screen else {
            return Task::none();
        };
        let Some(dialog) = &mut main.dialog else {
            // The dialog was closed while its request ran.
            if let dialog::Message::Done(result) = message {
                let (done, failed) = main.background.take().unwrap_or_default();
                main.notice = Some(match result {
                    Ok(opened) => Notice {
                        text: done,
                        room: match opened {
                            dialog::Opened::Room(id) | dialog::Opened::Space(id) => Some(id),
                            _ => None,
                        },
                        error: false,
                    },
                    Err(e) => Notice {
                        text: format!("{failed}: {e}"),
                        room: None,
                        error: true,
                    },
                });
            }
            return Task::none();
        };
        let session = main.session.clone();
        match dialog.update(message) {
            dialog::Action::None => Task::none(),
            dialog::Action::Close => {
                main.dialog = None;
                Task::none()
            }
            dialog::Action::Background => {
                main.background = Some(dialog.outcome());
                main.dialog = None;
                Task::none()
            }
            dialog::Action::Search(query) => {
                let (task, handle) = Task::perform(
                    {
                        let query = query.clone();
                        async move { session.search_users(&query, 10).await.unwrap_or_default() }
                    },
                    move |users| {
                        Message::Dialog(dialog::Message::Suggestions(query.clone(), users))
                    },
                )
                .abortable();
                dialog.search = Some(handle);
                task
            }
            dialog::Action::Submit(request) => Task::perform(run_request(session, request), |r| {
                Message::Dialog(dialog::Message::Done(r))
            }),
            dialog::Action::Finished(opened) => {
                main.dialog = None;
                match opened {
                    dialog::Opened::Nothing => Task::none(),
                    dialog::Opened::Room(id) => self.update(Message::OpenRoom(id)),
                    dialog::Opened::Space(id) => self.update(Message::SelectSpace(Some(id))),
                    dialog::Opened::Left(id) => {
                        if main.room.as_ref().is_some_and(|r| r.id == id) {
                            main.room = None;
                        }
                        if main.space.as_ref() == Some(&id) {
                            main.space = None;
                        }
                        Task::none()
                    }
                    dialog::Opened::SignedOut => {
                        self.screen = Screen::Login(Login::default());
                        Task::none()
                    }
                }
            }
        }
    }

    fn open_settings(&mut self) -> Task<Message> {
        let mut panel = settings::Panel::default();
        panel.google.account = self.google.account().unwrap_or_else(|e| {
            panel.google.error = Some(e.to_string());
            None
        });
        let mut tasks = Vec::new();
        if let Screen::Main(main) = &self.screen {
            panel.account = Some(main.session.user_id().to_string());
            let name = |id: &OwnedRoomId| {
                main.rooms
                    .rooms
                    .iter()
                    .find(|r| &r.id == id)
                    .map_or_else(|| id.to_string(), |r| r.name.clone())
            };
            if let Some(room) = &main.room {
                panel.room = Some(settings::Access::new(
                    room.id.clone(),
                    name(&room.id),
                    false,
                ));
                tasks.push(load_access(main.session.clone(), room.id.clone()));
            }
            if let Some(space) = &main.space {
                panel.space = Some(settings::SpaceCalls::new(space.clone(), name(space)));
                tasks.push(load_access(main.session.clone(), space.clone()));
                let (session, space) = (main.session.clone(), space.clone());
                tasks.push(Task::perform(
                    async move {
                        let service = session.space_call_service(&space).await.ok().flatten();
                        let can_set = session.can_set_space_call_service(&space).await.ok();
                        settings::Message::SpaceLoaded(space, service, can_set)
                    },
                    Message::Settings,
                ));
            }
        }
        self.settings_panel = Some(panel);
        Task::batch(tasks)
    }

    fn settings_update(&mut self, message: settings::Message) -> Task<Message> {
        let Some(panel) = &mut self.settings_panel else {
            return Task::none();
        };
        match panel.update(&mut self.settings, message) {
            settings::Action::None => Task::none(),
            settings::Action::Save => {
                panel.error = self
                    .settings_file
                    .as_ref()
                    .and_then(|file| self.settings.save(file).err())
                    .map(|e| e.to_string());
                Task::none()
            }
            settings::Action::GoogleSignIn(client) => {
                let google = self.google.clone();
                let (task, handle) = Task::perform(
                    async move {
                        google
                            .sign_in(client, |url| {
                                let _ = open::that_detached(url.as_str());
                            })
                            .await
                    },
                    |result| {
                        Message::Settings(settings::Message::GoogleSignedIn(
                            result.map_err(|e| e.to_string()),
                        ))
                    },
                )
                .abortable();
                panel.google.signing_in = Some(handle);
                task
            }
            settings::Action::GoogleSignOut => {
                let google = self.google.clone();
                Task::perform(async move { google.sign_out().await }, |result| {
                    Message::Settings(settings::Message::GoogleSignedOut(
                        result.map_err(|e| e.to_string()),
                    ))
                })
            }
            settings::Action::LetEveryoneEdit { id, space } => {
                let Screen::Main(main) = &self.screen else {
                    return Task::none();
                };
                let session = main.session.clone();
                Task::perform(
                    {
                        let id = id.clone();
                        async move {
                            if space {
                                session.let_everyone_edit_space(&id).await.map(|update| {
                                    let skipped = match update.not_allowed.len() {
                                        0 => String::new(),
                                        n => format!(
                                            " {n} could not be changed: you are not an admin there."
                                        ),
                                    };
                                    format!("Opened {} room(s).{skipped}", update.changed.len())
                                })
                            } else {
                                session
                                    .let_everyone_edit(&id)
                                    .await
                                    .map(|()| "Done.".to_owned())
                            }
                        }
                    },
                    move |result| {
                        Message::Settings(settings::Message::Opened(
                            id.clone(),
                            result.map_err(|e| e.to_string()),
                        ))
                    },
                )
            }
            settings::Action::SetSpaceService(space, service) => {
                let Screen::Main(main) = &self.screen else {
                    return Task::none();
                };
                let session = main.session.clone();
                Task::perform(
                    async move { session.set_space_call_service(&space, service).await },
                    |result| {
                        Message::Settings(settings::Message::SpaceSaved(
                            result.map_err(|e| e.to_string()),
                        ))
                    },
                )
            }
        }
    }

    fn calls(&mut self, room_id: OwnedRoomId, message: calls::Message) -> Task<Message> {
        use calls::Message as Calls;
        let Screen::Main(main) = &mut self.screen else {
            return Task::none();
        };
        let Some(room) = main.room.as_mut().filter(|r| r.id == room_id) else {
            return Task::none();
        };
        let session = main.session.clone();
        let google = self.google.clone();
        let state = &mut room.calls;
        let reply = move |message| Message::Calls(room_id.clone(), message);
        match message {
            Calls::Link(link) => state.link = link,
            Calls::CanSetLink(can_set) => state.can_set_link = can_set,
            Calls::SpaceService(service) => state.space_service = service,
            Calls::ToggleMenu => {
                state.menu = !state.menu;
                state.editor = None;
                state.error = None;
            }
            Calls::Start(service) => {
                state.busy = true;
                state.menu = false;
                state.error = None;
                let room = room.id.clone();
                return Task::perform(
                    async move {
                        let link = new_call_link(&service, &google).await?;
                        session
                            .post_call(&room, &service.name, &link)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |result| reply(Calls::Started(result)),
                );
            }
            Calls::Started(result) | Calls::LinkSaved(result) => {
                state.busy = false;
                match result {
                    Ok(()) => state.editor = None,
                    Err(e) => state.error = Some(e),
                }
            }
            Calls::EditLink => {
                state.menu = false;
                state.error = None;
                state.editor = Some(state.link.as_ref().map(Url::to_string).unwrap_or_default());
            }
            Calls::LinkInput(input) => state.editor = Some(input),
            Calls::NewLink(service) => {
                state.editor = service.new_link().map(|link| link.to_string());
            }
            Calls::SaveLink | Calls::RemoveLink => {
                let link = match message {
                    Calls::SaveLink => match state.editor.as_deref().map(str::trim).map(Url::parse)
                    {
                        Some(Ok(link)) => Some(link),
                        _ => return Task::none(),
                    },
                    _ => None,
                };
                state.busy = true;
                state.error = None;
                let room = room.id.clone();
                return Task::perform(
                    async move {
                        session
                            .set_call_link(&room, link.as_ref())
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |result| reply(Calls::LinkSaved(result)),
                );
            }
            Calls::CancelEdit => {
                state.editor = None;
                state.error = None;
            }
        }
        Task::none()
    }

    /// Permission and space call service of a newly opened room.
    fn load_room_calls(&self, room_id: &OwnedRoomId) -> Task<Message> {
        let Screen::Main(main) = &self.screen else {
            return Task::none();
        };
        let session = main.session.clone();
        let spaces = sidebar::ancestors(&main.rooms.rooms, room_id);
        let room = room_id.clone();
        let reply = {
            let room = room.clone();
            move |message| Message::Calls(room.clone(), message)
        };
        let can_set = {
            let (session, room) = (session.clone(), room.clone());
            Task::perform(
                async move { session.can_set_call_link(&room).await.ok() },
                {
                    let reply = reply.clone();
                    move |can_set| reply(calls::Message::CanSetLink(can_set))
                },
            )
        };
        let space_service = Task::perform(
            async move {
                for space in spaces {
                    if let Ok(Some(service)) = session.space_call_service(&space).await {
                        return Some(service);
                    }
                }
                None
            },
            move |service| reply(calls::Message::SpaceService(service)),
        );
        Task::batch([can_set, space_service])
    }

    fn later(&mut self, message: send_later::Message) -> Task<Message> {
        use send_later::Message as Later;
        let Screen::Main(main) = &mut self.screen else {
            return Task::none();
        };
        let Some(room) = main.room.as_mut() else {
            return Task::none();
        };
        let session = main.session.clone();
        let done = |result: mtrx_core::Result<()>| {
            Message::Later(Later::Done(result.map_err(|e| e.to_string())))
        };
        match message {
            Later::TogglePicker => {
                room.later.picker = match room.later.picker {
                    Some(_) => None,
                    None => Some(String::new()),
                };
                Task::none()
            }
            Later::Custom(custom) => {
                room.later.picker = Some(custom);
                Task::none()
            }
            Later::SubmitCustom => match room.later.custom_time(SystemTime::now()) {
                Some(at) => self.later(Later::At(at)),
                None => Task::none(),
            },
            Later::At(send_at) => {
                let text = room.composer.text().trim_end().to_owned();
                if text.trim().is_empty() {
                    return Task::none();
                }
                room.error = None;
                let room_id = room.id.clone();
                let editing = room.later.editing.as_ref().map(|m| m.id.clone());
                // The composer is cleared once saved, so nothing is lost on errors.
                Task::perform(
                    async move {
                        match editing {
                            Some(id) => session.reschedule(&id, &text, send_at).await,
                            None => session.schedule(&room_id, &text, send_at).await.map(|_| ()),
                        }
                    },
                    |result| Message::Later(Later::Saved(result.map_err(|e| e.to_string()))),
                )
            }
            Later::Edit(message) => {
                room.composer = text_editor::Content::with_text(&message.text);
                room.composer
                    .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
                // An overdue message needs a new time before it can be saved.
                room.later.picker = message.overdue.then(String::new);
                room.later.editing = Some(message);
                Task::none()
            }
            Later::CancelEdit => {
                room.later = Default::default();
                room.composer = text_editor::Content::new();
                room.error = None;
                Task::none()
            }
            Later::SendNow(id) => Task::perform(
                async move { session.send_scheduled_now(&id).await.map(|_| ()) },
                done,
            ),
            Later::Cancel(id) => Task::perform(
                async move { session.unschedule(&id).await.map(|_| ()) },
                done,
            ),
            Later::Saved(Ok(())) => {
                room.later = Default::default();
                room.composer = text_editor::Content::new();
                Task::none()
            }
            Later::Saved(Err(e)) | Later::Done(Err(e)) => {
                room.error = Some(e);
                Task::none()
            }
            Later::Done(Ok(())) => Task::none(),
        }
    }

    fn math_params(&self) -> math::Params {
        let theme = self.theme();
        let palette = theme.extended_palette();
        let max_width = self.window_width - SPACE_BAR_WIDTH - ROOM_LIST_WIDTH - MESSAGE_INSET;
        math::Params {
            max_width: max_width.max(200.0),
            scale: self.scale,
            text: palette.background.base.text,
            link: palette.primary.base.color,
            muted: style::muted(&theme),
        }
    }

    /// Request images for all math messages of the open room.
    fn render_math(&mut self) -> Task<Message> {
        let params = self.math_params();
        let Screen::Main(main) = &self.screen else {
            return Task::none();
        };
        let Some(room) = &main.room else {
            return Task::none();
        };
        let tasks: Vec<_> = room
            .math_documents()
            .map(|doc| self.math.request(doc, &params, Message::MathRendered))
            .collect();
        Task::batch(tasks)
    }

    /// Request the pictures on screen.
    fn load_avatars(&mut self) -> Task<Message> {
        let Screen::Main(main) = &mut self.screen else {
            return Task::none();
        };
        let tasks: Vec<_> = main
            .avatars_on_screen()
            .into_iter()
            .map(|(mxc, size)| {
                main.avatars
                    .request(&main.session, &mxc, size, Message::AvatarLoaded)
            })
            .collect();
        Task::batch(tasks)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            window::resize_events().map(|(_, size)| Message::WindowResized(size)),
            iced::system::theme_changes().map(Message::ThemeMode),
        ];
        if let Screen::Main(main) = &self.screen {
            subscriptions.push(
                Subscription::run_with(Shared(main.session.clone()), |s| s.0.scheduled_updates())
                    .map(Message::Scheduled),
            );
            if !main.scheduled.is_empty() {
                subscriptions.push(iced::time::every(TICK).map(|_| Message::Tick));
            }
            if main.dialog.as_ref().is_some_and(Dialog::busy) {
                subscriptions.push(iced::time::every(PROGRESS_TICK).map(|_| Message::Progress));
            }
            if let Some(list) = &main.room_list {
                subscriptions.push(
                    Subscription::run_with(Shared(list.clone()), |list| list.0.updates())
                        .map(Message::Rooms),
                );
            }
            if let Some(room) = &main.room {
                subscriptions.push(Subscription::run_with(
                    (Shared(main.session.clone()), room.id.clone()),
                    |(session, room)| {
                        let (session, room) = (session.0.clone(), room.clone());
                        iced::stream::channel(4, async move |mut tx| {
                            let Ok(links) = session.call_link_updates(&room).await else {
                                return;
                            };
                            let mut links = std::pin::pin!(links);
                            while let Some(link) = links.next().await {
                                let message =
                                    Message::Calls(room.clone(), calls::Message::Link(link));
                                if tx.send(message).await.is_err() {
                                    return;
                                }
                            }
                        })
                    },
                ));
                subscriptions.push(Subscription::run_with(Shared(room.timeline.clone()), |t| {
                    let id = t.0.room_id().clone();
                    t.0.updates()
                        .map(move |entries| Message::Timeline(id.clone(), entries))
                }));
            }
        }
        Subscription::batch(subscriptions)
    }

    pub fn view(&self) -> Element<'_, Message> {
        match &self.screen {
            Screen::Starting => container(text("Starting…")).center(Length::Fill).into(),
            Screen::Login(login) => login.view().map(Message::Login),
            Screen::Main(main) => {
                let (panel, close) = if let Some(panel) = &self.settings_panel {
                    (panel.view(&self.settings), Message::ShowSettings(false))
                } else if let Some(dialog) = &main.dialog {
                    (
                        dialog.view(&main.rooms.rooms),
                        Message::Dialog(dialog::Message::Cancel),
                    )
                } else {
                    return self.main_view(main);
                };
                let backdrop = mouse_area(
                    container(opaque(panel))
                        .center(Length::Fill)
                        .style(style::backdrop),
                )
                .on_press(close);
                stack![self.main_view(main), opaque(backdrop)].into()
            }
        }
    }

    fn main_view<'a>(&'a self, main: &'a Main) -> Element<'a, Message> {
        let theme = self.theme();
        let rooms = &main.rooms.rooms;
        let space_bar = sidebar::space_bar(rooms, main.space.as_ref(), &main.avatars);
        let room_list = sidebar::room_list(
            rooms,
            main.space.as_ref(),
            main.opening
                .as_ref()
                .or(main.invite.as_ref().map(|(id, _)| id))
                .or(main.room.as_ref().map(|r| &r.id)),
            main.rooms.loaded,
            &main.avatars,
            &main.scheduled,
            main.add_menu,
        );

        let mut content = column![];
        if main.verified == Some(false) {
            content = content.push(recovery_banner(main));
        }
        if let Some(banner) = overdue_banner(main) {
            content = content.push(banner);
        }
        if let Some(notice) = &main.notice {
            content = content.push(notice_bar(notice));
        }
        if let Some(error) = &main.error {
            content = content.push(container(text(error).style(text::danger)).padding(12));
        }
        let room_view: Element<'a, Message> = match (&main.invite, &main.opening, &main.room) {
            (Some((id, answering)), _, _) => {
                invite_view(rooms.iter().find(|r| &r.id == id), id, *answering)
            }
            (None, Some(_), _) => container(text("Opening…").style(text::secondary))
                .center(Length::Fill)
                .into(),
            (None, None, Some(room)) => {
                let summary = rooms.iter().find(|r| r.id == room.id);
                let scheduled = send_later::Pending {
                    messages: main
                        .scheduled
                        .iter()
                        .filter(|m| m.room_id == room.id)
                        .collect(),
                    now: main.now,
                };
                let look = Look {
                    theme: theme.clone(),
                    math: &self.math,
                    params: self.math_params(),
                    avatars: &main.avatars,
                    colored_names: self.settings.colored_names,
                    call_services: &self.settings.call_services,
                };
                room.view(summary, &look, &scheduled)
            }
            (None, None, None) => container(text("Select a room").style(text::secondary))
                .center(Length::Fill)
                .into(),
        };
        content = content.push(room_view);

        row![space_bar, room_list, content.width(Length::Fill)].into()
    }
}

fn recovery_banner(main: &Main) -> Element<'_, Message> {
    let busy = main.recovery_step.is_some();
    let mut input = text_input("Recovery key", &main.recovery_key)
        .style(style::input)
        .secure(true)
        .padding(8)
        .width(320);
    if !busy {
        input = input
            .on_input(Message::RecoveryKey)
            .on_submit(Message::Recover);
    }
    let unlock = button(text(if busy { "Unlocking…" } else { "Unlock" }))
        .style(style::primary_button)
        .on_press_maybe((!busy).then_some(Message::Recover));
    let mut banner = column![
        text("This device is not verified, so older encrypted messages cannot be read."),
        row![input, unlock].spacing(8),
    ]
    .spacing(8);
    if let Some(step) = main.recovery_step {
        let status = match step {
            RecoveryStep::CheckingKey => "Checking the recovery key…".to_owned(),
            RecoveryStep::ImportingIdentity => {
                "Verifying this device and connecting to the key backup…".to_owned()
            }
            RecoveryStep::DownloadingKeys { done, total } => {
                format!("Downloading message keys for recent rooms: {done} of {total}…")
            }
        };
        banner = banner.push(text(status).size(13));
    }
    if let Some(error) = &main.recovery_error {
        banner = banner.push(text(error).style(text::danger));
    }
    container(
        container(banner)
            .padding(12)
            .width(Length::Fill)
            .style(style::banner),
    )
    .padding([8, 24])
    .into()
}

/// Points to rooms (other than the open one) with overdue scheduled
/// messages; their composer bar offers sending or cancelling them.
fn overdue_banner(main: &Main) -> Option<Element<'_, Message>> {
    let open = main.room.as_ref().map(|r| &r.id);
    let mut rooms: Vec<(&OwnedRoomId, usize)> = Vec::new();
    for message in main.scheduled.iter().filter(|m| m.overdue) {
        if Some(&message.room_id) == open {
            continue;
        }
        match rooms.iter_mut().find(|(id, _)| *id == &message.room_id) {
            Some((_, count)) => *count += 1,
            None => rooms.push((&message.room_id, 1)),
        }
    }
    if rooms.is_empty() {
        return None;
    }
    let mut banner = row![text("Scheduled messages were not sent on time:")]
        .spacing(8)
        .align_y(iced::Alignment::Center);
    for (id, count) in rooms {
        let name = main
            .rooms
            .rooms
            .iter()
            .find(|r| &r.id == id)
            .map_or(id.as_str(), |r| r.name.as_str());
        let label = if count > 1 {
            format!("{name} ({count})")
        } else {
            name.to_owned()
        };
        banner = banner.push(
            button(text(label).size(13))
                .padding([2, 8])
                .style(style::secondary_button)
                .on_press(Message::OpenRoom(id.clone())),
        );
    }
    Some(
        container(
            container(banner)
                .padding([8, 12])
                .width(Length::Fill)
                .style(style::banner),
        )
        .padding([8, 24])
        .into(),
    )
}

/// Something finished in the background, or failed.
fn notice_bar(notice: &Notice) -> Element<'_, Message> {
    let mut bar = row![
        text(&notice.text)
            .size(13)
            .style(if notice.error {
                text::danger
            } else {
                text::default
            })
            .width(Length::Fill)
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    if let Some(room) = &notice.room {
        bar = bar.push(
            button(text("Open").size(13))
                .padding([3, 10])
                .style(style::primary_button)
                .on_press(Message::OpenRoom(room.clone())),
        );
    }
    bar = bar.push(
        button(text("Dismiss").size(13))
            .padding([3, 8])
            .style(style::text_button)
            .on_press(Message::CloseNotice),
    );
    container(container(bar).padding([6, 12]).style(style::code_block))
        .padding([8, 24])
        .into()
}

/// An invite, with Accept and Decline.
fn invite_view<'a>(
    summary: Option<&'a mtrx_core::RoomSummary>,
    id: &'a OwnedRoomId,
    answering: bool,
) -> Element<'a, Message> {
    let name = summary.map_or(id.as_str(), |s| s.name.as_str());
    let actions: Element<'a, Message> = if answering {
        text("Answering the invite…").style(text::secondary).into()
    } else {
        row![
            button(text("Decline"))
                .style(style::secondary_button)
                .on_press(Message::AnswerInvite(id.clone(), false)),
            button(text("Accept"))
                .style(style::primary_button)
                .on_press(Message::AnswerInvite(id.clone(), true)),
        ]
        .spacing(8)
        .into()
    };
    container(
        column![
            text(format!("You are invited to {name}"))
                .size(18)
                .font(crate::fonts::SEMIBOLD),
            text("Its messages show once you accept.").style(text::secondary),
            actions,
        ]
        .spacing(12)
        .align_x(iced::Alignment::Center),
    )
    .center(Length::Fill)
    .into()
}

/// Carry out a dialog's request.
async fn run_request(
    session: Arc<Session>,
    request: dialog::Request,
) -> Result<dialog::Opened, String> {
    use dialog::{Opened, Request};
    let result = async {
        Ok(match request {
            Request::CreateRoom(new) => Opened::Room(session.create_room(new).await?),
            Request::CreateSpace(new) => Opened::Space(session.create_room(new).await?),
            Request::DirectChat(user) => Opened::Room(session.direct_chat(&user).await?),
            Request::Join(address) => {
                let address = <&RoomOrAliasId>::try_from(address.as_str())
                    .map_err(|_| mtrx_core::Error::RoomNotFound(address.clone()))?;
                Opened::Room(session.join(address).await?)
            }
            Request::Invite(room, people) => {
                for person in &people {
                    session.invite(&room, person).await?;
                }
                Opened::Nothing
            }
            Request::Edit { room, name, topic } => {
                if let Some(name) = name {
                    session.set_room_name(&room, &name).await?;
                }
                if let Some(topic) = topic {
                    session.set_room_topic(&room, &topic).await?;
                }
                Opened::Nothing
            }
            Request::AddToSpace(space, rooms) => {
                for room in &rooms {
                    session.add_to_space(&space, room).await?;
                }
                Opened::Nothing
            }
            Request::Leave(room) => {
                session.leave(&room).await?;
                Opened::Left(room)
            }
            Request::SignOut => {
                session.logout().await?;
                Opened::SignedOut
            }
        })
    }
    .await;
    result.map_err(|e: mtrx_core::Error| e.to_string())
}

/// Who may edit `room`, and whether we may change that, for the settings panel.
fn load_access(session: Arc<Session>, room: OwnedRoomId) -> Task<Message> {
    Task::perform(
        async move {
            let permissions = session.permissions(&room).await.ok();
            let can_change = session.can_change_permissions(&room).await.ok();
            settings::Message::AccessLoaded(room, permissions, can_change)
        },
        Message::Settings,
    )
}

/// A link for a new call with `service`.
async fn new_call_link(service: &CallService, google: &Google) -> Result<Url, String> {
    match service.kind {
        ServiceKind::GoogleMeet => google.create_meeting().await.map_err(|e| match e {
            mtrx_core::Error::GoogleNotSignedIn => {
                "Connect your Google account in Settings to create Meet links.".to_owned()
            }
            e => e.to_string(),
        }),
        _ => service
            .new_link()
            .ok_or_else(|| format!("{} cannot make call links", service.name)),
    }
}

fn paginate(room: &mut Room) -> Task<Message> {
    if room.paginating || room.at_start {
        return Task::none();
    }
    room.paginating = true;
    let (id, timeline) = (room.id.clone(), room.timeline.clone());
    Task::perform(
        async move { timeline.paginate_back(30).await },
        move |result| Message::Paginated(id.clone(), result.map_err(|e| e.to_string())),
    )
}

fn mark_read(room: &Room) -> Task<Message> {
    let timeline = room.timeline.clone();
    Task::future(async move {
        let _ = timeline.mark_read().await;
    })
    .discard()
}

#[cfg(test)]
mod tests;
