//! Space bar and room list.

use std::collections::{HashMap, HashSet};

use iced::{
    Alignment, Color, Element, Length, Padding, Theme,
    widget::{button, column, container, row, scrollable, space, stack, text, tooltip},
};
use mtrx_core::{MxcUri, OwnedRoomId, RoomKind, RoomSummary, ScheduledMessage};

use crate::{
    app::Message,
    avatar::{Avatars, Shape},
    ellipsis::ellipsis,
    fonts, icons,
    style::{self, ROOM_AVATAR, ROOM_LIST_WIDTH, ROOM_ROW, SPACE_AVATAR, SPACE_BAR_WIDTH},
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

/// The room list's sections and their rooms, in display order. Sections may
/// be empty.
fn sections<'a>(
    rooms: &'a [RoomSummary],
    selected_space: Option<&OwnedRoomId>,
) -> [(&'static str, Vec<&'a RoomSummary>); 3] {
    let in_space = selected_space.map(|s| descendants(rooms, s));
    let listed = |r: &&RoomSummary| match &in_space {
        Some(ids) => ids.contains(&r.id),
        None => true,
    };
    let joined = |kind| move |r: &&RoomSummary| !r.invited && r.kind == kind;
    [
        (
            "Invites",
            rooms.iter().filter(|r| r.invited).filter(listed).collect(),
        ),
        (
            "Rooms",
            rooms
                .iter()
                .filter(joined(RoomKind::Room))
                .filter(listed)
                .collect(),
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
    ]
}

/// Pictures in the space bar and room list, with their sizes.
pub fn avatars<'a>(
    rooms: &'a [RoomSummary],
    selected_space: Option<&OwnedRoomId>,
) -> Vec<(&'a MxcUri, f32)> {
    let spaces = top_level_spaces(rooms)
        .into_iter()
        .filter_map(|s| Some((s.avatar.as_deref()?, SPACE_AVATAR)));
    let listed = sections(rooms, selected_space)
        .into_iter()
        .flat_map(|(_, entries)| entries)
        .filter_map(|r| Some((r.avatar.as_deref()?, ROOM_AVATAR)));
    spaces.chain(listed).collect()
}

pub fn space_bar<'a>(
    rooms: &[RoomSummary],
    selected: Option<&OwnedRoomId>,
    avatars: &Avatars,
) -> Element<'a, Message> {
    let entry = |icon: Element<'a, Message>, title: String, target: Option<OwnedRoomId>, unread| {
        let is_selected = selected == target.as_ref();
        let mut icon = stack![icon];
        if unread {
            let dot = container(space())
                .width(10)
                .height(10)
                .style(style::unread_dot);
            icon = icon.push(container(dot).align_right(SPACE_AVATAR));
        }
        let entry_button = button(icon)
            .padding(4)
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
        home_icon(),
        "Home".into(),
        None,
        any_unread(&|_| true)
    )];
    for space in top_level_spaces(rooms) {
        let inside = descendants(rooms, &space.id);
        let icon = avatars.view(
            space.id.as_str(),
            &space.name,
            space.avatar.as_deref(),
            SPACE_AVATAR,
            Shape::RoundedSquare,
        );
        let unread = any_unread(&|r| inside.contains(&r.id));
        bar = bar.push(entry(
            icon,
            space.name.clone(),
            Some(space.id.clone()),
            unread,
        ));
    }
    let settings = button(icons::gear(20.0, |theme| {
        theme
            .extended_palette()
            .background
            .base
            .text
            .scale_alpha(0.7)
    }))
    .padding(8)
    .style(style::list_item(false))
    .on_press(Message::ShowSettings(true));
    container(column![
        scrollable(bar.spacing(4).width(Length::Fill).padding([8, 0])).height(Length::Fill),
        container(with_tooltip(settings, "Settings".into()))
            .center_x(Length::Fill)
            .padding([8, 0]),
    ])
    .width(SPACE_BAR_WIDTH)
    .height(Length::Fill)
    .style(style::space_bar)
    .into()
}

/// Home in the space bar: a tile like a space avatar.
fn home_icon<'a>() -> Element<'a, Message> {
    container(text("⌂").size(20).font(fonts::SEMIBOLD))
        .center(SPACE_AVATAR)
        .style(style::home_tile)
        .into()
}

pub fn room_list<'a>(
    rooms: &'a [RoomSummary],
    selected_space: Option<&OwnedRoomId>,
    open: Option<&OwnedRoomId>,
    loaded: bool,
    avatars: &Avatars,
    scheduled: &[ScheduledMessage],
) -> Element<'a, Message> {
    let title = match selected_space {
        Some(id) => rooms
            .iter()
            .find(|r| &r.id == id)
            .map(|r| r.name.as_str())
            .unwrap_or("Space"),
        None => "Home",
    };

    let mut list =
        column![container(ellipsis(title).size(16).font(fonts::SEMIBOLD)).padding([4, 8])]
            .padding(8);
    for (heading, entries) in sections(rooms, selected_space) {
        if entries.is_empty() {
            continue;
        }
        list = list.push(section_heading(heading));
        for room in entries {
            let later = Later::of(scheduled, &room.id);
            list = list.push(room_entry(room, open == Some(&room.id), later, avatars));
        }
    }
    if !loaded {
        list =
            list.push(container(text("Syncing…").size(12).style(text::secondary)).padding([8, 8]));
    }
    container(scrollable(list))
        .width(ROOM_LIST_WIDTH)
        .height(Length::Fill)
        .style(style::panel)
        .into()
}

/// Small upper-case label above a group of rooms.
fn section_heading<'a>(heading: &str) -> Element<'a, Message> {
    container(
        text(heading.to_uppercase())
            .size(11)
            .font(fonts::SEMIBOLD)
            .style(text::secondary),
    )
    .padding(Padding::new(8.0).top(14).bottom(4))
    .into()
}

/// A room's scheduled messages, as shown in the room list.
struct Later {
    count: usize,
    overdue: usize,
}

impl Later {
    fn of(scheduled: &[ScheduledMessage], room: &OwnedRoomId) -> Option<Self> {
        let mut later = Self {
            count: 0,
            overdue: 0,
        };
        for message in scheduled.iter().filter(|m| &m.room_id == room) {
            later.count += 1;
            later.overdue += usize::from(message.overdue);
        }
        (later.count > 0).then_some(later)
    }

    /// A clock; warning-coloured if any message is overdue.
    fn view<'a>(&self) -> Element<'a, Message> {
        let (color, tip): (fn(&Theme) -> Color, _) = if self.overdue > 0 {
            (
                |theme| theme.extended_palette().warning.strong.color,
                format!("{} overdue scheduled message(s)", self.overdue),
            )
        } else {
            (
                |theme| {
                    theme
                        .extended_palette()
                        .background
                        .strong
                        .text
                        .scale_alpha(0.6)
                },
                format!("{} scheduled message(s)", self.count),
            )
        };
        with_tooltip(icons::clock(14.0, color), tip)
    }
}

fn room_entry<'a>(
    room: &'a RoomSummary,
    selected: bool,
    later: Option<Later>,
    avatars: &Avatars,
) -> Element<'a, Message> {
    let font = if room.unread_messages > 0 {
        fonts::SEMIBOLD
    } else {
        fonts::INTER
    };
    let avatar = avatars.view(
        room.id.as_str(),
        &room.name,
        room.avatar.as_deref(),
        ROOM_AVATAR,
        Shape::Circle,
    );
    let name = ellipsis(&room.name).size(14).font(font).width(Length::Fill);
    let mut line = row![avatar, name].spacing(8).align_y(Alignment::Center);
    if let Some(later) = later {
        line = line.push(later.view());
    }
    if room.unread_mentions > 0 || room.unread_messages > 0 {
        let highlight = room.unread_mentions > 0;
        let count = if highlight {
            room.unread_mentions
        } else {
            room.unread_messages
        };
        line = line.push(
            container(text(count.to_string()).size(11).font(fonts::SEMIBOLD))
                .padding([1, 6])
                .style(move |t| style::badge(t, highlight)),
        );
    }
    button(container(line).center_y(Length::Fill))
        .width(Length::Fill)
        .height(ROOM_ROW)
        .padding([0, 8])
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
