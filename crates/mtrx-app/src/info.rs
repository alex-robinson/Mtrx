//! Cards: who someone is, with their address to copy; a room's settings.
//! Opened by clicking a name or from the room and member lists.

use std::time::SystemTime;

use iced::{
    Alignment, Element, Length,
    widget::{button, checkbox, column, container, row, scrollable, text, text_input, toggler},
};
use mtrx_core::{
    Access, CallService, MxcUri, NotifyMode, OwnedMxcUri, OwnedRoomId, OwnedUserId, RoomKind,
    RoomSettings, RoomSummary, RoomTag, Topic, UserInfo, UserSummary,
};

use crate::{
    app, arrange,
    avatar::{Avatars, Shape},
    dialog::Kind,
    fonts, frame,
    presence::{self, Presences},
    selectable::selectable_text,
    settings::{self, Preference},
    sidebar, style, topic,
};

/// The picture in the card's header.
const AVATAR: f32 = 40.0;
const WIDTH: f32 = 460.0;
/// Height of a person's shared rooms list; more scroll.
const SHARED_HEIGHT: f32 = 200.0;

/// What a card shows, besides its target: the rooms we know, pictures, who
/// we are, people's presence as of `now`, whether the Notes are pinned at
/// the top (our own card switches it), and our call services, naming a
/// room's calls.
#[derive(Clone, Copy)]
pub struct Shown<'a> {
    pub rooms: &'a [RoomSummary],
    pub avatars: &'a Avatars,
    pub own_user: &'a OwnedUserId,
    pub presences: &'a Presences,
    pub now: SystemTime,
    pub pinned_notes: bool,
    pub call_services: &'a [CallService],
}

/// What to show a card for.
#[derive(Debug, Clone)]
pub enum Target {
    /// With the name and picture known so far.
    User(UserSummary),
    /// The room's settings.
    Room(OwnedRoomId),
}

#[derive(Debug, Clone)]
pub enum Message {
    UserLoaded(OwnedUserId, Result<UserInfo, String>),
    SettingsLoaded(OwnedRoomId, Result<RoomSettings, String>),
    Copy(String),
    Name(String),
    Topic(topic::Edit),
    /// Save the changed name and topic.
    SaveDetails,
    Notify(NotifyMode),
    Access(Access),
    ChangePicture,
    RemovePicture,
    /// A change was saved on the server, or not.
    Saved(Result<(), String>),
    Close,
}

/// A card's parts, for its [`frame::frame`].
struct Framed<'a> {
    title: Element<'a, app::Message>,
    action: Option<Element<'a, app::Message>>,
    body: Element<'a, app::Message>,
}

pub struct Card {
    pub target: Target,
    /// Spaces and rooms shared with the person, once loaded.
    shared: Option<Vec<OwnedRoomId>>,
    /// The room's settings, once loaded.
    pub settings: Option<RoomSettings>,
    /// The room's name and topic as edited.
    name: String,
    topic: topic::Editor,
    /// A change is being saved.
    pub saving: bool,
    error: Option<String>,
    /// The text last copied, to confirm it.
    copied: Option<String>,
}

impl Card {
    pub fn new(target: Target, rooms: &[RoomSummary]) -> Self {
        let room = match &target {
            Target::Room(id) => rooms.iter().find(|r| &r.id == id),
            Target::User(_) => None,
        };
        Self {
            name: room.map(|r| r.name.clone()).unwrap_or_default(),
            topic: topic::Editor::new(room.and_then(|r| r.topic.as_deref()).unwrap_or_default()),
            target,
            shared: None,
            settings: None,
            saving: false,
            error: None,
            copied: None,
        }
    }

    /// The user whose details still need loading.
    pub fn user(&self) -> Option<&OwnedUserId> {
        match &self.target {
            Target::User(user) => Some(&user.id),
            Target::Room(_) => None,
        }
    }

    /// The room whose settings are shown.
    pub fn room(&self) -> Option<&OwnedRoomId> {
        match &self.target {
            Target::Room(id) => Some(id),
            Target::User(_) => None,
        }
    }

    /// The card's picture, with its size.
    pub fn avatar<'a>(&'a self, rooms: &'a [RoomSummary]) -> Option<(&'a MxcUri, f32)> {
        let mxc = match &self.target {
            Target::User(user) => user.avatar.as_deref(),
            Target::Room(id) => rooms.iter().find(|r| &r.id == id)?.avatar.as_deref(),
        };
        Some((mxc?, AVATAR))
    }

    /// The edited name and topic, if they differ from `room`'s.
    pub fn changed_details(&self, room: &RoomSummary) -> (Option<String>, Option<String>) {
        let name = self.name.trim();
        (
            (!name.is_empty() && name != room.name).then(|| name.to_owned()),
            (self.topic.changed(room.topic.as_deref())).then(|| self.topic.topic()),
        )
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::UserLoaded(id, Ok(info)) => {
                if let Target::User(user) = &mut self.target
                    && user.id == id
                {
                    // Keep what we knew if the profile is not readable.
                    user.name = info.user.name.or(user.name.take());
                    user.avatar = info.user.avatar.or(user.avatar.take());
                    self.shared = Some(info.shared);
                }
            }
            Message::SettingsLoaded(id, Ok(settings)) => {
                if self.room() == Some(&id) {
                    self.settings = Some(settings);
                }
            }
            Message::UserLoaded(_, Err(e)) | Message::SettingsLoaded(_, Err(e)) => {
                self.error = Some(e)
            }
            Message::Copy(copied) => self.copied = Some(copied),
            Message::Name(name) => self.name = name,
            Message::Topic(edit) => self.topic.update(edit),
            Message::Notify(mode) => {
                if let Some(settings) = &mut self.settings {
                    settings.notify = mode;
                }
                self.saving = true;
                self.error = None;
            }
            Message::Access(access) => {
                if let Some(settings) = &mut self.settings {
                    settings.access = access;
                }
                self.saving = true;
                self.error = None;
            }
            Message::SaveDetails | Message::ChangePicture | Message::RemovePicture => {
                self.saving = true;
                self.error = None;
            }
            Message::Saved(result) => {
                self.saving = false;
                self.error = result.err();
            }
            Message::Close => {}
        }
    }

    pub fn view<'a>(&'a self, shown: Shown<'a>) -> Element<'a, app::Message> {
        let Framed {
            title,
            action,
            body,
        } = match &self.target {
            Target::User(user) => self.user_view(user, shown),
            Target::Room(id) => match shown.rooms.iter().find(|r| &r.id == id) {
                Some(room) => self.room_view(room, shown),
                None => Framed {
                    title: frame::title("Room"),
                    action: None,
                    body: text("This room is not in your list.").into(),
                },
            },
        };
        frame::frame(
            title,
            action,
            app::Message::Info(Message::Close),
            body,
            WIDTH,
        )
    }

    /// The header's main button for the room: Save once the name or topic
    /// changed, else Open (a room, not a space).
    fn room_action<'a>(&self, room: &RoomSummary) -> Option<Element<'a, app::Message>> {
        let (name, topic) = self.changed_details(room);
        if name.is_some() || topic.is_some() {
            Some(frame::primary(
                "Save",
                (!self.saving && self.topic.valid())
                    .then_some(app::Message::Info(Message::SaveDetails)),
            ))
        } else {
            (room.kind != RoomKind::Space && !room.invited)
                .then(|| frame::primary("Open", Some(app::Message::OpenRoom(room.id.clone()))))
        }
    }

    /// Saves the changed name and topic (⌘↩), if any.
    pub fn submit(&self, rooms: &[RoomSummary]) -> Option<app::Message> {
        let room = rooms.iter().find(|r| Some(&r.id) == self.room())?;
        let (name, topic) = self.changed_details(room);
        ((name.is_some() || topic.is_some()) && !self.saving && self.topic.valid())
            .then_some(app::Message::Info(Message::SaveDetails))
    }

    fn user_view<'a>(&'a self, user: &'a UserSummary, shown: Shown<'a>) -> Framed<'a> {
        let Shown {
            rooms,
            avatars,
            own_user,
            presences,
            now,
            pinned_notes,
            ..
        } = shown;
        let presence = presences.get(&user.id);
        let name = user.name.as_deref().unwrap_or(user.id.localpart());
        // The star favourites the direct chat, made if there is none.
        let chat = rooms
            .iter()
            .find(|r| r.kind == RoomKind::Direct && r.direct_user.as_ref() == Some(&user.id));
        let star = match chat {
            Some(chat) => sidebar::star_button(&chat.id, chat.tag == RoomTag::Favourite),
            None => sidebar::star_for(app::Message::FavouritePerson(user.id.clone()), false),
        };
        let picture = avatars.view(
            user.id.as_str(),
            name,
            user.avatar.as_deref(),
            AVATAR,
            Shape::Circle,
        );
        let picture = presence::with_dot(picture, presence, AVATAR, |theme| {
            theme.extended_palette().background.base.color
        });
        let mut title = column![selectable_text(name).size(18).font(fonts::SEMIBOLD)].spacing(2);
        if let Some(line) = presence.and_then(|p| presence::line(p, now)) {
            title = title.push(text(line).size(13).style(text::secondary));
        }
        let title = row![picture, title.width(Length::Fill), star]
            .spacing(12)
            .align_y(Alignment::Center);
        let mut card = column![self.copyable("User ID", user.id.to_string())].spacing(14);
        let shared = match &self.shared {
            None if self.error.is_none() => Some(text("Loading…").size(13).style(text::secondary)),
            None => None,
            Some(shared) if shared.is_empty() => Some(
                text("You share no rooms or spaces.")
                    .size(13)
                    .style(text::secondary),
            ),
            Some(_) => None,
        };
        if let Some(shared) = shared {
            card = card.push(shared);
        } else if let Some(shared) = &self.shared {
            card = card.push(shared_view(shared, rooms));
        }
        if let Some(error) = &self.error {
            card = card.push(text(error).size(12).style(text::danger));
        }
        // Our own card is the Notes': where they are pinned too.
        let message = if user.id == *own_user {
            card = card.push(settings::option(
                "Notes at the top",
                "Your notes to self stay at the top of the room list, in every space.",
                toggler(pinned_notes)
                    .on_toggle(|on| settings::preference(Preference::PinnedNotes(on)))
                    .into(),
            ));
            "Notes to self"
        } else {
            "Message"
        };
        Framed {
            title: title.into(),
            action: Some(frame::primary(
                message,
                Some(app::Message::ChatWith(user.id.clone())),
            )),
            body: card.into(),
        }
    }

    /// Picture, name and topic (editable where allowed), favourite and low
    /// priority, notifications, the address, and invite and leave.
    fn room_view<'a>(&'a self, room: &'a RoomSummary, shown: Shown<'a>) -> Framed<'a> {
        let Shown {
            rooms,
            avatars,
            call_services,
            ..
        } = shown;
        let settings = self.settings.as_ref();
        let allowed = |can: fn(&RoomSettings) -> bool| settings.is_some_and(can) && !self.saving;
        let star = sidebar::star_button(&room.id, room.tag == RoomTag::Favourite);
        let title = heading(
            avatars.view(
                room.id.as_str(),
                &room.name,
                room.avatar.as_deref(),
                AVATAR,
                Shape::Circle,
            ),
            &room.name,
            star,
        );
        let mut card = column![].spacing(14);

        if allowed(|s| s.can_change_picture) {
            let mut picture = row![
                button(text("Change picture…").size(12))
                    .padding([3, 10])
                    .style(style::secondary_button)
                    .on_press(app::Message::Info(Message::ChangePicture))
            ]
            .spacing(6);
            if room.avatar.is_some() {
                picture = picture.push(
                    button(text("Remove").size(12))
                        .padding([3, 10])
                        .style(style::text_button)
                        .on_press(app::Message::Info(Message::RemovePicture)),
                );
            }
            card = card.push(picture);
        }

        // Editable where allowed, else text to select from.
        let name: Element<'a, app::Message> = if allowed(|s| s.can_rename) {
            text_input("Room name", &self.name)
                .style(style::input)
                .padding([5, 8])
                .size(13)
                .on_input(|value| app::Message::Info(Message::Name(value)))
                .on_submit(app::Message::Info(Message::SaveDetails))
                .into()
        } else {
            selectable_text(self.name.as_str()).size(13).into()
        };
        let topic: Element<'a, app::Message> = if allowed(|s| s.can_change_topic) {
            (self.topic.view(call_services)).map(|edit| app::Message::Info(Message::Topic(edit)))
        } else {
            let topic = Topic::parse(room.topic.as_deref().unwrap_or_default());
            topic::details(&topic, call_services)
        };
        // The topic's text and links in one box.
        let topic = container(topic)
            .width(Length::Fill)
            .padding(10)
            .style(style::section);
        card = card
            .push(labelled("Name", name))
            .push(labelled("Topic", topic));

        if let Some(settings) = settings {
            // The chosen one stays enabled to look chosen, not greyed.
            let choice = |label, chosen: bool, message: Message| {
                button(text(label).size(13).center().width(Length::Fill))
                    .width(Length::Fill)
                    .padding([4, 8])
                    .style(move |theme, status| {
                        if chosen {
                            style::primary_button(theme, status)
                        } else {
                            style::secondary_button(theme, status)
                        }
                    })
                    .on_press_maybe((!self.saving).then_some(app::Message::Info(message)))
            };
            let notify =
                |label, mode| choice(label, settings.notify == mode, Message::Notify(mode));
            card = card.push(labelled(
                "Notifications",
                row![
                    notify("All messages", NotifyMode::All),
                    notify("Mentions", NotifyMode::Mentions),
                    notify("Mute", NotifyMode::Mute),
                ]
                .spacing(4),
            ));
            if settings.can_change_access && room.kind != RoomKind::Direct {
                let access = |label, access| {
                    choice(label, settings.access == access, Message::Access(access))
                };
                let mut choices = row![access("Invited people", Access::Invite)].spacing(4);
                // Space members only for a room in a space.
                if !room.parents.is_empty() || settings.access == Access::SpaceMembers {
                    choices = choices.push(access("Space members", Access::SpaceMembers));
                }
                card = card.push(labelled(
                    "Who can join",
                    choices.push(access("Anyone", Access::Public)),
                ));
            }
        }
        let low_priority = room.tag == RoomTag::LowPriority;
        card = card.push(
            checkbox(low_priority)
                .label("Low priority")
                .size(16)
                .text_size(13)
                .on_toggle(|on| {
                    app::Message::Arrange(arrange::Message::Tag(
                        room.id.clone(),
                        if on {
                            RoomTag::LowPriority
                        } else {
                            RoomTag::None
                        },
                    ))
                }),
        );

        let address = room.alias.clone().unwrap_or_else(|| room.id.to_string());
        card = card
            .push(self.copyable(
                if room.alias.is_some() {
                    "Address"
                } else {
                    "Room ID"
                },
                address.clone(),
            ))
            .push(self.copyable("Link", format!("https://matrix.to/#/{address}")));
        let kind = match room.kind {
            RoomKind::Space => "Space",
            RoomKind::Direct => "Direct chat",
            RoomKind::Room => "Room",
        };
        let members = match room.joined_members {
            1 => "1 member".to_owned(),
            n => format!("{n} members"),
        };
        let encryption = if room.encrypted {
            "encrypted"
        } else {
            "not encrypted"
        };
        let mut facts = vec![kind.to_owned()];
        if room.kind != RoomKind::Direct {
            let access = settings.map_or(
                if room.public {
                    Access::Public
                } else {
                    Access::Invite
                },
                |s| s.access,
            );
            facts.push(
                match access {
                    Access::Public => "Public: anyone can join",
                    Access::SpaceMembers => "Private: space members can join",
                    Access::Invite => "Private: by invitation",
                }
                .to_owned(),
            );
        }
        facts.extend([members, encryption.to_owned()]);
        card = card.push(
            selectable_text(facts.join(" · "))
                .size(13)
                .style(text::secondary),
        );
        let parents: Vec<&str> = room
            .parents
            .iter()
            .filter_map(|p| rooms.iter().find(|r| &r.id == p))
            .map(|r| r.name.as_str())
            .collect();
        if !parents.is_empty() {
            card = card.push(
                selectable_text(format!("In {}", parents.join(", ")))
                    .size(13)
                    .style(text::secondary),
            );
        }
        if let Some(error) = &self.error {
            card = card.push(text(error).size(12).style(text::danger));
        }

        // Invite in the members list, where the people are.
        let mut actions = row![].spacing(4);
        if !room.invited && settings.is_some_and(|s| s.can_invite) {
            let invite = if room.kind == RoomKind::Space {
                app::Message::OpenDialog(Kind::Invite {
                    room: room.id.clone(),
                    name: room.name.clone(),
                })
            } else {
                app::Message::ShowMembers(room.id.clone())
            };
            actions = actions.push(
                button(text("Invite…").size(13))
                    .style(style::text_button)
                    .on_press(invite),
            );
        }
        actions = actions.push(
            button(text("Leave…").size(13))
                .style(style::text_button)
                .on_press(app::Message::OpenDialog(Kind::Leave {
                    room: room.id.clone(),
                    name: room.name.clone(),
                })),
        );
        Framed {
            title,
            action: self.room_action(room),
            body: card.push(actions).into(),
        }
    }

    /// A label, the value and a button copying it.
    fn copyable<'a>(&self, label: &'a str, value: String) -> Element<'a, app::Message> {
        let copied = self.copied.as_ref() == Some(&value);
        labelled(
            label,
            row![
                container(
                    selectable_text(value.clone())
                        .size(13)
                        .font(iced::Font::MONOSPACE)
                        .wrapping(text::Wrapping::Glyph)
                )
                .width(Length::Fill)
                .padding([4, 8])
                .style(style::code_block),
                button(text(if copied { "Copied" } else { "Copy" }).size(12))
                    .padding([4, 10])
                    .style(style::secondary_button)
                    .on_press(app::Message::Info(Message::Copy(value))),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
    }
}

fn labelled<'a>(
    label: &'a str,
    content: impl Into<Element<'a, app::Message>>,
) -> Element<'a, app::Message> {
    column![
        text(label)
            .size(11)
            .font(fonts::SEMIBOLD)
            .style(text::secondary),
        content.into(),
    ]
    .spacing(4)
    .into()
}

fn heading<'a>(
    picture: Element<'a, app::Message>,
    name: &'a str,
    star: Element<'a, app::Message>,
) -> Element<'a, app::Message> {
    row![
        picture,
        container(selectable_text(name).size(18).font(fonts::SEMIBOLD)).width(Length::Fill),
        star,
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

/// Spaces, then rooms, shared with someone, all of them scrolling; each
/// opens.
fn shared_view<'a>(shared: &[OwnedRoomId], rooms: &'a [RoomSummary]) -> Element<'a, app::Message> {
    let mut known: Vec<&RoomSummary> = shared
        .iter()
        .filter_map(|id| rooms.iter().find(|r| &r.id == id))
        .collect();
    known.sort_by_key(|r| r.kind != RoomKind::Space);
    let title = format!("Shared spaces and rooms ({})", known.len());
    let mut list = column![].spacing(2).padding(iced::Padding::ZERO.right(10));
    for room in known {
        let target = match room.kind {
            RoomKind::Space => app::Message::SelectSpace(Some(room.id.clone())),
            _ => app::Message::OpenRoom(room.id.clone()),
        };
        list = list.push(
            button(text(&room.name).size(13))
                .width(Length::Fill)
                .padding([3, 8])
                .style(style::list_item(false))
                .on_press(target),
        );
    }
    column![
        text(title)
            .size(11)
            .font(fonts::SEMIBOLD)
            .style(text::secondary),
        container(scrollable(list).height(Length::Shrink)).max_height(SHARED_HEIGHT),
    ]
    .spacing(4)
    .into()
}

/// A card target for a room in the list: the person for a direct chat.
pub fn target_of(room: &RoomSummary) -> Target {
    match (&room.direct_user, room.kind) {
        (Some(user), RoomKind::Direct) => Target::User(UserSummary {
            id: user.clone(),
            name: Some(room.name.clone()),
            avatar: room.avatar.clone(),
        }),
        _ => Target::Room(room.id.clone()),
    }
}

/// A card target for a message's sender.
pub fn sender(id: OwnedUserId, name: Option<String>, avatar: Option<OwnedMxcUri>) -> Target {
    Target::User(UserSummary { id, name, avatar })
}
