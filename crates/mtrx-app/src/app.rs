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
    Element, Length, Size, Subscription, Task, Theme, mouse,
    theme::Mode,
    widget::{
        button, column, container, image, mouse_area, opaque, row, scrollable::Viewport, stack,
        text, text_editor, text_input,
    },
    window,
};
use mtrx_core::{
    CallService, Connection, Contact, Google, Media, MediaRef, Member, Notification, OwnedMxcUri,
    OwnedRoomId, OwnedUserId, Profile, RecoveryStep, RoomList, RoomListColours, RoomListState,
    RoomOrAliasId, RoomTag, RoomTimeline, ScheduledMessage, SecretStore, ServiceKind, Session,
    TimelineEntry, Url,
};

use crate::{
    actions::{self, Compose, Editing, InlineEdit},
    arrange::{self, Arrange},
    avatar::{self, Avatars},
    calls,
    dialog::{self, Dialog},
    emoji,
    hint::{Position, hint},
    info,
    last_view::LastView,
    login::{self, Login},
    math::{self, Math, Rendered},
    media::{self, Previews, Viewer},
    members, mention, message, pdf_viewer,
    room::{self, Look, Older, Room},
    selection::{self, Selection},
    send_later,
    settings::{self, Settings},
    sidebar,
    style::{self, MESSAGE_INSET, SPACE_BAR_WIDTH},
    tint::Tint,
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
    /// The saved session, or why there is none to open.
    Restored(Result<Shared<Session>, Shared<mtrx_core::Error>>),
    Login(login::Message),
    LoggedIn(Result<Shared<Session>, String>),
    Ready(Result<(Shared<RoomList>, bool), String>),
    Rooms(RoomListState),
    SelectSpace(Option<OwnedRoomId>),
    OpenRoom(OwnedRoomId),
    RoomOpened(OwnedRoomId, Result<Shared<RoomTimeline>, String>),
    Timeline(OwnedRoomId, Vec<TimelineEntry>),
    Scrolled(Viewport),
    Paginated(OwnedRoomId, Result<(), String>),
    Composer(text_editor::Action),
    Send,
    Sent(OwnedRoomId, Result<(), String>),
    /// Our message in a timeline row started sending a while ago.
    SendingSlow(OwnedRoomId, String),
    Later(send_later::Message),
    Act(actions::Message),
    /// Selecting text in the timeline.
    Select(selection::Message),
    /// The room list's colours changed, here or on another device.
    Colours(RoomListColours),
    /// Colours from the settings file were moved to the account, or not.
    ColoursMoved(Result<(), String>),
    /// Scroll the timeline by this much, while dragging a selection.
    ScrollTimeline(f32),
    /// Names of others typing in a room.
    Typing(OwnedRoomId, Vec<String>),
    Scheduled(Vec<ScheduledMessage>),
    /// Refreshes times shown relative to now.
    Tick,
    ShowSettings(bool),
    Settings(settings::Message),
    AddMenu(bool),
    OpenDialog(dialog::Kind),
    Dialog(dialog::Message),
    /// Accept (`true`) or decline an invite.
    AnswerInvite(OwnedRoomId, bool),
    InviteAnswered(OwnedRoomId, bool, Result<(), String>),
    CloseNotice,
    /// Fast ticks while something is in progress.
    Progress,
    RoomFilter(String),
    /// Open the first room matching the search.
    OpenFirstMatch,
    FocusSearch,
    Escape,
    ToggleMembers,
    /// A room's members, and whether we may mention everyone (`@room`).
    MembersLoaded(OwnedRoomId, Result<(Vec<Member>, bool), String>),
    /// Completing a mention in the composer.
    Mention(mention::Message),
    /// Start or open a direct chat with a member.
    ChatWith(OwnedUserId),
    /// Start the Notes to self and open them.
    StartNotes,
    Notified(Notification),
    /// A desktop notification about the room was clicked.
    NotificationClicked(OwnedRoomId),
    WindowFocused(bool),
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
    PreviewLoaded(MediaRef, Option<media::Preview>),
    /// Show an image large.
    OpenImage(Media),
    CloseImage,
    /// The whole image for the viewer.
    ImageLoaded(MediaRef, Result<media::Picture, String>),
    /// A PDF in the viewer was loaded, with its first page drawn.
    OpenPdf(Media),
    Pdf(pdf_viewer::Message),
    /// Ask where to save a message's file, then save it.
    SaveMedia(Media),
    MediaSaved(Result<(), String>),
    /// Ask for files to attach to the message being written.
    Attach,
    /// A file was dropped onto the window: attach it to the message being
    /// written.
    FileDropped(PathBuf),
    /// Files were read to attach to the message in a room.
    Attached(OwnedRoomId, Vec<Result<media::Attached, String>>),
    /// Remove an attached file, by index.
    Unattach(usize),
    Arrange(arrange::Message),
    ArrangeFailed(String),
    /// Show who someone is or what a room is.
    ShowInfo(info::Target),
    Info(info::Message),
    /// Star the direct chat with someone, starting it if there is none.
    FavouritePerson(OwnedUserId),
    /// Reload the open room's timeline (⌘R / Ctrl+R); while offline, try
    /// the server now.
    Reload,
    /// The server became reachable or not.
    Connection(Connection),
    RoomListScrolled,
    OutOfView(sidebar::OutOfView),
    /// Scroll the room list to this offset.
    ScrollRoomList(f32),
    /// Dragging the room list's edge.
    Resize(Resize),
    /// Try the server now.
    Reconnect,
    /// The server no longer accepts the session: forget it and log in again.
    SignInAgain,
    /// The ended session's local data is gone.
    Forgotten(Result<(), String>),
    Reloaded(OwnedRoomId, Result<Shared<RoomTimeline>, String>),
    /// People the user knows, for suggestions.
    ContactsLoaded(Vec<Contact>),
    /// The emoji the user reacts with, most used first.
    FrequentEmoji(Vec<String>),
    /// The composer's emoji panel.
    ComposerEmoji(emoji::Message),
    /// Put an emoji into the composer.
    InsertEmoji(String),
}

pub struct App {
    profile: Profile,
    /// The window has the keyboard focus (notifications only when not
    /// looking at the room).
    focused: bool,
    /// Connection for creating Google Meet links, shared by all accounts.
    google: Arc<Google>,
    settings: Settings,
    /// Where settings are saved; `None` keeps them in memory (tests).
    settings_file: Option<PathBuf>,
    /// Where the last space and room shown are saved; `None` in tests.
    view_file: Option<PathBuf>,
    /// The room list's edge is being dragged.
    resizing: bool,
    /// The settings panel, while open.
    settings_panel: Option<settings::Panel>,
    screen: Screen,
    mode: Mode,
    window: Size,
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
    /// The member list is shown beside the open room, whichever it is.
    show_members: bool,
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
    /// The room search text.
    filter: String,
    own_user: OwnedUserId,
    previews: Previews,
    /// An image shown large.
    viewer: Option<Viewer>,
    /// A PDF shown over the window.
    pdf: Option<pdf_viewer::Viewer>,
    /// Categories, tags and dragging in the room list.
    arrange: Arrange,
    /// The info card shown.
    info: Option<info::Card>,
    /// People the user knows, closest first; refreshed when a dialog
    /// picking people opens.
    contacts: Vec<Contact>,
    /// The emoji the user reacts with, most used first; refreshed when a
    /// reaction picker opens.
    frequent_emoji: Vec<String>,
    connection: Connection,
    /// The room list's colours, from the account.
    colours: RoomListColours,
    /// Unread rooms scrolled out of the room list's view.
    out_of_view: sidebar::OutOfView,
    /// The view to show again once its room is listed (after starting).
    restore: Option<LastView>,
}

/// The room list's width limits, and its draggable edge's.
const MIN_ROOM_LIST_WIDTH: f32 = 200.0;
const MAX_ROOM_LIST_WIDTH: f32 = 520.0;
const RESIZE_HANDLE: f32 = 5.0;

#[derive(Debug, Clone, Copy)]
pub enum Resize {
    Start,
    /// The cursor's x in the window.
    Move(f32),
    End,
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
            show_members: false,
            verified: None,
            recovery_key: String::new(),
            recovery_step: None,
            recovery_error: None,
            scheduled: session.scheduled(),
            own_user: session.user_id(),
            arrange: Arrange::new(&session),
            info: None,
            contacts: Vec::new(),
            frequent_emoji: Vec::new(),
            connection: session.connection(),
            colours: session.room_list_colours(),
            out_of_view: Default::default(),
            restore: None,
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
            filter: String::new(),
            previews: Previews::new(),
            viewer: None,
            pdf: None,
        }
    }
}

impl Main {
    /// Pictures of the spaces, listed rooms, and the open room's header and
    /// timeline senders, with their sizes.
    fn avatars_on_screen(&self, settings: &Settings) -> Vec<(OwnedMxcUri, f32)> {
        let rooms = &self.rooms.rooms;
        let mut wanted = sidebar::avatars(&self.room_list_view(settings));
        wanted.extend(self.info.as_ref().and_then(|c| c.avatar(rooms)));
        if let Some(room) = &self.room {
            wanted.extend(room.avatars(
                rooms.iter().find(|r| r.id == room.id),
                &self.own_user,
                self.show_members,
            ));
        }
        wanted
            .into_iter()
            .map(|(mxc, size)| (mxc.to_owned(), size))
            .collect()
    }

    fn room_list_view<'a>(&'a self, settings: &Settings) -> sidebar::RoomListView<'a> {
        let (grouping, width) = (settings.room_list, settings.room_list_width);
        sidebar::RoomListView {
            rooms: &self.rooms.rooms,
            selected_space: self.space.as_ref(),
            open: self
                .opening
                .as_ref()
                .or(self.invite.as_ref().map(|(id, _)| id))
                .or(self.room.as_ref().map(|r| &r.id)),
            loaded: self.rooms.loaded,
            avatars: &self.avatars,
            scheduled: &self.scheduled,
            add_menu: self.add_menu,
            filter: &self.filter,
            arrange: &self.arrange,
            grouping,
            out_of_view: self.out_of_view,
            width,
            tint: Tint::of(&self.colours, self.space.as_ref()),
            room_pictures: settings.room_pictures,
            notes: self.pinned_notes(settings),
        }
    }

    /// The Notes to self if pinned; missing ones only once the room list
    /// is complete, so they are not started twice.
    fn pinned_notes(&self, settings: &Settings) -> Option<sidebar::Notes<'_>> {
        if !settings.pinned_notes {
            return None;
        }
        let rooms = &self.rooms.rooms;
        match rooms.iter().find(|r| r.is_notes(&self.own_user)) {
            Some(notes) => Some(sidebar::Notes::Room(notes)),
            None => self.rooms.loaded.then_some(sidebar::Notes::Missing),
        }
    }
}

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
            focused: true,
            google: Arc::new(Google::new(&data_root, secrets)),
            settings: Settings::load(&settings_file),
            settings_file: Some(settings_file),
            view_file: Some(LastView::path(&profile)),
            resizing: false,
            settings_panel: None,
            screen: Screen::Starting,
            mode: Mode::Light,
            window: Size::new(1200.0, 800.0),
            scale: 1.0,
            math: Math::new(),
        };
        let restore = Task::perform(
            async move { Session::restore(profile).await.map(|s| Shared(Arc::new(s))) },
            |result| Message::Restored(result.map_err(|e| Shared(Arc::new(e)))),
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
        // Unread rooms may have moved into or out of the room list's view.
        let moved = matches!(
            message,
            Message::Rooms(_)
                | Message::Ready(_)
                | Message::RoomListScrolled
                | Message::SelectSpace(_)
                | Message::RoomFilter(_)
                | Message::Arrange(_)
                | Message::AddMenu(_)
                | Message::OpenRoom(_)
                | Message::WindowResized(_)
        );
        let remember = matches!(message, Message::OpenRoom(_) | Message::SelectSpace(_));
        let task = self.handle(message);
        if remember {
            self.remember_view();
        }
        match &self.screen {
            Screen::Main(main) if moved => Task::batch([
                task,
                sidebar::find_out_of_view(&main.rooms.rooms).map(Message::OutOfView),
            ]),
            _ => task,
        }
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::RoomListScrolled => Task::none(),
            Message::OutOfView(out_of_view) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.out_of_view = out_of_view;
                }
                Task::none()
            }
            Message::ScrollRoomList(offset) => sidebar::scroll_to(offset),
            Message::Resize(Resize::Start) => {
                self.resizing = true;
                Task::none()
            }
            Message::Resize(Resize::Move(x)) => {
                self.settings.room_list_width =
                    (x - SPACE_BAR_WIDTH).clamp(MIN_ROOM_LIST_WIDTH, MAX_ROOM_LIST_WIDTH);
                Task::none()
            }
            Message::Resize(Resize::End) => {
                self.resizing = false;
                if let Some(file) = &self.settings_file
                    && let Err(e) = self.settings.save(file)
                {
                    tracing::warn!("could not save settings: {e}");
                }
                // Math is laid out for the room's width.
                self.render_math()
            }
            Message::Restored(Ok(Shared(session))) | Message::LoggedIn(Ok(Shared(session))) => {
                let mut main = Main::new(session.clone());
                main.restore = self.view_file.as_deref().map(LastView::load);
                self.screen = Screen::Main(Box::new(main));
                let migrate = self.move_colours_to_account(&session);
                let start = Task::perform(
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
                );
                Task::batch([start, migrate])
            }
            Message::Colours(colours) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.colours = colours;
                }
                Task::none()
            }
            Message::ColoursMoved(result) => {
                match result {
                    Ok(()) => {
                        self.settings.legacy_home_tint = None;
                        self.settings.legacy_space_tints.clear();
                        if let Some(file) = &self.settings_file
                            && let Err(e) = self.settings.save(file)
                        {
                            tracing::warn!("could not save settings: {e}");
                        }
                    }
                    Err(e) => tracing::warn!("could not move room list colours: {e}"),
                }
                Task::none()
            }
            Message::Restored(Err(Shared(error))) => {
                let mut login = Login::default();
                // Usually nobody is signed in yet. Without a keyring signing
                // in would fail too, so say so before the user tries.
                if let mtrx_core::Error::NoSecretStore(_) = *error {
                    login.error = Some(error.to_string());
                }
                self.screen = Screen::Login(login);
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
                        main.arrange.rooms_updated(&mut main.rooms.rooms);
                        main.room_list = Some(list);
                        main.verified = Some(verified);
                    }
                    Err(e) => main.error = Some(e),
                }
                Task::batch([self.load_avatars(), self.restore_view()])
            }
            Message::Rooms(state) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.rooms = state;
                    main.arrange.rooms_updated(&mut main.rooms.rooms);
                }
                Task::batch([self.load_avatars(), self.restore_view()])
            }
            Message::SelectSpace(space) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                main.space = space.clone();
                main.info = None;
                main.restore = None;
                main.arrange.escape();
                let members = match space {
                    Some(space) => main.arrange.space_selected(&main.session, space),
                    None => Task::none(),
                };
                Task::batch([members, self.load_avatars()])
            }
            Message::Arrange(message) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                if matches!(message, arrange::Message::NewCategory(_)) {
                    main.add_menu = false;
                }
                let context = arrange::Context {
                    session: &main.session,
                    space: main.space.as_ref(),
                    rooms: &mut main.rooms.rooms,
                    grouping: self.settings.room_list,
                };
                let task = main.arrange.update(message, context);
                Task::batch([task, self.load_avatars()])
            }
            Message::ArrangeFailed(e) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.notice = Some(Notice {
                        text: format!("Could not rearrange the room list: {e}"),
                        room: None,
                        error: true,
                    });
                }
                Task::none()
            }
            Message::OpenRoom(id) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                main.add_menu = false;
                main.info = None;
                // Chosen now: last time's view is no longer wanted.
                main.restore = None;
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
                        let paginate = fill(&mut room);
                        let read = mark_read(&room);
                        // The member list stays open from the last room.
                        let members = if main.show_members {
                            load_members(&main.session, &mut room)
                        } else {
                            Task::none()
                        };
                        main.room = Some(room);
                        Task::batch([
                            paginate,
                            read,
                            members,
                            self.render_math(),
                            self.load_avatars(),
                            self.load_previews(),
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
                let clocks = room.sending_changed().into_iter().map(|row| {
                    let id = id.clone();
                    Task::perform(tokio::time::sleep(room::SENDING_CLOCK), move |()| {
                        Message::SendingSlow(id.clone(), row.clone())
                    })
                });
                Task::batch([
                    Task::batch(clocks),
                    read,
                    fill(room),
                    self.render_math(),
                    self.load_avatars(),
                    self.load_previews(),
                ])
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
                        Task::batch([
                            self.render_math(),
                            self.load_avatars(),
                            self.load_previews(),
                        ])
                    }
                    Older::Paginate => paginate(room),
                }
            }
            Message::Paginated(id, result) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let mut more = Task::none();
                if let Some(room) = main.room.as_mut().filter(|r| r.id == id) {
                    room.paginating = false;
                    match result {
                        Ok(()) => {
                            room.show_more();
                            more = fill(room);
                        }
                        // Loaded once the server is back.
                        Err(_) if main.connection != Connection::Online => {}
                        Err(e) => room.error = Some(e),
                    }
                }
                Task::batch([
                    more,
                    self.render_math(),
                    self.load_avatars(),
                    self.load_previews(),
                ])
            }
            Message::Composer(action) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let Some(room) = main.room.as_mut() else {
                    return Task::none();
                };
                let edit = action.is_edit();
                room.composer.perform(action);
                room.update_completion();
                // Mentions need the members: loaded on the first `@`.
                let members = if room.completion.is_some()
                    && room.member_list.is_none()
                    && !room.loading_members
                {
                    load_members(&main.session, room)
                } else {
                    Task::none()
                };
                let typing = if edit {
                    let timeline = room.timeline.clone();
                    let typing = !room.composer.text().trim().is_empty();
                    Task::future(async move {
                        let _ = timeline.typing(typing).await;
                    })
                    .discard()
                } else {
                    Task::none()
                };
                // The pictures of those offered.
                let avatars = if room.completion.is_some() {
                    self.load_avatars()
                } else {
                    Task::none()
                };
                Task::batch([members, typing, avatars])
            }
            Message::Mention(message) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let Some(room) = main.room.as_mut() else {
                    return Task::none();
                };
                let count = room.candidates(&main.own_user).len();
                let mention = match message {
                    mention::Message::Close => {
                        room.completion = None;
                        return Task::none();
                    }
                    mention::Message::Up | mention::Message::Down => {
                        let step = if matches!(message, mention::Message::Up) {
                            -1
                        } else {
                            1
                        };
                        if let Some(completion) = &mut room.completion {
                            completion.step(step, count);
                        }
                        return Task::none();
                    }
                    mention::Message::Choose => {
                        let selected = room.completion.as_ref().map_or(0, |c| c.selected);
                        match room.candidates(&main.own_user).get(selected) {
                            Some(candidate) => candidate.mention(),
                            None => return Task::none(),
                        }
                    }
                    mention::Message::Pick(mention) => mention,
                };
                room.complete(&mention);
                iced::widget::operation::focus(room::COMPOSER)
            }
            Message::Send => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let Some(room) = main.room.as_mut() else {
                    return Task::none();
                };
                let text = room.composer.text();
                if text.trim().is_empty() && room.attachments.is_empty() {
                    return Task::none();
                }
                let compose = std::mem::take(&mut room.compose);
                room.set_composer(text_editor::Content::new());
                let files = std::mem::take(&mut room.attachments)
                    .into_iter()
                    .map(|attached| attached.file)
                    .collect();
                room.error = None;
                let (id, timeline) = (room.id.clone(), room.timeline.clone());
                // Queue only: the local echo shows the send state. Queued
                // first, so it shows before the server hears of anything.
                Task::perform(
                    async move {
                        let sent = timeline
                            .send(text.trim_end(), files, compose.reply_to().map(|id| &**id))
                            .await
                            .map(|_| ());
                        let _ = timeline.typing(false).await;
                        sent
                    },
                    move |result| Message::Sent(id.clone(), result.map_err(|e| e.to_string())),
                )
            }
            Message::SendingSlow(id, row) => {
                if let Screen::Main(main) = &mut self.screen
                    && let Some(room) = main.room.as_mut().filter(|r| r.id == id)
                {
                    room.sending_slow(&row);
                }
                Task::none()
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
            Message::Act(message) => self.act(message),
            Message::ScrollTimeline(dy) => iced::widget::operation::scroll_by(
                room::TIMELINE,
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: dy },
            ),
            Message::Select(message) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let Some(room) = main.room.as_mut() else {
                    return Task::none();
                };
                match message {
                    selection::Message::Copy => {
                        return room
                            .selected_text()
                            .map_or_else(Task::none, iced::clipboard::write);
                    }
                    selection::Message::SelectAll => {
                        return room.on_screen().and_then(|(first, last)| {
                            Task::done(Message::Select(selection::Message::All(first, last)))
                        });
                    }
                    _ => {}
                }
                Selection::update(&mut room.selection, message);
                Task::none()
            }
            Message::Typing(room_id, names) => {
                if let Screen::Main(main) = &mut self.screen
                    && let Some(room) = main.room.as_mut().filter(|r| r.id == room_id)
                {
                    room.typing = names;
                }
                Task::none()
            }
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
            Message::OpenDialog(kind) => {
                self.settings_panel = None;
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                main.add_menu = false;
                main.info = None;
                let near = near_spaces(&kind, &main.rooms.rooms, main.space.as_ref());
                let mut dialog = Dialog::new(kind);
                if !dialog.picks_people() {
                    main.dialog = Some(dialog);
                    return Task::none();
                }
                dialog.prefer_spaces(near);
                dialog.offer_self(main.own_user.clone());
                dialog.update(dialog::Message::Contacts(main.contacts.clone()));
                main.dialog = Some(dialog);
                let session = main.session.clone();
                Task::perform(
                    async move { session.contacts().await.unwrap_or_default() },
                    Message::ContactsLoaded,
                )
            }
            Message::ComposerEmoji(message) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let Some(room) = main.room.as_mut() else {
                    return Task::none();
                };
                match message {
                    emoji::Message::Close => {
                        room.composer_emoji = None;
                        Task::none()
                    }
                    emoji::Message::Open => {
                        room.composer_emoji = Some(emoji::Panel::default());
                        Task::batch([
                            iced::widget::operation::focus(emoji::SEARCH),
                            load_frequent_emoji(&main.session),
                        ])
                    }
                    message => {
                        if let Some(panel) = &mut room.composer_emoji {
                            panel.update(message);
                        }
                        Task::none()
                    }
                }
            }
            Message::InsertEmoji(emoji) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let Some(room) = main.room.as_mut() else {
                    return Task::none();
                };
                emoji::insert(&mut room.composer, &emoji);
                room.update_completion();
                Task::batch([
                    iced::widget::operation::focus(room::COMPOSER),
                    used_emoji(main, emoji),
                ])
            }
            Message::FrequentEmoji(emoji) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.frequent_emoji = emoji;
                }
                Task::none()
            }
            Message::ContactsLoaded(contacts) => {
                if let Screen::Main(main) = &mut self.screen {
                    if let Some(dialog) = &mut main.dialog {
                        dialog.update(dialog::Message::Contacts(contacts.clone()));
                    }
                    main.contacts = contacts;
                }
                Task::none()
            }
            Message::ShowInfo(target) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                main.arrange.escape();
                main.add_menu = false;
                // Opened from the list of a message's readers, say.
                if let Some(room) = &mut main.room {
                    room.readers = None;
                    room.reactors = None;
                }
                let card = info::Card::new(target, &main.rooms.rooms);
                let load = match card.user() {
                    Some(user) => {
                        let (session, user) = (main.session.clone(), user.clone());
                        Task::perform(
                            {
                                let user = user.clone();
                                async move { session.user_info(&user).await.map_err(|e| e.to_string()) }
                            },
                            move |result| {
                                Message::Info(info::Message::UserLoaded(user.clone(), result))
                            },
                        )
                    }
                    None => card
                        .room()
                        .map_or_else(Task::none, |room| load_settings(&main.session, room)),
                };
                main.info = Some(card);
                Task::batch([load, self.load_avatars()])
            }
            Message::Info(message) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let Some(card) = &mut main.info else {
                    return Task::none();
                };
                let session = main.session.clone();
                let room = card.room().cloned();
                let saved = |result: mtrx_core::Result<()>| {
                    Message::Info(info::Message::Saved(result.map_err(|e| e.to_string())))
                };
                let task = match (&message, room) {
                    (info::Message::Close, _) => {
                        main.info = None;
                        return Task::none();
                    }
                    (info::Message::Copy(text), _) => iced::clipboard::write(text.clone()),
                    (info::Message::SaveDetails, Some(room)) => {
                        let Some(summary) = main.rooms.rooms.iter().find(|r| r.id == room) else {
                            return Task::none();
                        };
                        let (name, topic) = card.changed_details(summary);
                        Task::perform(
                            async move {
                                if let Some(name) = name {
                                    session.set_room_name(&room, &name).await?;
                                }
                                if let Some(topic) = topic {
                                    session.set_room_topic(&room, &topic).await?;
                                }
                                Ok(())
                            },
                            saved,
                        )
                    }
                    (info::Message::Access(access), Some(room)) => {
                        let access = *access;
                        Task::perform(
                            async move { session.set_room_access(&room, access).await },
                            saved,
                        )
                    }
                    (info::Message::Notify(mode), Some(room)) => {
                        let mode = *mode;
                        Task::perform(
                            async move { session.set_notify_mode(&room, mode).await },
                            saved,
                        )
                    }
                    (info::Message::ChangePicture, Some(room)) => Task::perform(
                        async move {
                            let Some(file) = rfd::AsyncFileDialog::new()
                                .set_title("Room picture")
                                .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
                                .pick_file()
                                .await
                            else {
                                return Ok(());
                            };
                            session.set_room_picture(&room, file.path()).await
                        },
                        saved,
                    ),
                    (info::Message::RemovePicture, Some(room)) => Task::perform(
                        async move { session.remove_room_picture(&room).await },
                        saved,
                    ),
                    // What was saved, or the settings as they are after a failure.
                    (info::Message::Saved(_), Some(room)) => load_settings(&session, &room),
                    _ => Task::none(),
                };
                card.update(message);
                Task::batch([task, self.load_avatars()])
            }
            Message::FavouritePerson(user) => {
                let Screen::Main(main) = &self.screen else {
                    return Task::none();
                };
                let session = main.session.clone();
                Task::future(async move {
                    let room = session.direct_chat(&user).await?;
                    session.set_room_tag(&room, RoomTag::Favourite).await
                })
                .then(|result| match result {
                    Ok(()) => Task::none(),
                    Err(e) => Task::done(Message::ArrangeFailed(e.to_string())),
                })
            }
            Message::Reload => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                match main.connection {
                    Connection::Offline => return self.update(Message::Reconnect),
                    // Its bar offers signing in again.
                    Connection::SignedOut => return Task::none(),
                    Connection::Online => {}
                }
                let Some(room) = &mut main.room else {
                    return Task::none();
                };
                room.reloading = true;
                let (session, id) = (main.session.clone(), room.id.clone());
                Task::perform(
                    async move {
                        let timeline = session.timeline(&id).await.map(|t| Shared(Arc::new(t)));
                        (id, timeline.map_err(|e| e.to_string()))
                    },
                    |(id, result)| Message::Reloaded(id, result),
                )
            }
            Message::Connection(connection) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let back =
                    main.connection == Connection::Offline && connection == Connection::Online;
                main.connection = connection;
                // History that could not load while offline.
                match main.room.as_mut() {
                    Some(room) if back => fill(room),
                    _ => Task::none(),
                }
            }
            Message::SignInAgain => {
                let Screen::Main(main) = &self.screen else {
                    return Task::none();
                };
                let session = main.session.clone();
                Task::perform(
                    async move { session.forget().await.map_err(|e| e.to_string()) },
                    Message::Forgotten,
                )
            }
            Message::Forgotten(result) => {
                let Screen::Main(main) = &self.screen else {
                    return Task::none();
                };
                // Ready to log in as before.
                let mut login = Login::default();
                login.homeserver = main.own_user.server_name().to_string();
                login.user = main.own_user.localpart().to_owned();
                login.error = result.err();
                self.screen = Screen::Login(login);
                Task::none()
            }
            Message::Reconnect => {
                let Screen::Main(main) = &self.screen else {
                    return Task::none();
                };
                let session = main.session.clone();
                Task::future(async move { session.reconnect().await }).discard()
            }
            Message::Reloaded(id, result) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let Some(room) = main.room.as_mut().filter(|r| r.id == id) else {
                    return Task::none();
                };
                match result {
                    Ok(Shared(timeline)) => {
                        room.reload(timeline);
                        Task::batch([
                            fill(room),
                            mark_read(room),
                            self.render_math(),
                            self.load_avatars(),
                            self.load_previews(),
                            self.load_room_calls(&id),
                        ])
                    }
                    Err(e) => {
                        room.reloading = false;
                        room.error = Some(e);
                        Task::none()
                    }
                }
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
            Message::RoomFilter(filter) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.filter = filter;
                }
                self.load_avatars()
            }
            Message::OpenFirstMatch => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                match sidebar::first_match(&main.rooms.rooms, &main.filter) {
                    Some(room) => {
                        let id = room.id.clone();
                        main.filter.clear();
                        self.update(Message::OpenRoom(id))
                    }
                    None => Task::none(),
                }
            }
            Message::FocusSearch => iced::widget::operation::focus(sidebar::SEARCH),
            // Closes the topmost thing: the image viewer, a drag, menu or
            // category name, else the search.
            Message::Escape => {
                if let Screen::Main(main) = &mut self.screen {
                    if main.viewer.is_some() || main.pdf.is_some() {
                        main.viewer = None;
                        main.pdf = None;
                    } else if main.info.is_some() {
                        main.info = None;
                    } else if let Some(room) = main.room.as_mut().filter(|r| r.calls.is_open()) {
                        room.calls.close();
                    } else if let Some(room) = main
                        .room
                        .as_mut()
                        .filter(|r| !r.candidates(&main.own_user).is_empty())
                    {
                        room.completion = None;
                    } else if let Some(room) = main.room.as_mut().filter(|r| r.menu.is_some()) {
                        room.menu = None;
                    } else if let Some(room) = main.room.as_mut().filter(|r| r.readers.is_some()) {
                        room.readers = None;
                    } else if let Some(room) = main.room.as_mut().filter(|r| r.reactors.is_some()) {
                        room.reactors = None;
                    } else if let Some(room) = main.room.as_mut().filter(|r| r.unsent.is_some()) {
                        room.unsent = None;
                    } else if let Some(room) = main.room.as_mut().filter(|r| r.selection.is_some())
                    {
                        room.selection = None;
                    } else if let Some(room) =
                        main.room.as_mut().filter(|r| r.composer_emoji.is_some())
                    {
                        room.composer_emoji = None;
                    } else if let Some(edit) = main
                        .room
                        .as_mut()
                        .and_then(|r| r.editing.as_mut())
                        .filter(|e| e.emoji.is_some())
                    {
                        edit.emoji = None;
                    } else if let Some(room) = main.room.as_mut().filter(|r| r.editing.is_some()) {
                        room.editing = None;
                    } else if let Some(room) =
                        main.room.as_mut().filter(|r| r.later.picker.is_some())
                    {
                        room.later = Default::default();
                    } else if let Some(room) = main.room.as_mut().filter(|r| r.picker.is_some()) {
                        room.picker = None;
                        room.emoji = None;
                    } else if !main.arrange.escape() {
                        main.filter.clear();
                    }
                }
                Task::none()
            }
            Message::ToggleMembers => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                main.show_members = !main.show_members;
                // Opening it shows the members anew.
                match main.room.as_mut() {
                    Some(room) if main.show_members && !room.loading_members => {
                        load_members(&main.session, room)
                    }
                    _ => Task::none(),
                }
            }
            Message::MembersLoaded(id, result) => {
                if let Screen::Main(main) = &mut self.screen
                    && let Some(room) = main.room.as_mut().filter(|r| r.id == id)
                {
                    room.members_loaded(result);
                }
                self.load_avatars()
            }
            Message::StartNotes => {
                let Screen::Main(main) = &self.screen else {
                    return Task::none();
                };
                let own_user = main.own_user.clone();
                self.update(Message::ChatWith(own_user))
            }
            Message::ChatWith(user) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                main.info = None;
                let session = main.session.clone();
                Task::perform(
                    async move { session.direct_chat(&user).await.map_err(|e| e.to_string()) },
                    |result| match result {
                        Ok(room) => Message::OpenRoom(room),
                        Err(e) => Message::Dialog(dialog::Message::Done(Err(e))),
                    },
                )
            }
            Message::Notified(notification) => {
                let Screen::Main(main) = &self.screen else {
                    return Task::none();
                };
                let looking = self.focused
                    && main
                        .room
                        .as_ref()
                        .is_some_and(|r| r.id == notification.room_id);
                if !self.settings.desktop_notifications || looking {
                    return Task::none();
                }
                let room = notification.room_id.clone();
                Task::future(notify(notification)).then(move |clicked| {
                    if clicked {
                        Task::done(Message::NotificationClicked(room.clone()))
                    } else {
                        Task::none()
                    }
                })
            }
            Message::NotificationClicked(room) => {
                let raise = window::oldest().then(|id| match id {
                    Some(id) => window::gain_focus(id),
                    None => Task::none(),
                });
                Task::batch([raise, self.update(Message::OpenRoom(room))])
            }
            Message::WindowFocused(focused) => {
                self.focused = focused;
                Task::none()
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
                    if let Some(room) = main.room.as_mut() {
                        room.scheduled_changed(&scheduled);
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
            // A mention opens the direct chat with them; other links the
            // browser.
            Message::LinkClicked(url) => match message::mentioned_user(&url) {
                Some(user) => self.update(Message::ChatWith(user)),
                None => {
                    let _ = open::that_detached(url);
                    Task::none()
                }
            },
            Message::WindowResized(size) => {
                self.window = size;
                Task::batch([self.render_math(), self.pdf_resized()])
            }
            Message::ScaleFactor(scale) => {
                self.scale = scale;
                Task::batch([self.render_math(), self.pdf_resized()])
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
            Message::PreviewLoaded(media, picture) => {
                if let Screen::Main(main) = &mut self.screen {
                    main.previews.insert(media, picture);
                }
                Task::none()
            }
            Message::OpenImage(media) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let viewer = Viewer::new(media);
                let load = viewer.load(main.session.clone());
                main.viewer = Some(viewer);
                load
            }
            Message::CloseImage => {
                if let Screen::Main(main) = &mut self.screen {
                    main.viewer = None;
                    main.pdf = None;
                }
                Task::none()
            }
            Message::OpenPdf(media) => {
                let Screen::Main(main) = &mut self.screen else {
                    return Task::none();
                };
                let viewer = pdf_viewer::Viewer::new(media, self.window, self.scale);
                let open = viewer.open(main.session.clone());
                main.pdf = Some(viewer);
                open
            }
            Message::Pdf(message) => match &mut self.screen {
                Screen::Main(main) => main
                    .pdf
                    .as_mut()
                    .map_or_else(Task::none, |viewer| viewer.update(message)),
                _ => Task::none(),
            },
            Message::ImageLoaded(media, result) => {
                if let Screen::Main(main) = &mut self.screen
                    && let Some(viewer) = &mut main.viewer
                {
                    viewer.loaded(&media, result);
                }
                Task::none()
            }
            Message::SaveMedia(file) => {
                let Screen::Main(main) = &self.screen else {
                    return Task::none();
                };
                Task::perform(media::save(main.session.clone(), file), Message::MediaSaved)
            }
            Message::MediaSaved(result) => {
                if let Screen::Main(main) = &mut self.screen
                    && let Err(e) = result
                {
                    // Shown where it was saved from.
                    match (&mut main.viewer, &mut main.pdf, &mut main.room) {
                        (Some(viewer), _, _) => viewer.error = Some(e),
                        (None, Some(pdf), _) => pdf.error = Some(e),
                        (None, None, Some(room)) => room.error = Some(e),
                        (None, None, None) => {}
                    }
                }
                Task::none()
            }
            Message::Attach => {
                let Screen::Main(main) = &self.screen else {
                    return Task::none();
                };
                let Some(room) = &main.room else {
                    return Task::none();
                };
                let id = room.id.clone();
                Task::future(media::pick_files()).then(move |paths| attach(id.clone(), paths))
            }
            Message::FileDropped(path) => {
                let Screen::Main(main) = &self.screen else {
                    return Task::none();
                };
                let Some(room) = &main.room else {
                    return Task::none();
                };
                attach(room.id.clone(), vec![path])
            }
            Message::Attached(id, attached) => {
                if let Screen::Main(main) = &mut self.screen
                    && let Some(room) = main.room.as_mut().filter(|r| r.id == id)
                {
                    for attached in attached {
                        match attached {
                            Ok(attached) => room.attachments.push(attached),
                            Err(e) => room.error = Some(e),
                        }
                    }
                }
                Task::none()
            }
            Message::Unattach(index) => {
                if let Screen::Main(main) = &mut self.screen
                    && let Some(room) = &mut main.room
                    && index < room.attachments.len()
                {
                    room.attachments.remove(index);
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

    /// Room list colours chosen before the account kept them, moved there
    /// unless the account has colours already.
    fn move_colours_to_account(&self, session: &Arc<Session>) -> Task<Message> {
        let Some(legacy) = self.settings.legacy_colours() else {
            return Task::none();
        };
        let session = session.clone();
        Task::perform(
            async move {
                if session.room_list_colours() != RoomListColours::default() {
                    return Ok(());
                }
                session
                    .change_room_list_colours(|colours| *colours = legacy)
                    .await
                    .map_err(|e| e.to_string())
            },
            Message::ColoursMoved,
        )
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
            settings::Action::Colour(space, tint) => {
                let Screen::Main(main) = &self.screen else {
                    return Task::none();
                };
                let session = main.session.clone();
                Task::perform(
                    async move {
                        session
                            .change_room_list_colours(|colours| {
                                let name = tint.name().to_owned();
                                match space {
                                    Some(space) => {
                                        colours.spaces.insert(space, name);
                                    }
                                    None => colours.home = Some(name),
                                }
                            })
                            .await
                    },
                    |result| {
                        Message::Settings(settings::Message::ColourSaved(
                            result.map_err(|e| e.to_string()),
                        ))
                    },
                )
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
            Calls::OpenMenu => {
                state.menu = true;
                state.editor = None;
                state.error = None;
            }
            Calls::Close => state.close(),
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
            Calls::Started(result) => {
                state.busy = false;
                // Shown below the composer, like other sending errors.
                if let Err(e) = result {
                    room.error = Some(e);
                }
            }
            Calls::LinkSaved(result) => {
                state.busy = false;
                match result {
                    Ok(()) => {
                        state.menu = false;
                        state.editor = None;
                    }
                    Err(e) => state.error = Some(e),
                }
            }
            Calls::EditLink => {
                state.menu = false;
                state.error = None;
                state.editor = Some(state.link.as_ref().map(Url::to_string).unwrap_or_default());
                state.announce = false;
            }
            Calls::LinkInput(input) => state.editor = Some(input),
            Calls::Announce(on) => state.announce = on,
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
                let announce = link.clone().filter(|_| state.announce);
                state.busy = true;
                state.error = None;
                let room = room.id.clone();
                return Task::perform(
                    async move {
                        session
                            .set_call_link(&room, link.as_ref())
                            .await
                            .map_err(|e| e.to_string())?;
                        if let Some(link) = announce {
                            session
                                .announce_call_link(&room, &link)
                                .await
                                .map_err(|e| e.to_string())?;
                        }
                        Ok(())
                    },
                    move |result| reply(Calls::LinkSaved(result)),
                );
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

    fn act(&mut self, message: actions::Message) -> Task<Message> {
        use actions::Message as Act;
        let Screen::Main(main) = &mut self.screen else {
            return Task::none();
        };
        let Some(room) = main.room.as_mut() else {
            return Task::none();
        };
        let timeline = room.timeline.clone();
        let done = |result: mtrx_core::Result<()>| {
            Message::Act(Act::Done(result.map_err(|e| e.to_string())))
        };
        match message {
            Act::React(key, emoji) => {
                room.picker = None;
                room.emoji = None;
                let react = Task::perform(
                    {
                        let emoji = emoji.clone();
                        async move { timeline.toggle_reaction(&key, &emoji).await }
                    },
                    done,
                );
                Task::batch([react, used_emoji(main, emoji)])
            }
            Act::EditEmoji(message) => {
                let Some(edit) = &mut room.editing else {
                    return Task::none();
                };
                match message {
                    emoji::Message::Close => {
                        edit.emoji = None;
                        Task::none()
                    }
                    emoji::Message::Open => {
                        edit.emoji = Some(emoji::Panel::default());
                        Task::batch([
                            iced::widget::operation::focus(emoji::SEARCH),
                            load_frequent_emoji(&main.session),
                        ])
                    }
                    message => {
                        if let Some(panel) = &mut edit.emoji {
                            panel.update(message);
                        }
                        Task::none()
                    }
                }
            }
            Act::EditInsertEmoji(emoji) => {
                if let Some(edit) = &mut room.editing {
                    edit.insert_emoji(&emoji);
                }
                Task::batch([InlineEdit::focus(), used_emoji(main, emoji)])
            }
            Act::OpenMenu(id, at) => {
                room.menu = Some((id, at));
                room.picker = None;
                room.emoji = None;
                Task::none()
            }
            Act::CloseMenu => {
                room.menu = None;
                Task::none()
            }
            Act::OpenReaders(id) => {
                room.readers = Some(id);
                Task::none()
            }
            Act::CloseReaders => {
                room.readers = None;
                Task::none()
            }
            Act::OpenReactors(id, key) => {
                room.reactors = Some((id, key));
                Task::none()
            }
            Act::CloseReactors => {
                room.reactors = None;
                Task::none()
            }
            Act::OpenUnsent(id) => {
                room.unsent = Some(id);
                Task::none()
            }
            Act::CloseUnsent => {
                room.unsent = None;
                Task::none()
            }
            Act::Retry(key) => {
                room.unsent = None;
                Task::perform(async move { timeline.retry(&key).await }, done)
            }
            Act::Discard(key) => {
                room.unsent = None;
                Task::perform(async move { timeline.discard(&key).await }, done)
            }
            Act::Copy(text) => {
                room.menu = None;
                iced::clipboard::write(text)
            }
            Act::CopySelection => {
                room.menu = None;
                room.selected_text()
                    .map_or_else(Task::none, iced::clipboard::write)
            }
            Act::CopyImage(document) => {
                room.menu = None;
                Task::perform(self.math.copy_image(&document), |result| {
                    Message::Act(Act::Done(result))
                })
            }
            Act::CopyMediaImage(file) => {
                room.menu = None;
                Task::perform(media::copy_image(main.session.clone(), file), |result| {
                    Message::Act(Act::Done(result))
                })
            }
            Act::SaveMedia(file) => {
                room.menu = None;
                Task::done(Message::SaveMedia(file))
            }
            Act::SaveImage(document) => {
                room.menu = None;
                let png = self.math.png(&document);
                Task::perform(
                    async move {
                        let png = png.await?;
                        let Some(file) = rfd::AsyncFileDialog::new()
                            .set_file_name("message.png")
                            .save_file()
                            .await
                        else {
                            return Ok(());
                        };
                        file.write(&png)
                            .await
                            .map_err(|e| format!("Could not save {}: {e}", file.file_name()))
                    },
                    |result| Message::Act(Act::Done(result)),
                )
            }
            Act::TogglePicker(id, at) => {
                let picker = (id, at);
                room.picker = (room.picker.as_ref() != Some(&picker)).then_some(picker);
                room.emoji = None;
                if room.picker.is_none() {
                    return Task::none();
                }
                load_frequent_emoji(&main.session)
            }
            Act::ClosePicker => {
                room.picker = None;
                room.emoji = None;
                Task::none()
            }
            Act::Emoji(emoji::Message::Open) => {
                room.emoji = Some(emoji::Panel::default());
                iced::widget::operation::focus(emoji::SEARCH)
            }
            Act::Emoji(emoji::Message::Close) => {
                room.emoji = None;
                Task::none()
            }
            Act::Emoji(message) => {
                if let Some(panel) = &mut room.emoji {
                    panel.update(message);
                }
                Task::none()
            }
            Act::Reply(target) => {
                room.compose = Compose::Reply(target);
                room.later.picker = None;
                Task::none()
            }
            Act::Edit(key, text) => {
                room.editing = Some(InlineEdit::new(Editing::Sent(key), &text));
                InlineEdit::focus()
            }
            Act::EditAction(action) => {
                if let Some(edit) = &mut room.editing {
                    edit.perform(action);
                }
                Task::none()
            }
            Act::SaveEdit => {
                let Some(edit) = &room.editing else {
                    return Task::none();
                };
                let text = edit.text();
                if text.trim().is_empty() {
                    return Task::none();
                }
                let session = main.session.clone();
                let target = edit.target.clone();
                Task::perform(
                    async move {
                        match target {
                            Editing::Sent(key) => timeline.edit_markdown(&key, &text).await,
                            Editing::Scheduled(id) => session.edit_scheduled(&id, &text).await,
                        }
                    },
                    |result| Message::Act(Act::EditSaved(result.map_err(|e| e.to_string()))),
                )
            }
            Act::EditSaved(Ok(())) | Act::CancelEdit => {
                room.editing = None;
                Task::none()
            }
            Act::EditSaved(Err(e)) => {
                room.error = Some(e);
                Task::none()
            }
            Act::AskDelete(id) => {
                room.confirm_delete = Some(id);
                Task::none()
            }
            Act::CancelDelete => {
                room.confirm_delete = None;
                Task::none()
            }
            Act::Delete(key) => {
                room.confirm_delete = None;
                Task::perform(async move { timeline.redact(&key).await }, done)
            }
            Act::CancelCompose => {
                // The reply's text stays as a draft.
                room.compose = Compose::New;
                room.error = None;
                Task::none()
            }
            Act::Done(result) => {
                if let Err(e) = result {
                    room.error = Some(e);
                }
                Task::none()
            }
        }
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
            Later::Close => {
                room.later = Default::default();
                Task::none()
            }
            Later::Open => {
                room.composer_emoji = None;
                let picker = send_later::Picker::new(SystemTime::now(), None);
                let scroll = picker.scroll();
                room.later.picker = Some(picker);
                scroll
            }
            Later::ChangeTime(message) => {
                // From its time, unless that has passed.
                let picker = send_later::Picker::new(SystemTime::now(), Some(message.send_at));
                let scroll = picker.scroll();
                room.later.picker = Some(picker);
                room.later.rescheduling = Some(message);
                scroll
            }
            Later::Month(_) | Later::Day(_) | Later::Hour(_) | Later::Minute(_) => {
                if let Some(picker) = &mut room.later.picker {
                    picker.update(&message, SystemTime::now());
                }
                Task::none()
            }
            Later::Schedule => match room
                .later
                .picker
                .as_ref()
                .map(|p| p.time(SystemTime::now()))
            {
                Some(Ok(at)) => self.later(Later::At(at)),
                _ => Task::none(),
            },
            Later::At(send_at) => {
                room.error = None;
                let saved = |result: mtrx_core::Result<()>| {
                    Message::Later(Later::Saved(result.map_err(|e| e.to_string())))
                };
                if let Some(message) = &room.later.rescheduling {
                    let id = message.id.clone();
                    return Task::perform(
                        async move { session.reschedule(&id, send_at).await },
                        saved,
                    );
                }
                let text = room.composer.text().trim_end().to_owned();
                if text.trim().is_empty() && room.attachments.is_empty() {
                    return Task::none();
                }
                let room_id = room.id.clone();
                let reply_to = room.compose.reply_to().cloned();
                let files = room.attachments.iter().map(|a| a.file.clone()).collect();
                // The composer is cleared once saved, so nothing is lost on errors.
                Task::perform(
                    async move {
                        session
                            .schedule(&room_id, &text, files, send_at, reply_to.as_deref())
                            .await
                            .map(|_| ())
                    },
                    saved,
                )
            }
            Later::Edit(message) => {
                room.editing = Some(InlineEdit::new(
                    Editing::Scheduled(message.id.clone()),
                    &message.text,
                ));
                InlineEdit::focus()
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
                // A new time leaves the composer alone; scheduling took its text.
                if room.later.rescheduling.is_none() {
                    room.compose = Compose::New;
                    room.set_composer(text_editor::Content::new());
                    room.attachments.clear();
                }
                room.later = Default::default();
                Task::none()
            }
            Later::Saved(Err(e)) | Later::Done(Err(e)) => {
                room.error = Some(e);
                Task::none()
            }
            Later::Done(Ok(())) => Task::none(),
        }
    }

    /// The PDF viewer follows the window's size and scale.
    fn pdf_resized(&mut self) -> Task<Message> {
        match &mut self.screen {
            Screen::Main(main) => main
                .pdf
                .as_mut()
                .map_or_else(Task::none, |pdf| pdf.resized(self.window, self.scale)),
            _ => Task::none(),
        }
    }

    /// Show the space and room of last time once the room is listed, until
    /// the first sync shows it is gone.
    fn restore_view(&mut self) -> Task<Message> {
        let Screen::Main(main) = &mut self.screen else {
            return Task::none();
        };
        let Some(view) = &main.restore else {
            return Task::none();
        };
        match view.found(&main.rooms.rooms) {
            Some((space, room)) => {
                main.restore = None;
                main.space = space.clone();
                // As on selecting it: the space's members, for its direct chats.
                let members = match space {
                    Some(space) => main.arrange.space_selected(&main.session, space),
                    None => Task::none(),
                };
                Task::batch([members, self.update(Message::OpenRoom(room))])
            }
            None => {
                if main.rooms.loaded {
                    main.restore = None;
                }
                Task::none()
            }
        }
    }

    /// Save the space and room shown, to show them again next time.
    fn remember_view(&self) {
        let (Screen::Main(main), Some(file)) = (&self.screen, &self.view_file) else {
            return;
        };
        let room = main
            .opening
            .as_ref()
            .or(main.room.as_ref().map(|r| &r.id))
            .cloned();
        LastView {
            space: main.space.clone(),
            room,
        }
        .save(file);
    }

    fn math_params(&self) -> math::Params {
        let theme = self.theme();
        let palette = theme.extended_palette();
        let max_width = self.window.width
            - SPACE_BAR_WIDTH
            - self.settings.room_list_width
            - RESIZE_HANDLE
            - MESSAGE_INSET;
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
            .avatars_on_screen(&self.settings)
            .into_iter()
            .map(|(mxc, size)| {
                main.avatars
                    .request(&main.session, &mxc, size, Message::AvatarLoaded)
            })
            .collect();
        Task::batch(tasks)
    }

    /// Request the previews of the open room's rendered images.
    fn load_previews(&mut self) -> Task<Message> {
        let Screen::Main(main) = &mut self.screen else {
            return Task::none();
        };
        let Some(room) = &main.room else {
            return Task::none();
        };
        let tasks: Vec<_> = room
            .previewed()
            .map(|media| {
                main.previews
                    .request(&main.session, media, Message::PreviewLoaded)
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
            subscriptions.push(
                Subscription::run_with(
                    (Shared(main.session.clone()), "notifications"),
                    |(s, _)| s.0.notifications(),
                )
                .map(Message::Notified),
            );
            subscriptions.push(iced::event::listen_with(shortcuts));
            // A selection being dragged ends wherever the button is released.
            if main
                .room
                .as_ref()
                .and_then(|r| r.selection.as_ref())
                .is_some_and(|s| s.dragging)
            {
                subscriptions.push(iced::event::listen_with(|event, _, _| match event {
                    iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                        Some(Message::Select(selection::Message::End))
                    }
                    _ => None,
                }));
            }
            if self.resizing {
                subscriptions.push(iced::event::listen_with(|event, _, _| match event {
                    iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                        Some(Message::Resize(Resize::Move(position.x)))
                    }
                    iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                        Some(Message::Resize(Resize::End))
                    }
                    _ => None,
                }));
            }
            subscriptions.push(
                Subscription::run_with(Shared(main.session.clone()), |s| s.0.connection_updates())
                    .map(Message::Connection),
            );
            subscriptions.push(
                Subscription::run_with(Shared(main.session.clone()), |s| {
                    s.0.room_categories_updates()
                })
                .map(|c| Message::Arrange(arrange::Message::Categories(c))),
            );
            subscriptions.extend(main.arrange.subscription());
            subscriptions.push(
                Subscription::run_with(Shared(main.session.clone()), |s| {
                    s.0.room_list_colours_updates()
                })
                .map(Message::Colours),
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
                subscriptions.push(Subscription::run_with(
                    (Shared(room.timeline.clone()), "typing"),
                    |(t, _)| {
                        let id = t.0.room_id().clone();
                        t.0.typing_updates()
                            .map(move |names| Message::Typing(id.clone(), names))
                    },
                ));
                subscriptions.push(Subscription::run_with(Shared(room.timeline.clone()), |t| {
                    let id = t.0.room_id().clone();
                    t.0.updates()
                        .map(move |entries| Message::Timeline(id.clone(), entries))
                }));
                subscriptions.push(media::dropped_files());
            }
        }
        Subscription::batch(subscriptions)
    }

    pub fn view(&self) -> Element<'_, Message> {
        match &self.screen {
            Screen::Starting => container(text("Starting…")).center(Length::Fill).into(),
            Screen::Login(login) => login.view().map(Message::Login),
            // The main view is always the stack's first layer, so opening
            // and closing what floats over it keeps its widgets' state.
            Screen::Main(main) => {
                let mut layers = stack![self.main_view(main)];
                let (panel, close) = if let Some(panel) = &self.settings_panel {
                    let spaces = sidebar::top_level_spaces(&main.rooms.rooms)
                        .into_iter()
                        .map(|s| (s.id.clone(), s.name.clone()))
                        .collect();
                    (
                        panel.view(&self.settings, spaces, &main.colours),
                        Message::ShowSettings(false),
                    )
                } else if let Some(dialog) = &main.dialog {
                    (
                        dialog.view(&main.rooms.rooms),
                        Message::Dialog(dialog::Message::Cancel),
                    )
                } else if let Some(card) = &main.info {
                    (
                        card.view(&main.rooms.rooms, &main.avatars, &main.own_user),
                        Message::Info(info::Message::Close),
                    )
                } else if let Some(viewer) = &main.viewer {
                    // Fills the window, with its own backdrop.
                    return layers.push(opaque(viewer.view(&main.previews))).into();
                } else if let Some(pdf) = &main.pdf {
                    return layers.push(opaque(pdf.view())).into();
                } else {
                    return layers.into();
                };
                let backdrop = mouse_area(
                    container(opaque(panel))
                        .center(Length::Fill)
                        .style(style::backdrop),
                )
                .on_press(close);
                layers = layers.push(opaque(backdrop));
                layers.into()
            }
        }
    }

    fn main_view<'a>(&'a self, main: &'a Main) -> Element<'a, Message> {
        let theme = self.theme();
        let rooms = &main.rooms.rooms;
        let space_bar = sidebar::space_bar(rooms, main.space.as_ref(), &main.avatars);
        let room_list = sidebar::room_list(main.room_list_view(&self.settings));

        // Banners come and go inside a column that is always there, so the
        // room view keeps its place and its widgets their state (the
        // composer its focus).
        let mut banners = column![];
        if main.verified == Some(false) {
            banners = banners.push(recovery_banner(main));
        }
        if let Some(banner) = overdue_banner(main) {
            banners = banners.push(banner);
        }
        if let Some(notice) = &main.notice {
            banners = banners.push(notice_bar(notice));
        }
        if let Some(error) = &main.error {
            banners = banners.push(container(text(error).style(text::danger)).padding(12));
        }
        let mut content = column![banners];
        let room_view: Element<'a, Message> = match (&main.invite, &main.opening, &main.room) {
            (Some((id, answering)), _, _) => {
                invite_view(rooms.iter().find(|r| &r.id == id), id, *answering)
            }
            (None, Some(id), _) => {
                let opening =
                    container(text("Opening…").style(text::secondary)).center(Length::Fill);
                // The member list stays where it is, loading.
                if main.show_members {
                    let name = rooms
                        .iter()
                        .find(|r| &r.id == id)
                        .map_or("", |r| r.name.as_str());
                    row![
                        opening,
                        members::view(None, id, name, &main.own_user, &main.avatars)
                    ]
                    .into()
                } else {
                    opening.into()
                }
            }
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
                    own_user: &main.own_user,
                    theme: theme.clone(),
                    math: &self.math,
                    params: self.math_params(),
                    avatars: &main.avatars,
                    colored_names: self.settings.colored_names,
                    call_services: &self.settings.call_services,
                    previews: &main.previews,
                    frequent_emoji: &main.frequent_emoji,
                    show_members: main.show_members,
                };
                room.view(summary, &look, &scheduled)
            }
            (None, None, None) => container(text("Select a room").style(text::secondary))
                .center(Length::Fill)
                .into(),
        };
        content = content.push(room_view);

        // The room list's edge, dragged to resize it.
        let handle = mouse_area(
            container(iced::widget::space())
                .width(RESIZE_HANDLE)
                .height(Length::Fill),
        )
        .interaction(mouse::Interaction::ResizingHorizontally)
        .on_press(Message::Resize(Resize::Start));
        let main_row = row![space_bar, room_list, handle, content.width(Length::Fill)];
        // Bars and the drag preview come after the rooms, keeping them in
        // place.
        let mut window = column![main_row];
        match main.connection {
            Connection::Online => {}
            Connection::Offline => window = window.push(offline_bar()),
            Connection::SignedOut => window = window.push(signed_out_bar()),
        }
        let mut layers = stack![window];
        if let Some(preview) = sidebar::drag_preview(rooms, &main.arrange, &main.avatars) {
            layers = layers.push(preview);
        }
        layers.into()
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
/// Along the bottom, near the composer, while the server is unreachable.
fn offline_bar<'a>() -> Element<'a, Message> {
    container(
        row![
            text("Offline – reconnecting…")
                .size(13)
                .font(crate::fonts::SEMIBOLD),
            text("Messages are sent once the server is back.")
                .size(13)
                .width(Length::Fill),
            button(text("Retry now").size(13))
                .padding([3, 10])
                .style(style::secondary_button)
                .on_press(Message::Reconnect),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center),
    )
    .padding([6, 16])
    .width(Length::Fill)
    .style(style::offline_bar)
    .into()
}

/// Along the bottom once the server no longer accepts the session.
fn signed_out_bar<'a>() -> Element<'a, Message> {
    container(
        row![
            text("Your session has ended")
                .size(13)
                .font(crate::fonts::SEMIBOLD),
            text("The server no longer accepts this login: it expired or was signed out elsewhere.")
                .size(13)
                .width(Length::Fill),
            hint(
                button(text("Sign in again").size(13))
                    .padding([3, 10])
                    .style(style::primary_button)
                    .on_press(Message::SignInAgain),
                "Remove this device's data and log in again (your recovery key unlocks older messages)",
                Position::Top,
            ),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center),
    )
    .padding([6, 16])
    .width(Length::Fill)
    .style(style::offline_bar)
    .into()
}

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

/// ⌘K / Ctrl+K focuses the room search, Esc closes the viewer or clears the
/// search; window focus.
fn shortcuts(
    event: iced::Event,
    status: iced::event::Status,
    _window: window::Id,
) -> Option<Message> {
    use iced::keyboard::{self, Key, key::Named};
    match event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => match key {
            // Unless a text field copies its own selection.
            Key::Character(c)
                if c == "c" && modifiers.command() && status == iced::event::Status::Ignored =>
            {
                Some(Message::Select(selection::Message::Copy))
            }
            Key::Character(c)
                if c == "a" && modifiers.command() && status == iced::event::Status::Ignored =>
            {
                Some(Message::Select(selection::Message::SelectAll))
            }
            Key::Character(c) if c == "k" && modifiers.command() => Some(Message::FocusSearch),
            Key::Character(c) if c == "r" && modifiers.command() => Some(Message::Reload),
            Key::Named(Named::Escape) => Some(Message::Escape),
            // Zoom in the PDF viewer.
            Key::Character(c) if (c == "=" || c == "+") && modifiers.command() => Some(
                Message::Pdf(pdf_viewer::Message::Zoom(pdf_viewer::Zoom::In)),
            ),
            Key::Character(c) if c == "-" && modifiers.command() => Some(Message::Pdf(
                pdf_viewer::Message::Zoom(pdf_viewer::Zoom::Out),
            )),
            Key::Character(c) if c == "0" && modifiers.command() => Some(Message::Pdf(
                pdf_viewer::Message::Zoom(pdf_viewer::Zoom::Fit),
            )),
            _ => None,
        },
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            Some(Message::Arrange(arrange::Message::Pressed))
        }
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::Arrange(arrange::Message::Released))
        }
        iced::Event::Window(window::Event::Focused) => Some(Message::WindowFocused(true)),
        iced::Event::Window(window::Event::Unfocused) => Some(Message::WindowFocused(false)),
        _ => None,
    }
}

/// Read files to attach to the message being written in `room`.
fn attach(room: OwnedRoomId, paths: Vec<PathBuf>) -> Task<Message> {
    if paths.is_empty() {
        return Task::none();
    }
    Task::perform(media::read_attached(paths), move |attached| {
        Message::Attached(room.clone(), attached)
    })
}

/// Load the room's members, for its member list and mentions, and whether
/// we may mention everyone.
fn load_members(session: &Arc<Session>, room: &mut Room) -> Task<Message> {
    room.loading_members = true;
    let (session, id) = (session.clone(), room.id.clone());
    Task::perform(
        {
            let id = id.clone();
            async move {
                let (members, everyone) =
                    futures_util::join!(session.members(&id), session.can_mention_room(&id));
                Ok((
                    members.map_err(|e| e.to_string())?,
                    everyone.unwrap_or(false),
                ))
            }
        },
        move |result| Message::MembersLoaded(id.clone(), result),
    )
}

/// Show a system notification; true when it was clicked. The calls block,
/// so they run off the UI thread.
async fn notify(notification: Notification) -> bool {
    const MAX_BODY: usize = 200;
    let title = if notification.direct {
        notification.sender_name.clone()
    } else {
        format!("{} in {}", notification.sender_name, notification.room_name)
    };
    let body: String = notification.body.chars().take(MAX_BODY).collect();
    let shown = tokio::task::spawn_blocking(move || {
        let mut desktop = notify_rust::Notification::new();
        desktop.appname("Mtrx").summary(&title).body(&body);
        #[cfg(windows)]
        crate::windows::notification_sender(&mut desktop);
        // Freedesktop servers report clicks only for a "default" action.
        #[cfg(all(unix, not(target_os = "macos")))]
        desktop.action("default", "Open");
        let mut clicked = false;
        desktop
            .show()?
            .wait_for_response(|response: &notify_rust::NotificationResponse| {
                clicked = matches!(response, notify_rust::NotificationResponse::Default);
            })?;
        Ok::<_, notify_rust::error::Error>(clicked)
    })
    .await;
    match shown {
        Ok(Ok(clicked)) => clicked,
        Ok(Err(e)) => {
            tracing::warn!("could not show a notification: {e}");
            false
        }
        Err(_) => false,
    }
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

/// Load older events. Whether the start was reached shows as
/// [`TimelineEntry::Start`] in the entries.
fn paginate(room: &mut Room) -> Task<Message> {
    if room.paginating || room.at_start() {
        return Task::none();
    }
    room.paginating = true;
    let (id, timeline) = (room.id.clone(), room.timeline.clone());
    Task::perform(
        async move { timeline.paginate_back(30).await.map(|_| ()) },
        move |result| Message::Paginated(id.clone(), result.map_err(|e| e.to_string())),
    )
}

fn load_settings(session: &Arc<Session>, room: &OwnedRoomId) -> Task<Message> {
    let (session, room) = (session.clone(), room.clone());
    Task::perform(
        {
            let room = room.clone();
            async move {
                session
                    .room_settings(&room)
                    .await
                    .map_err(|e| e.to_string())
            }
        },
        move |result| Message::Info(info::Message::SettingsLoaded(room.clone(), result)),
    )
}

/// The user's most used emoji, from account data.
fn load_frequent_emoji(session: &Arc<Session>) -> Task<Message> {
    let session = session.clone();
    Task::perform(
        async move { session.frequent_emoji().await },
        Message::FrequentEmoji,
    )
}

/// Count a use of `emoji`: first among the most used here at once, and in
/// account data (shared with Element).
fn used_emoji(main: &mut Main, emoji: String) -> Task<Message> {
    main.frequent_emoji.retain(|e| *e != emoji);
    main.frequent_emoji.insert(0, emoji.clone());
    let session = main.session.clone();
    Task::future(async move {
        if let Err(e) = session.record_emoji(&emoji).await {
            tracing::warn!("could not record the emoji used: {e}");
        }
    })
    .discard()
}

/// Spaces whose members a dialog suggests first: the space a room is
/// created in, the spaces around a room people are invited to, or the
/// space shown.
fn near_spaces(
    kind: &dialog::Kind,
    rooms: &[mtrx_core::RoomSummary],
    shown: Option<&OwnedRoomId>,
) -> Vec<OwnedRoomId> {
    let around = |room: &OwnedRoomId| {
        let mut spaces = sidebar::ancestors(rooms, room);
        spaces.push(room.clone());
        spaces
    };
    match kind {
        dialog::Kind::NewRoom {
            space: Some((space, _)),
        } => around(space),
        dialog::Kind::Invite { room, .. } => around(room),
        _ => shown.map(around).unwrap_or_default(),
    }
}

/// Load older events while too few are loaded to fill the view.
fn fill(room: &mut Room) -> Task<Message> {
    if room.wants_older() {
        paginate(room)
    } else {
        Task::none()
    }
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
