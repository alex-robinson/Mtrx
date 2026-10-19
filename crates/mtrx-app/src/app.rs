//! Application state, messages and top-level layout.

use std::{
    fmt,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

use futures_util::{SinkExt, StreamExt};
use iced::{
    Element, Length, Size, Subscription, Task, Theme,
    theme::Mode,
    widget::{button, column, container, row, scrollable::Viewport, text, text_editor, text_input},
    window,
};
use mtrx_core::{
    OwnedRoomId, Profile, RecoveryStep, RoomList, RoomListState, RoomTimeline, SecretStore,
    Session, TimelineEntry,
};

use crate::{
    login::{self, Login},
    math::{self, Math, Rendered},
    room::{Older, Room},
    sidebar,
    style::{self, ROOM_LIST_WIDTH, SPACE_BAR_WIDTH, TIMELINE_PADDING},
};

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
    RecoveryKey(String),
    Recover,
    RecoveryProgress(RecoveryStep),
    Recovered(Result<bool, String>),
    MathRendered(math::Key, Rendered),
    LinkClicked(String),
    WindowResized(Size),
    ScaleFactor(f32),
    ThemeMode(Mode),
}

pub struct App {
    profile: Profile,
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
    error: Option<String>,
}

impl Main {
    fn new(session: Arc<Session>) -> Self {
        Self {
            session,
            room_list: None,
            rooms: RoomListState::default(),
            space: None,
            room: None,
            verified: None,
            recovery_key: String::new(),
            recovery_step: None,
            recovery_error: None,
            error: None,
        }
    }
}

/// Profile from the same environment variables the CLI reads.
fn profile() -> Result<Profile, String> {
    let name = std::env::var("MTRX_PROFILE").unwrap_or_else(|_| "default".into());
    let data_dir = std::env::var_os("MTRX_DATA_DIR").map(PathBuf::from);
    let secrets = match std::env::var("MTRX_SECRET_STORE").as_deref() {
        Ok("file") => SecretStore::File,
        _ => SecretStore::Keyring,
    };
    Profile::new(&name, data_dir.as_deref(), secrets).map_err(|e| e.to_string())
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let profile = profile().expect("valid MTRX_PROFILE");
        let app = Self {
            profile: profile.clone(),
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
                Task::none()
            }
            Message::Rooms(state) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.rooms = state;
                }
                Task::none()
            }
            Message::SelectSpace(space) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.space = space;
                }
                Task::none()
            }
            Message::OpenRoom(id) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                if main.room.as_ref().is_some_and(|r| r.id == id) {
                    return Task::none();
                }
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
                match result {
                    Ok(Shared(timeline)) => {
                        let mut room = Room::new(id, timeline);
                        let paginate = paginate(&mut room);
                        let read = mark_read(&room);
                        main.room = Some(room);
                        Task::batch([paginate, read, self.render_math()])
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
                Task::batch([read, self.render_math()])
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
                        self.render_math()
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
                self.render_math()
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
        }
    }

    fn math_params(&self) -> math::Params {
        let theme = self.theme();
        let palette = theme.extended_palette();
        let max_width = self.window_width - SPACE_BAR_WIDTH - ROOM_LIST_WIDTH - TIMELINE_PADDING;
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

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            window::resize_events().map(|(_, size)| Message::WindowResized(size)),
            iced::system::theme_changes().map(Message::ThemeMode),
        ];
        if let Screen::Main(main) = &self.screen {
            if let Some(list) = &main.room_list {
                subscriptions.push(
                    Subscription::run_with(Shared(list.clone()), |list| list.0.updates())
                        .map(Message::Rooms),
                );
            }
            if let Some(room) = &main.room {
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
            Screen::Main(main) => self.main_view(main),
        }
    }

    fn main_view<'a>(&'a self, main: &'a Main) -> Element<'a, Message> {
        let theme = self.theme();
        let rooms = &main.rooms.rooms;
        let space_bar = sidebar::space_bar(rooms, main.space.as_ref());
        let room_list = sidebar::room_list(
            rooms,
            main.space.as_ref(),
            main.room.as_ref().map(|r| &r.id),
            main.rooms.loaded,
        );

        let mut content = column![];
        if main.verified == Some(false) {
            content = content.push(recovery_banner(main));
        }
        if let Some(error) = &main.error {
            content = content.push(container(text(error).style(text::danger)).padding(12));
        }
        let room_view: Element<'a, Message> = match &main.room {
            Some(room) => {
                let name = rooms
                    .iter()
                    .find(|r| r.id == room.id)
                    .map(|r| r.name.as_str())
                    .unwrap_or("");
                room.view(name, &theme, &self.math, &self.math_params())
            }
            None => container(text("Select a room").style(text::secondary))
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
        .secure(true)
        .padding(8)
        .width(320);
    if !busy {
        input = input
            .on_input(Message::RecoveryKey)
            .on_submit(Message::Recover);
    }
    let unlock = button(text(if busy { "Unlocking…" } else { "Unlock" }))
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
