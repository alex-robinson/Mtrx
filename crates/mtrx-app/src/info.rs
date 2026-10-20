//! Info cards: who someone is, or what a room is, with their address to
//! copy. Opened by clicking a name or from the room and member lists.

use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, scrollable, space, text},
};
use mtrx_core::{
    MxcUri, OwnedMxcUri, OwnedRoomId, OwnedUserId, RoomKind, RoomSummary, UserInfo, UserSummary,
};

use crate::{
    app,
    avatar::{Avatars, Shape},
    dialog::Kind,
    ellipsis::ellipsis,
    fonts, style,
};

const AVATAR: f32 = 64.0;
/// Shared rooms listed on a person's card.
const MAX_SHARED: usize = 8;

/// What to show a card for.
#[derive(Debug, Clone)]
pub enum Target {
    /// With the name and picture known so far.
    User(UserSummary),
    Room(OwnedRoomId),
}

#[derive(Debug, Clone)]
pub enum Message {
    UserLoaded(OwnedUserId, Result<UserInfo, String>),
    Copy(String),
    Close,
}

pub struct Card {
    pub target: Target,
    /// Spaces and rooms shared with the person, once loaded.
    shared: Option<Vec<OwnedRoomId>>,
    error: Option<String>,
    /// The text last copied, to confirm it.
    copied: Option<String>,
}

impl Card {
    pub fn new(target: Target) -> Self {
        Self {
            target,
            shared: None,
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

    /// The card's picture, with its size.
    pub fn avatar<'a>(&'a self, rooms: &'a [RoomSummary]) -> Option<(&'a MxcUri, f32)> {
        let mxc = match &self.target {
            Target::User(user) => user.avatar.as_deref(),
            Target::Room(id) => rooms.iter().find(|r| &r.id == id)?.avatar.as_deref(),
        };
        Some((mxc?, AVATAR))
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
            Message::UserLoaded(_, Err(e)) => self.error = Some(e),
            Message::Copy(copied) => self.copied = Some(copied),
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
        container(scrollable(body))
            .padding(20)
            .width(400)
            .max_height(560)
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
        let mut card = column![
            heading(avatars, user.id.as_str(), name, user.avatar.as_deref()),
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
        let mut actions = row![space::horizontal()].spacing(8);
        if user.id != *own_user {
            actions = actions.push(
                button(text("Message"))
                    .style(style::primary_button)
                    .on_press(app::Message::ChatWith(user.id.clone())),
            );
        }
        card.push(actions.push(close())).into()
    }

    fn room_view<'a>(
        &'a self,
        room: &'a RoomSummary,
        rooms: &'a [RoomSummary],
        avatars: &Avatars,
    ) -> Element<'a, app::Message> {
        let mut card = column![heading(
            avatars,
            room.id.as_str(),
            &room.name,
            room.avatar.as_deref()
        )]
        .spacing(14);
        if let Some(topic) = &room.topic {
            card = card.push(text(topic).size(13));
        }
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
        card = card.push(
            text(format!("{kind} · {members} · {encryption}"))
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
                text(format!("In {}", parents.join(", ")))
                    .size(13)
                    .style(text::secondary),
            );
        }
        let mut actions = row![space::horizontal()].spacing(8);
        if !room.invited {
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
        column![
            text(label)
                .size(11)
                .font(fonts::SEMIBOLD)
                .style(text::secondary),
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
        ]
        .spacing(4)
        .into()
    }
}

fn heading<'a>(
    avatars: &Avatars,
    id: &str,
    name: &'a str,
    avatar: Option<&MxcUri>,
) -> Element<'a, app::Message> {
    row![
        avatars.view(id, name, avatar, AVATAR, Shape::Circle),
        ellipsis(name)
            .size(18)
            .font(fonts::SEMIBOLD)
            .width(Length::Fill),
    ]
    .spacing(14)
    .align_y(Alignment::Center)
    .into()
}

/// Spaces, then rooms, shared with someone; each opens.
fn shared_view<'a>(shared: &[OwnedRoomId], rooms: &'a [RoomSummary]) -> Element<'a, app::Message> {
    let mut known: Vec<&RoomSummary> = shared
        .iter()
        .filter_map(|id| rooms.iter().find(|r| &r.id == id))
        .collect();
    known.sort_by_key(|r| r.kind != RoomKind::Space);
    let more = known.len().saturating_sub(MAX_SHARED);
    let mut list = column![
        text("Shared spaces and rooms")
            .size(11)
            .font(fonts::SEMIBOLD)
            .style(text::secondary)
    ]
    .spacing(2);
    for room in known.into_iter().take(MAX_SHARED) {
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
    if more > 0 {
        list = list.push(
            text(format!("and {more} more"))
                .size(12)
                .style(text::secondary),
        );
    }
    list.into()
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
