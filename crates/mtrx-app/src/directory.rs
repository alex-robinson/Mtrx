//! Rooms and spaces to join, as a space's overview and the join dialog list
//! them: a picture, the name, what it is about, who can join, and Join, Ask
//! to join or Open.

use iced::{
    Alignment, Element, Length, Theme,
    widget::{button, column, container, row, text},
};
use mtrx_core::{DirectoryRoom, JoinAccess, Membership, RoomSummary};

use crate::{
    app,
    avatar::{Avatars, Shape},
    ellipsis::ellipsis,
    fonts,
    hint::{Position, hint},
    icons, style,
};

pub const AVATAR: f32 = 28.0;

/// Our membership of a listed room: as the room list has it (it follows
/// joins from anywhere), else asked to join (`asked` here, or as the server
/// listed it).
pub fn membership(room: &DirectoryRoom, rooms: &[RoomSummary], asked: bool) -> Option<Membership> {
    match rooms.iter().find(|r| r.id == room.id) {
        Some(r) if r.invited => Some(Membership::Invited),
        Some(_) => Some(Membership::Joined),
        None if asked => Some(Membership::Asked),
        None => room.membership.filter(|m| *m == Membership::Asked),
    }
}

/// One room or space, after `lead` (an indent, say); `busy` while joining
/// or asking to.
pub fn entry<'a>(
    room: &'a DirectoryRoom,
    membership: Option<Membership>,
    busy: bool,
    lead: Element<'a, app::Message>,
    avatars: &Avatars,
    on_join: app::Message,
    on_knock: app::Message,
) -> Element<'a, app::Message> {
    let picture = avatars.view(
        room.id.as_str(),
        &room.name,
        room.avatar.as_deref(),
        AVATAR,
        if room.is_space {
            Shape::RoundedSquare
        } else {
            Shape::Circle
        },
    );
    let mut name = row![ellipsis(&room.name).size(14)]
        .spacing(6)
        .align_y(Alignment::Center);
    if room.suggested {
        name = name.push(text("suggested").size(11).style(text::secondary));
    }
    let details = column![
        name,
        ellipsis(details(room)).size(12).style(text::secondary)
    ]
    .spacing(1)
    .width(Length::Fill);
    let (access, access_hint) = access_icon(room.access);
    row![
        lead,
        picture,
        details,
        hint(access, access_hint, Position::Left),
        action(room, membership, busy, on_join, on_knock),
    ]
    .spacing(8)
    .padding([3, 4])
    .align_y(Alignment::Center)
    .into()
}

/// "Space · 4 rooms · the topic's first line · 12 members".
fn details(room: &DirectoryRoom) -> String {
    let mut parts = Vec::new();
    if room.is_space {
        parts.push(match room.children {
            0 => "Space".to_owned(),
            1 => "Space · 1 room".to_owned(),
            n => format!("Space · {n} rooms"),
        });
    }
    if let Some(line) =
        (room.topic.as_deref()).and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty()))
    {
        parts.push(line.to_owned());
    }
    parts.push(match room.members {
        1 => "1 member".to_owned(),
        n => format!("{n} members"),
    });
    parts.join(" · ")
}

/// Join, Ask to join or Open, as the room allows.
fn action<'a>(
    room: &DirectoryRoom,
    membership: Option<Membership>,
    busy: bool,
    on_join: app::Message,
    on_knock: app::Message,
) -> Element<'a, app::Message> {
    let quiet = |label: &'a str| -> Element<'a, app::Message> {
        container(text(label).size(12).style(text::secondary))
            .padding([4, 10])
            .into()
    };
    if busy {
        return quiet(if room.access == JoinAccess::Knock {
            "Asking…"
        } else {
            "Joining…"
        });
    }
    let act = |label: &'a str, on_press: app::Message| -> Element<'a, app::Message> {
        button(text(label).size(12))
            .padding([4, 10])
            .style(style::secondary_button)
            .on_press(on_press)
            .into()
    };
    let open = if room.is_space {
        app::Message::SelectSpace(Some(room.id.clone()))
    } else {
        app::Message::OpenRoom(room.id.clone())
    };
    match membership {
        Some(Membership::Joined) => act("Open", open),
        // Shows the invite, to accept or decline.
        Some(Membership::Invited) => act("Invited", open),
        Some(Membership::Asked) => quiet("Asked"),
        None => match room.access {
            JoinAccess::Public | JoinAccess::SpaceMembers => act("Join", on_join),
            JoinAccess::Knock => act("Ask to join", on_knock),
            JoinAccess::Invite => quiet("Invite only"),
        },
    }
}

/// Who can join, as an icon and its hint.
fn access_icon<'a>(access: JoinAccess) -> (Element<'a, app::Message>, &'static str) {
    const SIZE: f32 = 15.0;
    match access {
        JoinAccess::Public => (icons::globe(SIZE, secondary), "Anyone can join"),
        JoinAccess::SpaceMembers => (icons::people(SIZE, secondary), "Space members can join"),
        JoinAccess::Knock => (icons::door(SIZE, secondary), "Ask to join"),
        JoinAccess::Invite => (icons::lock(SIZE, secondary), "Invited people only"),
    }
}

/// A row of tabs, `chosen` marked.
pub fn tabs<'a, T: Copy + PartialEq + 'a>(
    tabs: &[(&'a str, T)],
    chosen: T,
    on_press: impl Fn(T) -> app::Message,
) -> Element<'a, app::Message> {
    row(tabs.iter().map(|&(label, tab)| {
        let is = tab == chosen;
        button(
            text(label)
                .size(14)
                .font(if is { fonts::SEMIBOLD } else { fonts::INTER })
                .style(if is { text::default } else { text::secondary }),
        )
        .padding([4, 10])
        .style(style::list_item(is))
        .on_press(on_press(tab))
        .into()
    }))
    .spacing(4)
    .into()
}

/// The colour of quiet icons.
pub fn secondary(theme: &Theme) -> iced::Color {
    theme
        .extended_palette()
        .background
        .base
        .text
        .scale_alpha(0.6)
}
