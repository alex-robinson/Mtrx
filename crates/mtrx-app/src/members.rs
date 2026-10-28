//! The member list beside an open room.

use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, scrollable, text},
};
use mtrx_core::{Member, OwnedRoomId, OwnedUserId, Role};

use crate::{
    app::Message,
    avatar::{Avatars, Shape},
    context_menu,
    dialog::Kind,
    ellipsis::ellipsis,
    fonts,
    hint::{Position, hint},
    popover::{Edge, Side, popover},
    presence::{self, Presences},
    style,
};

const WIDTH: f32 = 250.0;
const AVATAR: f32 = 24.0;

/// What the member rows need besides the members.
#[derive(Clone, Copy)]
pub struct Shown<'a> {
    pub room: &'a OwnedRoomId,
    pub own: &'a OwnedUserId,
    pub avatars: &'a Avatars,
    pub presences: &'a Presences,
    /// The member whose menu is open, in any room.
    pub menu: Option<&'a (OwnedRoomId, OwnedUserId)>,
}

/// The panel: the members once loaded (`list`), or why loading failed.
pub fn view<'a>(
    list: Option<&'a Result<Vec<Member>, String>>,
    room_name: &str,
    shown: Shown<'_>,
) -> Element<'a, Message> {
    let room = shown.room;
    let panel = column![
        row![
            text("Members")
                .size(14)
                .font(fonts::SEMIBOLD)
                .width(Length::Fill),
            button(text("Invite…").size(12))
                .padding([2, 8])
                .style(style::text_button)
                .on_press(Message::OpenDialog(Kind::Invite {
                    room: room.clone(),
                    name: room_name.to_owned(),
                })),
            button(text("×").size(14))
                .padding([0, 6])
                .style(style::text_button)
                .on_press(Message::ToggleMembers),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
    ]
    .spacing(2)
    .padding(8);
    container(scrollable(panel.push(rows(list, shown))))
        .width(WIDTH)
        .height(Length::Fill)
        .style(style::panel)
        .into()
}

/// The members once loaded (`list`), joined then invited, or why loading
/// failed; also in a space's overview.
pub fn rows<'a>(
    list: Option<&'a Result<Vec<Member>, String>>,
    shown: Shown<'_>,
) -> Element<'a, Message> {
    let mut rows = column![].spacing(2);
    match list {
        None => rows = rows.push(text("Loading…").size(12).style(text::secondary)),
        Some(Err(e)) => rows = rows.push(text(e).size(12).style(text::danger)),
        Some(Ok(members)) => {
            let (joined, invited): (Vec<_>, Vec<_>) = members.iter().partition(|m| !m.invited);
            rows = rows.push(heading(format!("Joined · {}", joined.len())));
            for member in joined {
                rows = rows.push(entry(member, shown));
            }
            if !invited.is_empty() {
                rows = rows.push(heading(format!("Invited · {}", invited.len())));
                for member in invited {
                    rows = rows.push(entry(member, shown));
                }
            }
        }
    }
    rows.into()
}

/// Pictures of the listed members, with their size.
pub fn avatars(members: &[Member]) -> impl Iterator<Item = (&mtrx_core::MxcUri, f32)> {
    avatars_of(members)
}

/// Pictures of `members` at the list's size, also for mentions.
pub fn avatars_of<'a>(
    members: impl IntoIterator<Item = &'a Member>,
) -> impl Iterator<Item = (&'a mtrx_core::MxcUri, f32)> {
    members
        .into_iter()
        .filter_map(|m| Some((m.avatar.as_deref()?, AVATAR)))
}

fn heading<'a>(label: String) -> Element<'a, Message> {
    container(
        text(label.to_uppercase())
            .size(11)
            .font(fonts::SEMIBOLD)
            .style(text::secondary),
    )
    .padding([8, 4])
    .into()
}

/// A member, with their presence; clicking shows who they are. A menu
/// manages them, where we may.
fn entry<'a>(member: &'a Member, shown: Shown<'_>) -> Element<'a, Message> {
    let name = member.name.as_deref().unwrap_or(member.id.localpart());
    let role = match member.role {
        Role::Admin => "Admin",
        Role::Moderator => "Mod",
        Role::Member => "",
    };
    let avatar = shown.avatars.view(
        member.id.as_str(),
        name,
        member.avatar.as_deref(),
        AVATAR,
        Shape::Circle,
    );
    let presence = shown.presences.get(&member.id);
    let line = row![
        presence::with_dot(avatar, presence, AVATAR, |theme| {
            theme.extended_palette().background.weak.color
        }),
        ellipsis(name).size(13).width(Length::Fill),
        text(role).size(11).style(text::secondary),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if member.id == *shown.own {
        return container(line).padding([3, 4]).into();
    }
    let card = hint(
        button(line)
            .width(Length::Fill)
            .padding([3, 4])
            .style(style::list_item(false))
            .on_press(Message::ShowInfo(crate::info::sender(
                member.id.clone(),
                member.name.clone(),
                member.avatar.clone(),
            ))),
        member.id.to_string(),
        Position::Left,
    );
    let manage = &member.manage;
    if !manage.remove && !manage.ban && manage.roles.is_empty() {
        return card;
    }
    let key = (shown.room.clone(), member.id.clone());
    let open = shown.menu == Some(&key);
    let more = button(text("⋯").size(14))
        .padding([0, 6])
        .style(style::list_item(open))
        .on_press(Message::MemberMenu((!open).then(|| key.clone())));
    let menu = open.then(|| manage_menu(member, name, shown.room));
    row![
        card,
        popover(more, menu, Side::Below, Edge::Right).on_dismiss(Message::MemberMenu(None)),
    ]
    .spacing(2)
    .align_y(Alignment::Center)
    .into()
}

/// Change a member's role, remove or ban them, as far as we may.
fn manage_menu<'a>(member: &Member, name: &str, room: &OwnedRoomId) -> Element<'a, Message> {
    let mut items = Vec::new();
    for role in &member.manage.roles {
        let label = match role {
            Role::Admin => "Make admin…",
            Role::Moderator => "Make moderator",
            Role::Member => "Make member",
        };
        let message = match role {
            // Not to be undone: they get our power.
            Role::Admin => Message::OpenDialog(Kind::MakeAdmin {
                room: room.clone(),
                user: member.id.clone(),
                name: name.to_owned(),
            }),
            _ => Message::SetRole(room.clone(), member.id.clone(), *role),
        };
        items.push((label.to_owned(), message));
    }
    let remove = |ban| {
        Message::OpenDialog(Kind::RemoveMember {
            room: room.clone(),
            user: member.id.clone(),
            name: name.to_owned(),
            ban,
        })
    };
    if member.manage.remove {
        items.push(("Remove…".to_owned(), remove(false)));
    }
    if member.manage.ban {
        items.push(("Ban…".to_owned(), remove(true)));
    }
    context_menu::menu(items)
}
