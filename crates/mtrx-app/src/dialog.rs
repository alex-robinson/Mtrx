//! Dialogs for managing rooms: create a room, space, direct chat or
//! category, choose a category's rooms, join, invite, add rooms to a space,
//! leave.

use std::time::{Duration, Instant};

use iced::{
    Alignment, Element, Length,
    task::Handle,
    widget::{
        button, checkbox, column, container, row, scrollable, space, text, text_input, toggler,
    },
};
use mtrx_core::{
    Contact, NewRoom, OwnedRoomId, OwnedUserId, Permissions, RoomCategories, RoomSummary, UserId,
    UserSummary,
};

use crate::{
    app,
    arrange::{self, Arrange, CategoryEdit},
    fonts, sidebar, style,
};

/// Which dialog is open.
#[derive(Clone, Debug)]
pub enum Kind {
    /// In `space` if given.
    NewRoom {
        space: Option<(OwnedRoomId, String)>,
    },
    NewSpace,
    DirectChat,
    Join,
    Invite {
        room: OwnedRoomId,
        name: String,
    },
    AddExisting {
        space: OwnedRoomId,
        name: String,
    },
    Leave {
        room: OwnedRoomId,
        name: String,
    },
    SignOut,
    /// A new category of `space` (Home for `None`), with `room` in it if
    /// given, or the rooms of an existing one.
    Category {
        space: Option<OwnedRoomId>,
        category: Option<String>,
        room: Option<OwnedRoomId>,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    Name(String),
    Topic(String),
    Encrypted(bool),
    Moderated(bool),
    Address(String),
    /// The people search field changed.
    Query(String),
    /// People the user shares spaces, rooms or chats with.
    Contacts(Vec<Contact>),
    /// The server's user directory's answer to a query.
    Suggestions(String, Vec<UserSummary>),
    Pick(UserSummary),
    Unpick(usize),
    ToggleRoom(OwnedRoomId),
    /// Tick all of these rooms.
    TickAll(Vec<OwnedRoomId>),
    Submit,
    Cancel,
    /// The request finished; on success, possibly with a room to show.
    Done(Result<Opened, String>),
}

/// What to show once a dialog's request succeeded.
#[derive(Debug, Clone)]
pub enum Opened {
    Nothing,
    Room(OwnedRoomId),
    Space(OwnedRoomId),
    Left(OwnedRoomId),
    SignedOut,
}

/// What the app should do after a [`Message`].
pub enum Action {
    None,
    Close,
    /// Close, letting the running request finish in the background.
    Background,
    Search(String),
    Submit(Request),
    Finished(Opened),
    /// Close, and change the room list's arrangement.
    Arrange(arrange::Message),
}

/// A request for the server, made by the app.
pub enum Request {
    CreateRoom(NewRoom),
    CreateSpace(NewRoom),
    DirectChat(OwnedUserId),
    Join(String),
    Invite(OwnedRoomId, Vec<OwnedUserId>),
    AddToSpace(OwnedRoomId, Vec<OwnedRoomId>),
    Leave(OwnedRoomId),
    SignOut,
}

pub struct Dialog {
    kind: Kind,
    name: String,
    topic: String,
    encrypted: bool,
    moderated: bool,
    address: String,
    query: String,
    /// People the user knows, closest first.
    contacts: Vec<Contact>,
    /// Spaces whose members are suggested first.
    near: Vec<OwnedRoomId>,
    /// Ourselves, offered first for a direct chat: notes to self.
    own: Option<UserSummary>,
    /// The server's user directory's results for the query.
    directory: Vec<UserSummary>,
    people: Vec<UserSummary>,
    rooms: Vec<OwnedRoomId>,
    /// The running user search; replaced by each new query.
    pub search: Option<Handle>,
    /// When the request was sent, while it runs.
    busy: Option<Instant>,
    /// Refreshed by the app's tick, for the elapsed time.
    now: Instant,
    error: Option<String>,
}

/// After this long a request offers to continue in the background.
const SLOW: Duration = Duration::from_secs(15);

impl Dialog {
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            name: String::new(),
            topic: String::new(),
            encrypted: true,
            moderated: false,
            address: String::new(),
            query: String::new(),
            contacts: Vec::new(),
            near: Vec::new(),
            own: None,
            directory: Vec::new(),
            people: Vec::new(),
            rooms: Vec::new(),
            search: None,
            busy: None,
            now: Instant::now(),
            error: None,
        }
    }

    /// The dialog picks people.
    pub fn picks_people(&self) -> bool {
        matches!(
            self.kind,
            Kind::NewRoom { .. } | Kind::NewSpace | Kind::DirectChat | Kind::Invite { .. }
        )
    }

    /// Offer a chat with ourselves (notes to self) in a direct chat dialog.
    pub fn offer_self(&mut self, own: OwnedUserId) {
        if matches!(self.kind, Kind::DirectChat) {
            self.own = Some(UserSummary {
                id: own,
                name: Some("Notes to self".into()),
                avatar: None,
            });
        }
    }

    /// The name and rooms of the category being changed; the room to put
    /// into a new one.
    pub fn fill_category(&mut self, categories: &RoomCategories) {
        let Kind::Category {
            space,
            category,
            room,
        } = &self.kind
        else {
            return;
        };
        match category {
            Some(id) => {
                if let Some(category) = categories.layout(space.as_deref()).category(id) {
                    self.name = category.name.clone();
                    self.rooms = category.rooms.clone();
                }
            }
            None => self.rooms = room.iter().cloned().collect(),
        }
    }

    /// Suggest members of these spaces first.
    pub fn prefer_spaces(&mut self, spaces: Vec<OwnedRoomId>) {
        self.near = spaces;
    }

    /// People to suggest: those the user knows that match the query (the
    /// closest, and those in nearby spaces, first), then the server's
    /// directory results. Picked people are left out.
    fn suggestions(&self) -> (Vec<&UserSummary>, Vec<&UserSummary>) {
        const KNOWN: usize = 8;
        let query = self.query.trim().trim_start_matches('@').to_lowercase();
        let picked = |user: &UserSummary| self.people.iter().any(|p| p.id == user.id);
        let mut known: Vec<(bool, bool, &Contact)> = self
            .contacts
            .iter()
            .filter(|c| !picked(&c.user))
            .filter_map(|c| {
                let name = c.user.name.as_deref().unwrap_or_default().to_lowercase();
                let id = c.user.id.as_str()[1..].to_lowercase();
                let starts = name.starts_with(&query)
                    || name.split_whitespace().any(|w| w.starts_with(&query))
                    || id.starts_with(&query);
                (starts || name.contains(&query) || id.contains(&query)).then(|| {
                    let near = c.spaces.iter().any(|s| self.near.contains(s));
                    (!starts, !near, c)
                })
            })
            .collect();
        // Stable: within equal keys, the closest first.
        known.sort_by_key(|(later, far, _)| (*later, *far));
        let own = self.own.as_ref().filter(|own| {
            !picked(own)
                && ("notes to self".contains(&query)
                    || own.id.as_str()[1..].to_lowercase().contains(&query))
        });
        let known: Vec<&UserSummary> = own
            .into_iter()
            .chain(known.into_iter().take(KNOWN).map(|(_, _, c)| &c.user))
            .collect();
        let others = self
            .directory
            .iter()
            .filter(|u| !picked(u) && !known.iter().any(|k| k.id == u.id))
            .collect();
        (known, others)
    }

    /// A request is running.
    pub fn busy(&self) -> bool {
        self.busy.is_some()
    }

    pub fn tick(&mut self, now: Instant) {
        self.now = now;
    }

    /// Notices for when the request finishes in the background: on success,
    /// and the start of the message on failure.
    pub fn outcome(&self) -> (String, String) {
        let name = self.name.trim();
        let (done, failed) = match &self.kind {
            Kind::NewRoom { .. } => (
                format!("The room “{name}” is ready."),
                format!("Could not create the room “{name}”"),
            ),
            Kind::NewSpace => (
                format!("The space “{name}” is ready."),
                format!("Could not create the space “{name}”"),
            ),
            Kind::DirectChat => (
                "The direct chat is ready.".into(),
                "Could not start the direct chat".into(),
            ),
            Kind::Join => (
                format!("Joined {}.", self.address.trim()),
                format!("Could not join {}", self.address.trim()),
            ),
            Kind::Invite { name, .. } => (
                format!("Invites to {name} sent."),
                format!("Could not invite everyone to {name}"),
            ),
            Kind::AddExisting { name, .. } => (
                format!("Rooms added to {name}."),
                format!("Could not add all rooms to {name}"),
            ),
            Kind::Leave { name, .. } => {
                (format!("Left {name}."), format!("Could not leave {name}"))
            }
            Kind::SignOut => ("Signed out.".into(), "Could not sign out".into()),
            // Saved without waiting, like other changes to the room list.
            Kind::Category { .. } => (String::new(), String::new()),
        };
        (done, failed)
    }

    /// What is happening while the request runs.
    fn progress(&self, since: Instant) -> Element<'_, app::Message> {
        let doing = match &self.kind {
            Kind::NewRoom { .. } => "Creating the room on the server…",
            Kind::NewSpace => "Creating the space on the server…",
            Kind::DirectChat => "Starting the chat…",
            Kind::Join => "Joining…",
            Kind::Invite { .. } => "Sending the invites…",
            Kind::AddExisting { .. } => "Adding the rooms…",
            Kind::Leave { .. } => "Leaving…",
            Kind::SignOut => "Signing out…",
            Kind::Category { .. } => "Saving…",
        };
        let elapsed = self.now.saturating_duration_since(since);
        let mut status = column![
            text(if elapsed.as_secs() >= 3 {
                format!("{doing} {} s", elapsed.as_secs())
            } else {
                doing.to_owned()
            })
            .size(13)
        ]
        .spacing(4);
        if elapsed >= SLOW {
            status = status.push(
                text(
                    "The server is taking long. You can continue in the background; \
                     Mtrx tells you when it is done.",
                )
                .size(12)
                .style(text::secondary),
            );
        }
        status.into()
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Name(v) => self.name = v,
            Message::Topic(v) => self.topic = v,
            Message::Encrypted(v) => self.encrypted = v,
            Message::Moderated(v) => self.moderated = v,
            Message::Address(v) => self.address = v,
            Message::Query(query) => {
                self.query = query.clone();
                if let Some(search) = self.search.take() {
                    search.abort();
                }
                self.directory.clear();
                if self.picks_people() && query.trim().trim_start_matches('@').len() >= 2 {
                    return Action::Search(query.trim().to_owned());
                }
            }
            Message::Contacts(contacts) => self.contacts = contacts,
            Message::Suggestions(query, users) => {
                if query == self.query.trim() {
                    self.directory = users;
                }
            }
            Message::Pick(user) => {
                if matches!(self.kind, Kind::DirectChat) {
                    self.people.clear();
                }
                if !self.people.iter().any(|p| p.id == user.id) {
                    self.people.push(user);
                }
                self.query.clear();
                self.directory.clear();
            }
            Message::Unpick(index) if index < self.people.len() => {
                self.people.remove(index);
            }
            Message::Unpick(_) => {}
            Message::ToggleRoom(room) => match self.rooms.iter().position(|r| *r == room) {
                Some(index) => {
                    self.rooms.remove(index);
                }
                None => self.rooms.push(room),
            },
            Message::TickAll(rooms) => {
                for room in rooms {
                    if !self.rooms.contains(&room) {
                        self.rooms.push(room);
                    }
                }
            }
            Message::Submit if matches!(self.kind, Kind::Category { .. }) => {
                if let Some(edit) = self.category_edit() {
                    return Action::Arrange(arrange::Message::SaveCategory(edit));
                }
            }
            Message::Submit => {
                if let Some(request) = self.request().filter(|_| !self.busy()) {
                    self.busy = Some(Instant::now());
                    self.error = None;
                    return Action::Submit(request);
                }
            }
            Message::Cancel if self.busy() => return Action::Background,
            Message::Cancel => return Action::Close,
            Message::Done(Ok(opened)) => return Action::Finished(opened),
            Message::Done(Err(e)) => {
                self.busy = None;
                self.error = Some(e);
            }
        }
        Action::None
    }

    /// The request the dialog describes, if complete.
    fn request(&self) -> Option<Request> {
        let name = self.name.trim();
        let topic = self.topic.trim();
        let optional = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        let named = optional(name);
        let people = || self.people.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
        let permissions = if self.moderated {
            Permissions::Moderators
        } else {
            Permissions::Everyone
        };
        Some(match &self.kind {
            Kind::NewRoom { space } => Request::CreateRoom(NewRoom {
                name: Some(named.clone()?),
                topic: optional(topic),
                encrypted: self.encrypted,
                invite: people(),
                permissions,
                parent: space.as_ref().map(|(id, _)| id.clone()),
                ..Default::default()
            }),
            Kind::NewSpace => Request::CreateSpace(NewRoom {
                name: Some(named.clone()?),
                topic: optional(topic),
                space: true,
                invite: people(),
                permissions,
                ..Default::default()
            }),
            Kind::DirectChat => Request::DirectChat(self.person()?),
            Kind::Join => Request::Join(optional(self.address.trim())?),
            Kind::Invite { room, .. } => {
                let mut people = people();
                if people.is_empty() {
                    people.push(self.person()?);
                }
                Request::Invite(room.clone(), people)
            }
            Kind::AddExisting { space, .. } if !self.rooms.is_empty() => {
                Request::AddToSpace(space.clone(), self.rooms.clone())
            }
            Kind::AddExisting { .. } => return None,
            Kind::Leave { room, .. } => Request::Leave(room.clone()),
            Kind::SignOut => Request::SignOut,
            // Not for the server: see `category_edit`.
            Kind::Category { .. } => return None,
        })
    }

    /// The category the dialog describes, if named; rooms are optional.
    fn category_edit(&self) -> Option<CategoryEdit> {
        let Kind::Category {
            space, category, ..
        } = &self.kind
        else {
            return None;
        };
        let name = self.name.trim();
        (!name.is_empty()).then(|| CategoryEdit {
            space: space.clone(),
            category: category.clone(),
            name: name.to_owned(),
            rooms: self.rooms.clone(),
        })
    }

    /// The single person chosen, or a full user ID typed in.
    fn person(&self) -> Option<OwnedUserId> {
        self.people
            .first()
            .map(|p| p.id.clone())
            .or_else(|| OwnedUserId::try_from(self.query.trim()).ok())
    }

    /// `rooms` are the joined rooms, for adding existing ones to a space or
    /// putting them into a category (`arrange`).
    pub fn view<'a>(
        &'a self,
        rooms: &'a [RoomSummary],
        arrange: &'a Arrange,
    ) -> Element<'a, app::Message> {
        let (title, submit) = match &self.kind {
            Kind::NewRoom {
                space: Some((_, space)),
            } => (format!("New room in {space}"), "Create"),
            Kind::NewRoom { space: None } => ("New room".into(), "Create"),
            Kind::NewSpace => ("New space".into(), "Create"),
            Kind::DirectChat => ("New direct chat".into(), "Start chat"),
            Kind::Join => ("Join a room".into(), "Join"),
            Kind::Invite { name, .. } => (format!("Invite to {name}"), "Invite"),
            Kind::AddExisting { name, .. } => (format!("Add rooms to {name}"), "Add"),
            Kind::Leave { name, .. } => (format!("Leave {name}?"), "Leave"),
            Kind::SignOut => ("Sign out?".into(), "Sign out"),
            Kind::Category { category: None, .. } => ("New category".into(), "Create"),
            Kind::Category { .. } => ("Edit category".into(), "Save"),
        };
        let mut body = column![text(title).size(18).font(fonts::SEMIBOLD)].spacing(12);
        body = match &self.kind {
            Kind::NewRoom { .. } | Kind::NewSpace => {
                let space = matches!(self.kind, Kind::NewSpace);
                let mut fields = column![
                    self.field("Name", &self.name, Message::Name),
                    self.field("Topic (optional)", &self.topic, Message::Topic),
                ]
                .spacing(8);
                if !space {
                    fields = fields.push(option(
                        "Encrypted",
                        "Only members can read messages. Cannot be turned off later.",
                        toggler(self.encrypted).on_toggle(|v| message(Message::Encrypted(v))),
                    ));
                }
                fields = fields.push(option(
                    "Only moderators can edit",
                    "Otherwise everyone can invite, rename and change the topic.",
                    toggler(self.moderated).on_toggle(|v| message(Message::Moderated(v))),
                ));
                body.push(fields)
                    .push(self.people_view("Invite people (optional)"))
            }
            Kind::DirectChat => body.push(self.people_view("Name or @user:server")),
            Kind::Invite { .. } => body.push(self.people_view("Name or @user:server")),
            Kind::Join => body.push(
                column![
                    self.field(
                        "#room:server or !id:server",
                        &self.address,
                        Message::Address
                    ),
                    text("Rooms you are invited to are in the room list.")
                        .size(12)
                        .style(text::secondary),
                ]
                .spacing(4),
            ),
            Kind::AddExisting { space, .. } => body.push(self.rooms_view(space, rooms)),
            Kind::Category { space, .. } => body
                .push(self.field("Name", &self.name, Message::Name))
                .push(self.category_rooms_view(space.as_ref(), rooms, arrange)),
            Kind::Leave { .. } => body.push(
                text("You can only come back if the room is public or someone invites you.")
                    .size(13),
            ),
            Kind::SignOut => body.push(
                text(
                    "This device's data, including its message keys, is deleted. To read \
                     encrypted messages after signing in again, you need your recovery key \
                     or another signed-in device.",
                )
                .size(13),
            ),
        };
        if let Some(since) = self.busy {
            body = body.push(self.progress(since));
        }
        if let Some(error) = &self.error {
            body = body.push(text(error).size(12).style(text::danger));
        }
        let ready = (self.request().is_some() || self.category_edit().is_some()) && !self.busy();
        let slow = self
            .busy
            .is_some_and(|since| self.now.saturating_duration_since(since) >= SLOW);
        let cancel = match (self.busy(), slow) {
            (false, _) => Some("Cancel"),
            (true, true) => Some("Continue in background"),
            (true, false) => None,
        };
        body = body.push(
            row![
                space::horizontal(),
                button(text(cancel.unwrap_or("Cancel")))
                    .style(style::text_button)
                    .on_press_maybe(cancel.map(|_| message(Message::Cancel))),
                button(text(submit))
                    .style(if matches!(self.kind, Kind::Leave { .. } | Kind::SignOut) {
                        style::danger_button
                    } else {
                        style::primary_button
                    })
                    .on_press_maybe(ready.then(|| message(Message::Submit))),
            ]
            .spacing(8),
        );
        container(body)
            .padding(20)
            .width(480)
            .style(style::dialog)
            .into()
    }

    fn field<'a>(
        &self,
        placeholder: &'a str,
        value: &'a str,
        on_input: fn(String) -> Message,
    ) -> Element<'a, app::Message> {
        text_input(placeholder, value)
            .on_input(move |v| message(on_input(v)))
            .on_submit(message(Message::Submit))
            .style(style::input)
            .padding(8)
            .into()
    }

    /// Chosen people, the search field and its suggestions.
    fn people_view<'a>(&'a self, placeholder: &'a str) -> Element<'a, app::Message> {
        let mut section = column![].spacing(6);
        if !self.people.is_empty() {
            let chips = self.people.iter().enumerate().map(|(i, person)| {
                button(text(format!("{} ×", label(person))).size(13))
                    .padding([3, 8])
                    .style(style::secondary_button)
                    .on_press(message(Message::Unpick(i)))
                    .into()
            });
            section = section.push(row(chips).spacing(6).wrap());
        }
        let single = matches!(self.kind, Kind::DirectChat) && !self.people.is_empty();
        if !single {
            section = section.push(self.field(placeholder, &self.query, Message::Query));
        }
        if single {
            return section.into();
        }
        let (known, others) = self.suggestions();
        let mut list = column![].spacing(2);
        if !known.is_empty() {
            list = list.push(list_heading("People you know"));
            for user in known {
                list = list.push(person(user));
            }
        }
        if !others.is_empty() {
            list = list.push(list_heading("Others on the server"));
            for user in others {
                list = list.push(person(user));
            }
        }
        // A full user ID can be picked even if no one by it was found.
        let query = self.query.trim();
        let listed = |id: &UserId| {
            self.people.iter().any(|p| p.id == id)
                || self.contacts.iter().any(|c| c.user.id == id)
                || self.directory.iter().any(|u| u.id == id)
                || self.own.as_ref().is_some_and(|o| o.id == id)
        };
        if let Ok(id) = UserId::parse(query)
            && !listed(&id)
        {
            list = list.push(person(&UserSummary {
                id,
                name: None,
                avatar: None,
            }));
        }
        section = section.push(scrollable(list).height(Length::Shrink));
        section.into()
    }

    /// Joined rooms not yet in `space`, to tick.
    fn rooms_view<'a>(
        &'a self,
        space: &OwnedRoomId,
        rooms: &'a [RoomSummary],
    ) -> Element<'a, app::Message> {
        let candidates: Vec<_> = rooms
            .iter()
            .filter(|r| !r.invited && r.id != *space && !r.parents.contains(space))
            .collect();
        if candidates.is_empty() {
            return text("All your rooms are already in this space.")
                .size(13)
                .into();
        }
        let list = candidates.into_iter().map(|room| {
            let chosen = self.rooms.contains(&room.id);
            button(text(&room.name))
                .width(Length::Fill)
                .padding([4, 8])
                .style(style::list_item(chosen))
                .on_press(message(Message::ToggleRoom(room.id.clone())))
                .into()
        });
        scrollable(column(list).spacing(2)).height(260).into()
    }
}

impl Dialog {
    /// The rooms and chats `space` lists that the search finds, to tick,
    /// with the category each is in now.
    fn category_rooms_view<'a>(
        &'a self,
        space: Option<&OwnedRoomId>,
        rooms: &'a [RoomSummary],
        arrange: &'a Arrange,
    ) -> Element<'a, app::Message> {
        let layout = arrange.categories.layout(space.map(|s| s.as_ref()));
        let editing = match &self.kind {
            Kind::Category { category, .. } => category.as_deref(),
            _ => None,
        };
        let found: Vec<_> =
            sidebar::search(rooms, space, arrange, &self.query.trim().to_lowercase())
                .into_iter()
                .filter(|r| !r.invited)
                .collect();
        let unticked: Vec<OwnedRoomId> = (found.iter())
            .filter(|r| !self.rooms.contains(&r.id))
            .map(|r| r.id.clone())
            .collect();
        let chosen = match self.rooms.len() {
            0 => "No rooms chosen; you can also drag them in later.".to_owned(),
            1 => "1 room chosen".to_owned(),
            n => format!("{n} rooms chosen"),
        };
        let tick_all = button(text("Choose all shown").size(13))
            .padding([3, 8])
            .style(style::secondary_button)
            .on_press_maybe((!unticked.is_empty()).then(|| message(Message::TickAll(unticked))));
        let list = found.into_iter().map(|room| {
            let ticked = self.rooms.contains(&room.id);
            let toggle = message(Message::ToggleRoom(room.id.clone()));
            let mut line = row![
                checkbox(ticked).on_toggle({
                    let toggle = toggle.clone();
                    move |_| toggle.clone()
                }),
                text(&room.name).width(Length::Fill),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            // Ticking moves a room out of the category it is in.
            if let Some(other) = layout
                .category_of(&room.id)
                .filter(|c| Some(c.id.as_str()) != editing)
            {
                line = line.push(
                    text(format!("in {}", other.name))
                        .size(12)
                        .style(text::secondary),
                );
            }
            button(line)
                .width(Length::Fill)
                .padding([4, 8])
                .style(style::list_item(false))
                .on_press(toggle)
                .into()
        });
        column![
            self.field("Search rooms and chats", &self.query, Message::Query),
            row![
                text(chosen)
                    .size(12)
                    .style(text::secondary)
                    .width(Length::Fill),
                tick_all,
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            scrollable(column(list).spacing(2)).height(260),
        ]
        .spacing(8)
        .into()
    }
}

fn list_heading<'a>(heading: &'a str) -> Element<'a, app::Message> {
    container(
        text(heading)
            .size(11)
            .font(fonts::SEMIBOLD)
            .style(text::secondary),
    )
    .padding([4, 8])
    .into()
}

/// A suggested person; picks them.
fn person<'a>(user: &UserSummary) -> Element<'a, app::Message> {
    button(
        column![
            text(label(user)),
            text(user.id.to_string()).size(12).style(text::secondary),
        ]
        .spacing(1),
    )
    .width(Length::Fill)
    .padding([4, 8])
    .style(style::list_item(false))
    .on_press(message(Message::Pick(user.clone())))
    .into()
}

fn label(user: &UserSummary) -> String {
    user.name
        .clone()
        .unwrap_or_else(|| user.id.localpart().to_owned())
}

fn option<'a>(
    label: &'a str,
    description: &'a str,
    toggle: impl Into<Element<'a, app::Message>>,
) -> Element<'a, app::Message> {
    row![
        column![
            text(label),
            text(description).size(12).style(text::secondary)
        ]
        .spacing(2)
        .width(Length::Fill),
        toggle.into(),
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .into()
}

fn message(message: Message) -> app::Message {
    app::Message::Dialog(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(id: &str) -> OwnedRoomId {
        format!("!{id}:x").try_into().unwrap()
    }

    fn saved(action: Action) -> CategoryEdit {
        match action {
            Action::Arrange(arrange::Message::SaveCategory(edit)) => edit,
            _ => panic!("not saved"),
        }
    }

    #[test]
    fn a_category_needs_a_name_only() {
        let mut dialog = Dialog::new(Kind::Category {
            space: None,
            category: None,
            room: Some(room("seminar")),
        });
        dialog.fill_category(&RoomCategories::default());
        // The search is local: no user directory.
        assert!(matches!(
            dialog.update(Message::Query("pap".into())),
            Action::None
        ));
        assert!(matches!(dialog.update(Message::Submit), Action::None));
        dialog.update(Message::Name(" Papers ".into()));
        dialog.update(Message::TickAll(vec![room("paper-a"), room("seminar")]));
        dialog.update(Message::ToggleRoom(room("seminar")));
        let edit = saved(dialog.update(Message::Submit));
        assert_eq!(edit.name, "Papers");
        assert_eq!(edit.category, None);
        assert_eq!(edit.rooms, [room("paper-a")]);
    }

    #[test]
    fn editing_starts_from_the_category() {
        let mut categories = RoomCategories::default();
        let id = categories.home.add_category("Teaching");
        categories.home.place(&room("seminar"), Some(&id));
        let mut dialog = Dialog::new(Kind::Category {
            space: None,
            category: Some(id.clone()),
            room: None,
        });
        dialog.fill_category(&categories);
        // No rooms is fine too.
        dialog.update(Message::ToggleRoom(room("seminar")));
        let edit = saved(dialog.update(Message::Submit));
        assert_eq!(edit.name, "Teaching");
        assert_eq!(edit.category, Some(id));
        assert!(edit.rooms.is_empty());
    }
}
