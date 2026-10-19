//! Dialogs for managing rooms: create a room, space or direct chat, join,
//! invite, edit name and topic, add rooms to a space, leave.

use std::time::{Duration, Instant};

use iced::{
    Alignment, Element, Length,
    task::Handle,
    widget::{button, column, container, row, scrollable, space, text, text_input, toggler},
};
use mtrx_core::{NewRoom, OwnedRoomId, OwnedUserId, Permissions, RoomSummary, UserSummary};

use crate::{app, fonts, style};

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
    Edit {
        room: OwnedRoomId,
        name: String,
        topic: String,
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
    Suggestions(String, Vec<UserSummary>),
    Pick(UserSummary),
    Unpick(usize),
    ToggleRoom(OwnedRoomId),
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
}

/// A request for the server, made by the app.
pub enum Request {
    CreateRoom(NewRoom),
    CreateSpace(NewRoom),
    DirectChat(OwnedUserId),
    Join(String),
    Invite(OwnedRoomId, Vec<OwnedUserId>),
    Edit {
        room: OwnedRoomId,
        name: Option<String>,
        topic: Option<String>,
    },
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
    suggestions: Vec<UserSummary>,
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
        let (name, topic) = match &kind {
            Kind::Edit { name, topic, .. } => (name.clone(), topic.clone()),
            _ => Default::default(),
        };
        Self {
            kind,
            name,
            topic,
            encrypted: true,
            moderated: false,
            address: String::new(),
            query: String::new(),
            suggestions: Vec::new(),
            people: Vec::new(),
            rooms: Vec::new(),
            search: None,
            busy: None,
            now: Instant::now(),
            error: None,
        }
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
            Kind::Edit { .. } => ("Name and topic saved.".into(), "Could not save".into()),
            Kind::AddExisting { name, .. } => (
                format!("Rooms added to {name}."),
                format!("Could not add all rooms to {name}"),
            ),
            Kind::Leave { name, .. } => {
                (format!("Left {name}."), format!("Could not leave {name}"))
            }
            Kind::SignOut => ("Signed out.".into(), "Could not sign out".into()),
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
            Kind::Edit { .. } => "Saving…",
            Kind::AddExisting { .. } => "Adding the rooms…",
            Kind::Leave { .. } => "Leaving…",
            Kind::SignOut => "Signing out…",
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
                if query.trim().len() >= 2 {
                    return Action::Search(query.trim().to_owned());
                }
                self.suggestions.clear();
            }
            Message::Suggestions(query, users) => {
                if query == self.query.trim() {
                    self.suggestions = users;
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
                self.suggestions.clear();
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
            Kind::Edit {
                room,
                name: old_name,
                topic: old_topic,
            } => Request::Edit {
                room: room.clone(),
                name: (name != old_name.trim() && !name.is_empty()).then(|| name.to_owned()),
                topic: (topic != old_topic.trim()).then(|| topic.to_owned()),
            },
            Kind::AddExisting { space, .. } if !self.rooms.is_empty() => {
                Request::AddToSpace(space.clone(), self.rooms.clone())
            }
            Kind::AddExisting { .. } => return None,
            Kind::Leave { room, .. } => Request::Leave(room.clone()),
            Kind::SignOut => Request::SignOut,
        })
    }

    /// The single person chosen, or a full user ID typed in.
    fn person(&self) -> Option<OwnedUserId> {
        self.people
            .first()
            .map(|p| p.id.clone())
            .or_else(|| OwnedUserId::try_from(self.query.trim()).ok())
    }

    /// `rooms` are the joined rooms, for adding existing ones to a space.
    pub fn view<'a>(&'a self, rooms: &'a [RoomSummary]) -> Element<'a, app::Message> {
        let (title, submit) = match &self.kind {
            Kind::NewRoom {
                space: Some((_, space)),
            } => (format!("New room in {space}"), "Create"),
            Kind::NewRoom { space: None } => ("New room".into(), "Create"),
            Kind::NewSpace => ("New space".into(), "Create"),
            Kind::DirectChat => ("New direct chat".into(), "Start chat"),
            Kind::Join => ("Join a room".into(), "Join"),
            Kind::Invite { name, .. } => (format!("Invite to {name}"), "Invite"),
            Kind::Edit { .. } => ("Name and topic".into(), "Save"),
            Kind::AddExisting { name, .. } => (format!("Add rooms to {name}"), "Add"),
            Kind::Leave { name, .. } => (format!("Leave {name}?"), "Leave"),
            Kind::SignOut => ("Sign out?".into(), "Sign out"),
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
            Kind::Edit { .. } => body
                .push(self.field("Name", &self.name, Message::Name))
                .push(self.field("Topic", &self.topic, Message::Topic)),
            Kind::AddExisting { space, .. } => body.push(self.rooms_view(space, rooms)),
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
        let ready = self.request().is_some() && !self.busy();
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
        if !self.suggestions.is_empty() {
            let list = self.suggestions.iter().map(|user| {
                button(
                    column![
                        text(
                            user.name
                                .clone()
                                .unwrap_or_else(|| user.id.localpart().to_owned())
                        ),
                        text(user.id.to_string()).size(12).style(text::secondary),
                    ]
                    .spacing(1),
                )
                .width(Length::Fill)
                .padding([4, 8])
                .style(style::list_item(false))
                .on_press(message(Message::Pick(user.clone())))
                .into()
            });
            section = section.push(scrollable(column(list).spacing(2)).height(Length::Shrink));
        }
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
