//! Cards: who someone is, with their address to copy; a room's settings.
//! Opened by clicking a name or from the room and member lists.

use iced::{
    Alignment, Element, Length,
    widget::{button, checkbox, column, container, row, scrollable, space, text, text_input},
};
use mtrx_core::{
    Access, MxcUri, NotifyMode, OwnedMxcUri, OwnedRoomId, OwnedUserId, RoomKind, RoomSettings,
    RoomSummary, RoomTag, UserInfo, UserSummary,
};

use crate::{
    app, arrange,
    avatar::{Avatars, Shape},
    dialog::Kind,
    ellipsis::ellipsis,
    fonts, sidebar, style,
};

const AVATAR: f32 = 64.0;
/// Height of a person's shared rooms list; more scroll.
const SHARED_HEIGHT: f32 = 200.0;

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
    Topic(String),
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

pub struct Card {
    pub target: Target,
    /// Spaces and rooms shared with the person, once loaded.
    shared: Option<Vec<OwnedRoomId>>,
    /// The room's settings, once loaded.
    pub settings: Option<RoomSettings>,
    /// The room's name and topic as edited.
    name: String,
    topic: String,
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
            topic: room.and_then(|r| r.topic.clone()).unwrap_or_default(),
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
        let topic = self.topic.trim();
        (
            (!name.is_empty() && name != room.name).then(|| name.to_owned()),
            (topic != room.topic.as_deref().unwrap_or_default()).then(|| topic.to_owned()),
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
            Message::Topic(topic) => self.topic = topic,
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

    pub fn view<'a>(
        &'a self,
        rooms: &'a [RoomSummary],
        avatars: &Avatars,
        own_user: &OwnedUserId,
    ) -> Element<'a, app::Message> {
        let body = match &self.target {
            Target::User(user) => self.user_view(user, rooms, avatars, own_user),
            Target::Room(id) => match rooms.iter().find(|r| &r.id == id) {
                Some(room) => self.room_view(room, rooms, avatars),
                None => text("This room is not in your list.").into(),
            },
        };
        // The scrollbar beside the content, not over it.
        let body = scrollable(body).direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().spacing(8),
        ));
        container(body)
            .padding(20)
            .width(440)
            .max_height(640)
            .style(style::dialog)
            .into()
    }

    fn user_view<'a>(
        &'a self,
        user: &'a UserSummary,
        rooms: &'a [RoomSummary],
        avatars: &Avatars,
        own_user: &OwnedUserId,
    ) -> Element<'a, app::Message> {
        let name = user.name.as_deref().unwrap_or(user.id.localpart());
        // The star favourites the direct chat, made if there is none.
        let chat = rooms
            .iter()
            .find(|r| r.kind == RoomKind::Direct && r.direct_user.as_ref() == Some(&user.id));
        let star = match chat {
            Some(chat) => sidebar::star_button(&chat.id, chat.tag == RoomTag::Favourite),
            None => sidebar::star_for(app::Message::FavouritePerson(user.id.clone()), false),
        };
        let mut card = column![
            heading(
                avatars,
                user.id.as_str(),
                name,
                user.avatar.as_deref(),
                star
            ),
            self.copyable("User ID", user.id.to_string()),
        ]
        .spacing(14);
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
        let message = if user.id == *own_user {
            "Notes to self"
        } else {
            "Message"
        };
        let actions = row![
            space::horizontal(),
            button(text(message))
                .style(style::primary_button)
                .on_press(app::Message::ChatWith(user.id.clone())),
        ]
        .spacing(8);
        card.push(actions.push(close())).into()
    }

    /// Picture, name and topic (editable where allowed), favourite and low
    /// priority, notifications, the address, and invite and leave.
    fn room_view<'a>(
        &'a self,
        room: &'a RoomSummary,
        rooms: &'a [RoomSummary],
        avatars: &Avatars,
    ) -> Element<'a, app::Message> {
        let settings = self.settings.as_ref();
        let allowed = |can: fn(&RoomSettings) -> bool| settings.is_some_and(can) && !self.saving;
        let star = sidebar::star_button(&room.id, room.tag == RoomTag::Favourite);
        let mut card = column![heading(
            avatars,
            room.id.as_str(),
            &room.name,
            room.avatar.as_deref(),
            star
        )]
        .spacing(14);

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

        let field =
            |label, value: &'a str, placeholder, can: bool, on_input: fn(String) -> Message| {
                let mut input = text_input(placeholder, value)
                    .style(style::input)
                    .padding([5, 8])
                    .size(13);
                if can {
                    input = input
                        .on_input(move |value| app::Message::Info(on_input(value)))
                        .on_submit(app::Message::Info(Message::SaveDetails));
                }
                labelled(label, input)
            };
        card = card
            .push(field(
                "Name",
                &self.name,
                "Room name",
                allowed(|s| s.can_rename),
                Message::Name,
            ))
            .push(field(
                "Topic",
                &self.topic,
                "What the room is about",
                allowed(|s| s.can_change_topic),
                Message::Topic,
            ));
        let (name, topic) = self.changed_details(room);
        if name.is_some() || topic.is_some() {
            card = card.push(row![
                space::horizontal(),
                button(text("Save").size(13))
                    .padding([4, 12])
                    .style(style::primary_button)
                    .on_press_maybe(
                        (!self.saving).then_some(app::Message::Info(Message::SaveDetails))
                    ),
            ]);
        }

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
        card = card.push(text(facts.join(" · ")).size(13).style(text::secondary));
        let parents: Vec<&str> = room
            .parents
            .iter()
            .filter_map(|p| rooms.iter().find(|r| &r.id == p))
            .map(|r| r.name.as_str())
            .collect();
        if !parents.is_empty() {
            card = card.push(
                text(format!("In {}", parents.join(", ")))
                    .size(13)
                    .style(text::secondary),
            );
        }
        if let Some(error) = &self.error {
            card = card.push(text(error).size(12).style(text::danger));
        }

        let mut actions = row![
            button(text("Leave…"))
                .style(style::text_button)
                .on_press(app::Message::OpenDialog(Kind::Leave {
                    room: room.id.clone(),
                    name: room.name.clone(),
                })),
            space::horizontal()
        ]
        .spacing(8);
        if !room.invited && settings.is_some_and(|s| s.can_invite) {
            actions = actions.push(
                button(text("Invite…"))
                    .style(style::secondary_button)
                    .on_press(app::Message::OpenDialog(Kind::Invite {
                        room: room.id.clone(),
                        name: room.name.clone(),
                    })),
            );
        }
        if room.kind != RoomKind::Space {
            actions = actions.push(
                button(text("Open"))
                    .style(style::primary_button)
                    .on_press(app::Message::OpenRoom(room.id.clone())),
            );
        }
        card.push(actions.push(close())).into()
    }

    /// A label, the value and a button copying it.
    fn copyable<'a>(&self, label: &'a str, value: String) -> Element<'a, app::Message> {
        let copied = self.copied.as_ref() == Some(&value);
        labelled(
            label,
            row![
                container(
                    text(value.clone())
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
    avatars: &Avatars,
    id: &str,
    name: &'a str,
    avatar: Option<&MxcUri>,
    star: Element<'a, app::Message>,
) -> Element<'a, app::Message> {
    row![
        avatars.view(id, name, avatar, AVATAR, Shape::Circle),
        ellipsis(name)
            .size(18)
            .font(fonts::SEMIBOLD)
            .width(Length::Fill),
        star,
    ]
    .spacing(14)
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

fn close<'a>() -> Element<'a, app::Message> {
    button(text("Close"))
        .style(style::secondary_button)
        .on_press(app::Message::Info(Message::Close))
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
