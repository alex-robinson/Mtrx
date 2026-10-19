//! The member list beside an open room.

use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, scrollable, text, tooltip},
};
use mtrx_core::{Member, OwnedRoomId, OwnedUserId, Role};

use crate::{
    app::Message,
    avatar::{Avatars, Shape},
    dialog::Kind,
    ellipsis::ellipsis,
    fonts, style,
};

const WIDTH: f32 = 250.0;
const AVATAR: f32 = 24.0;

/// The panel's state: the members once loaded, or why loading failed.
#[derive(Default)]
pub struct Members {
    pub list: Option<Result<Vec<Member>, String>>,
}

impl Members {
    pub fn view<'a>(
        &'a self,
        room: &OwnedRoomId,
        room_name: &str,
        own: &OwnedUserId,
        avatars: &Avatars,
    ) -> Element<'a, Message> {
        let mut panel = column![
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
        match &self.list {
            None => panel = panel.push(text("Loading…").size(12).style(text::secondary)),
            Some(Err(e)) => panel = panel.push(text(e).size(12).style(text::danger)),
            Some(Ok(members)) => {
                let (joined, invited): (Vec<_>, Vec<_>) = members.iter().partition(|m| !m.invited);
                panel = panel.push(heading(format!("Joined · {}", joined.len())));
                for member in joined {
                    panel = panel.push(entry(member, own, avatars));
                }
                if !invited.is_empty() {
                    panel = panel.push(heading(format!("Invited · {}", invited.len())));
                    for member in invited {
                        panel = panel.push(entry(member, own, avatars));
                    }
                }
            }
        }
        container(scrollable(panel))
            .width(WIDTH)
            .height(Length::Fill)
            .style(style::panel)
            .into()
    }
}

/// Pictures of the listed members, with their size.
pub fn avatars(members: &Members) -> impl Iterator<Item = (&mtrx_core::MxcUri, f32)> {
    members
        .list
        .iter()
        .flatten()
        .flatten()
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

/// A member; clicking starts a direct chat with them.
fn entry<'a>(member: &'a Member, own: &OwnedUserId, avatars: &Avatars) -> Element<'a, Message> {
    let name = member.name.as_deref().unwrap_or(member.id.localpart());
    let role = match member.role {
        Role::Admin => "Admin",
        Role::Moderator => "Mod",
        Role::Member => "",
    };
    let line = row![
        avatars.view(
            member.id.as_str(),
            name,
            member.avatar.as_deref(),
            AVATAR,
            Shape::Circle
        ),
        ellipsis(name).size(13).width(Length::Fill),
        text(role).size(11).style(text::secondary),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if member.id == *own {
        return container(line).padding([3, 4]).into();
    }
    tooltip(
        button(line)
            .width(Length::Fill)
            .padding([3, 4])
            .style(style::list_item(false))
            .on_press(Message::ChatWith(member.id.clone())),
        container(text(format!("Message {}", member.id)).size(12))
            .padding(6)
            .style(container::rounded_box),
        tooltip::Position::Left,
    )
    .into()
}
