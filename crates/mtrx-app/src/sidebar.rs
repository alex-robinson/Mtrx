//! Space bar and room list.

use std::collections::{HashMap, HashSet};

use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, scrollable, space, text, tooltip},
};
use mtrx_core::{OwnedRoomId, RoomKind, RoomSummary};

use crate::{
    app::Message,
    fonts,
    style::{self, ROOM_LIST_WIDTH, SPACE_BAR_WIDTH},
};

/// Joined spaces without a joined parent space.
pub fn top_level_spaces(rooms: &[RoomSummary]) -> Vec<&RoomSummary> {
    let ids: HashSet<&OwnedRoomId> = rooms.iter().map(|r| &r.id).collect();
    rooms
        .iter()
        .filter(|r| r.kind == RoomKind::Space && !r.invited)
        .filter(|r| !r.parents.iter().any(|p| ids.contains(p)))
        .collect()
}

/// Rooms below `space`, through nested spaces too.
fn descendants(rooms: &[RoomSummary], space: &OwnedRoomId) -> HashSet<OwnedRoomId> {
    let mut children: HashMap<&OwnedRoomId, Vec<&OwnedRoomId>> = HashMap::new();
    for room in rooms {
        for parent in &room.parents {
            children.entry(parent).or_default().push(&room.id);
        }
    }
    let mut found = HashSet::new();
    let mut stack = vec![space];
    while let Some(id) = stack.pop() {
        for child in children.get(id).into_iter().flatten() {
            if found.insert((*child).clone()) {
                stack.push(child);
            }
        }
    }
    found
}

pub fn space_bar<'a>(
    rooms: &[RoomSummary],
    selected: Option<&OwnedRoomId>,
) -> Element<'a, Message> {
    let entry = |label: String, title: String, target: Option<OwnedRoomId>, unread: bool| {
        let is_selected = selected == target.as_ref();
        let marker = if unread { "•" } else { " " };
        let entry_button = button(
            column![
                text(label).size(18).font(fonts::SEMIBOLD),
                text(marker).size(10)
            ]
            .align_x(Alignment::Center)
            .width(Length::Fill),
        )
        .width(48)
        .padding([6, 0])
        .style(style::list_item(is_selected))
        .on_press(Message::SelectSpace(target));
        container(with_tooltip(entry_button, title)).center_x(Length::Fill)
    };

    let any_unread = |ids: &dyn Fn(&RoomSummary) -> bool| {
        rooms
            .iter()
            .any(|r| ids(r) && r.kind != RoomKind::Space && r.unread_messages > 0)
    };
    let mut bar = column![entry(
        "⌂".into(),
        "Home".into(),
        None,
        any_unread(&|_| true)
    )];
    for space in top_level_spaces(rooms) {
        let inside = descendants(rooms, &space.id);
        let initial = space
            .name
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .collect::<String>();
        let unread = any_unread(&|r| inside.contains(&r.id));
        bar = bar.push(entry(
            initial,
            space.name.clone(),
            Some(space.id.clone()),
            unread,
        ));
    }
    container(scrollable(
        bar.spacing(6).width(Length::Fill).padding([10, 0]),
    ))
    .width(SPACE_BAR_WIDTH)
    .height(Length::Fill)
    .style(style::space_bar)
    .into()
}

pub fn room_list<'a>(
    rooms: &'a [RoomSummary],
    selected_space: Option<&OwnedRoomId>,
    open: Option<&OwnedRoomId>,
    loaded: bool,
) -> Element<'a, Message> {
    let in_space = selected_space.map(|s| descendants(rooms, s));
    let visible = |r: &RoomSummary| match &in_space {
        Some(ids) => ids.contains(&r.id),
        None => true,
    };
    let title = match selected_space {
        Some(id) => rooms
            .iter()
            .find(|r| &r.id == id)
            .map(|r| r.name.as_str())
            .unwrap_or("Space"),
        None => "Home",
    };

    let mut list = column![text(title).size(18).font(fonts::SEMIBOLD)]
        .spacing(2)
        .padding(12);
    let joined = |kind| move |r: &&RoomSummary| !r.invited && r.kind == kind;
    let sections: [(&str, Vec<&RoomSummary>); 3] = [
        ("Invites", rooms.iter().filter(|r| r.invited).collect()),
        (
            "Rooms",
            rooms.iter().filter(joined(RoomKind::Room)).collect(),
        ),
        // People get grouped per space later; for now DMs only show at Home.
        (
            "People",
            rooms
                .iter()
                .filter(joined(RoomKind::Direct))
                .filter(|_| selected_space.is_none())
                .collect(),
        ),
    ];
    for (heading, entries) in sections {
        let entries: Vec<_> = entries.into_iter().filter(|r| visible(r)).collect();
        if entries.is_empty() {
            continue;
        }
        list = list
            .push(space().height(8))
            .push(text(heading).size(12).style(text::secondary));
        for room in entries {
            list = list.push(room_entry(room, open == Some(&room.id)));
        }
    }
    if !loaded {
        list = list.push(text("Syncing…").size(12).style(text::secondary));
    }
    container(scrollable(list))
        .width(ROOM_LIST_WIDTH)
        .height(Length::Fill)
        .style(style::panel)
        .into()
}

fn room_entry(room: &RoomSummary, selected: bool) -> Element<'_, Message> {
    let font = if room.unread_messages > 0 {
        fonts::SEMIBOLD
    } else {
        fonts::INTER
    };
    let mut line = row![text(&room.name).font(font).width(Length::Fill)].align_y(Alignment::Center);
    if room.unread_mentions > 0 || room.unread_messages > 0 {
        let highlight = room.unread_mentions > 0;
        let count = if highlight {
            room.unread_mentions
        } else {
            room.unread_messages
        };
        line = line.push(
            container(text(count.to_string()).size(11))
                .padding([1, 6])
                .style(move |t| style::badge(t, highlight)),
        );
    }
    button(line)
        .width(Length::Fill)
        .padding([6, 8])
        .style(style::list_item(selected))
        .on_press(Message::OpenRoom(room.id.clone()))
        .into()
}

fn with_tooltip<'a>(content: impl Into<Element<'a, Message>>, tip: String) -> Element<'a, Message> {
    tooltip(
        content,
        container(text(tip).size(12))
            .padding(6)
            .style(container::rounded_box),
        tooltip::Position::Right,
    )
    .into()
}
