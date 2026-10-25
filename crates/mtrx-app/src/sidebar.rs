//! Space bar and room list.

use iced::{
    Alignment, Color, Element, Length, Padding, Rectangle, Task, Theme, Vector,
    advanced::widget::{Id, Operation, operation},
    keyboard::{
        Key, Modifiers,
        key::{self, Named},
    },
    widget::{
        button, column, container, mouse_area, row, scrollable, space, stack, text, text_input,
    },
};
use mtrx_core::{
    Grouping, Layout, MxcUri, OwnedRoomId, OwnedUserId, RoomKind, RoomSummary, RoomTag,
    ScheduledMessage, Section, Sort, UserId, top_level_spaces,
};

use crate::{
    app::Message,
    arrange::{self, Arrange, NAME_INPUT},
    avatar::{Avatars, Shape},
    dialog::Kind,
    ellipsis::{ellipsis, followed_by},
    fonts,
    hint::{Position, hint, shortcut},
    icons, info,
    presence::{self, Presences},
    style::{self, ROOM_AVATAR, ROOM_ROW, SPACE_AVATAR, SPACE_BAR_WIDTH},
    tint::Tint,
};

/// The room search field, focused by ⌘K / Ctrl+K.
pub const SEARCH: &str = "room-search";

/// Spaces containing `room`, nearest first (through nested spaces too).
pub fn ancestors(rooms: &[RoomSummary], room: &OwnedRoomId) -> Vec<OwnedRoomId> {
    let parents = |id: &OwnedRoomId| {
        rooms
            .iter()
            .find(|r| &r.id == id)
            .map(|r| r.parents.clone())
            .unwrap_or_default()
    };
    let mut found: Vec<OwnedRoomId> = Vec::new();
    let mut next = parents(room);
    while !next.is_empty() {
        let mut further = Vec::new();
        for space in next {
            if !found.contains(&space) {
                further.extend(parents(&space));
                found.push(space);
            }
        }
        next = further;
    }
    found
}

/// Whether anything `space` lists (Home for `None`) is unread: its dot in
/// the space bar.
fn unread(
    rooms: &[RoomSummary],
    space: Option<&OwnedRoomId>,
    arrange: &Arrange,
    grouping: &Grouping,
) -> bool {
    arrange
        .listed(rooms, space, grouping)
        .iter()
        .any(|r| r.unread_messages > 0)
}

/// The room list's sections and their rooms, in display order; sections
/// may be empty. Notes pinned at the top are not in them.
fn sections<'a>(view: &RoomListView<'a>) -> Vec<(Section, Vec<&'a RoomSummary>)> {
    let space = view.selected_space;
    let pinned = match view.notes {
        Some(Notes::Room(notes)) => Some(&notes.id),
        _ => None,
    };
    let listed = view
        .arrange
        .listed(view.rooms, space, &view.grouping)
        .into_iter()
        .filter(|r| Some(&r.id) != pinned);
    view.arrange
        .categories
        .layout(space.map(|s| s.as_ref()))
        .group(listed, &view.grouping)
}

/// What the search finds in what `space` (or Home) lists: rooms and chats
/// whose name contains `filter` (lower case), names starting with it first.
pub fn search<'a>(
    rooms: &'a [RoomSummary],
    space: Option<&OwnedRoomId>,
    arrange: &Arrange,
    grouping: &Grouping,
    filter: &str,
) -> Vec<&'a RoomSummary> {
    let mut found: Vec<_> = arrange
        .listed(rooms, space, grouping)
        .into_iter()
        .filter(|r| r.kind != RoomKind::Space)
        .filter_map(|r| {
            let name = r.name.to_lowercase();
            name.find(filter).map(|at| (at > 0, r))
        })
        .collect();
    found.sort_by_key(|(later, r)| (*later, r.name.to_lowercase()));
    found.into_iter().map(|(_, r)| r).collect()
}

/// The room the search field's Enter opens.
pub fn first_match<'a>(
    rooms: &'a [RoomSummary],
    space: Option<&OwnedRoomId>,
    arrange: &Arrange,
    grouping: &Grouping,
    filter: &str,
) -> Option<&'a RoomSummary> {
    search(
        rooms,
        space,
        arrange,
        grouping,
        &filter.trim().to_lowercase(),
    )
    .into_iter()
    .next()
}

/// The rooms the room list shows (in collapsed sections too), or those
/// found by the search; not the pinned Notes.
fn listed<'a>(view: &RoomListView<'a>) -> Vec<&'a RoomSummary> {
    let filter = view.filter.trim().to_lowercase();
    if filter.is_empty() {
        sections(view)
            .into_iter()
            .flat_map(|(_, entries)| entries)
            .collect()
    } else {
        let (space, arrange) = (view.selected_space, view.arrange);
        search(view.rooms, space, arrange, &view.grouping, &filter)
    }
}

/// The rooms with a row in the room list, in its order: the pinned Notes,
/// then each section's, or what the search found.
fn rows<'a>(view: &RoomListView<'a>) -> Vec<&'a RoomSummary> {
    if !view.filter.trim().is_empty() {
        return listed(view);
    }
    let layout = (view.arrange.categories).layout(view.selected_space.map(|s| s.as_ref()));
    let notes = match view.notes {
        Some(Notes::Room(notes)) => Some(notes),
        _ => None,
    };
    let sections = sections(view).into_iter().flat_map(|(section, entries)| {
        let collapsed = layout.is_collapsed(&section);
        entries
            .into_iter()
            .filter(move |room| view.shows(room, collapsed))
    });
    notes.into_iter().chain(sections).collect()
}

/// Going to another room from the keyboard, in the room list's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Go {
    /// ⌥⌘↑, ⌃⇧Tab.
    Above,
    /// ⌥⌘↓, ⌃Tab.
    Below,
    /// ⌘1 to ⌘9: the first to ninth row, counted from 0.
    Row(usize),
}

impl Go {
    /// Where the key goes. Also while a field has the focus: fields leave
    /// these keys (see the composer's key binding).
    pub fn from_key(key: &Key, physical_key: key::Physical, modifiers: Modifiers) -> Option<Self> {
        let up_or_down = |up| if up { Go::Above } else { Go::Below };
        match key.as_ref() {
            Key::Named(arrow @ (Named::ArrowUp | Named::ArrowDown))
                if modifiers.command() && modifiers.alt() && !modifiers.shift() =>
            {
                Some(up_or_down(arrow == Named::ArrowUp))
            }
            Key::Named(Named::Tab) if modifiers.control() => Some(up_or_down(modifiers.shift())),
            // By the key's place, also where digits need Shift (AZERTY).
            Key::Character(c) if modifiers.command() && !modifiers.alt() => {
                use key::Code::*;
                let row = match physical_key {
                    key::Physical::Code(code) => [
                        Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
                    ]
                    .iter()
                    .position(|digit| *digit == code),
                    key::Physical::Unidentified(_) => None,
                };
                let row = row.or_else(|| c.parse::<usize>().ok()?.checked_sub(1))?;
                (row < 9).then_some(Go::Row(row))
            }
            _ => None,
        }
    }
}

/// The room `go` leads to from the open one (from outside the list: the
/// first or last); none past the ends.
pub fn go_to<'a>(view: &RoomListView<'a>, go: Go) -> Option<&'a RoomSummary> {
    let rows = rows(view);
    let at = (view.open).and_then(|open| rows.iter().position(|room| &room.id == open));
    let index = match (go, at) {
        (Go::Row(row), _) => row,
        (Go::Above, Some(at)) => at.checked_sub(1)?,
        (Go::Below, Some(at)) => at + 1,
        (Go::Above, None) => rows.len().checked_sub(1)?,
        (Go::Below, None) => 0,
    };
    rows.get(index).copied()
}

/// Pictures in the space bar and room list, with their sizes.
pub fn avatars<'a>(view: &RoomListView<'a>) -> Vec<(&'a MxcUri, f32)> {
    let spaces = top_level_spaces(view.rooms)
        .into_iter()
        .filter_map(|s| Some((s.avatar.as_deref()?, SPACE_AVATAR)));
    let listed = listed(view)
        .into_iter()
        .filter(|r| view.room_pictures || r.kind != RoomKind::Room)
        .filter_map(|r| Some((r.avatar.as_deref()?, ROOM_AVATAR)));
    spaces.chain(listed).collect()
}

/// The people of the direct chats the room list shows, in its order,
/// without us (the Notes to self).
pub fn direct_users<'a>(view: &RoomListView<'a>, own: &UserId) -> Vec<&'a OwnedUserId> {
    listed(view)
        .into_iter()
        .filter(|r| r.kind == RoomKind::Direct)
        .filter_map(|r| r.direct_user.as_ref())
        .filter(|user| *user != own)
        .collect()
}

pub fn space_bar<'a>(
    rooms: &[RoomSummary],
    selected: Option<&OwnedRoomId>,
    arrange: &Arrange,
    grouping: &Grouping,
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

    let mut bar = column![entry(
        home_icon(),
        "Home".into(),
        None,
        unread(rooms, None, arrange, grouping)
    )];
    for space in top_level_spaces(rooms) {
        let icon = avatars.view(
            space.id.as_str(),
            &space.name,
            space.avatar.as_deref(),
            SPACE_AVATAR,
            Shape::RoundedSquare,
        );
        bar = bar.push(entry(
            icon,
            space.name.clone(),
            Some(space.id.clone()),
            unread(rooms, Some(&space.id), arrange, grouping),
        ));
    }
    let new_space = button(
        container(text("+").size(22).font(fonts::SEMIBOLD))
            .center(SPACE_AVATAR)
            .style(style::new_space_tile),
    )
    .padding(4)
    .style(style::list_item(false))
    .on_press(Message::OpenDialog(Kind::NewSpace));
    bar = bar.push(container(with_tooltip(new_space, "New space".into())).center_x(Length::Fill));
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

/// What the room list shows.
pub struct RoomListView<'a> {
    pub rooms: &'a [RoomSummary],
    pub selected_space: Option<&'a OwnedRoomId>,
    /// The room shown on the right.
    pub open: Option<&'a OwnedRoomId>,
    /// The first sync finished.
    pub loaded: bool,
    pub avatars: &'a Avatars,
    pub scheduled: &'a [ScheduledMessage],
    /// The "+" menu is open.
    pub add_menu: bool,
    /// The search text.
    pub filter: &'a str,
    pub arrange: &'a Arrange,
    pub grouping: Grouping,
    pub out_of_view: OutOfView,
    pub width: f32,
    /// The list's colour here.
    pub tint: Tint,
    /// Rooms show their pictures rather than a public/private symbol.
    pub room_pictures: bool,
    /// The Notes to self, pinned at the top; `None` if not pinned.
    pub notes: Option<Notes<'a>>,
    /// Dots on the pictures of direct chats.
    pub presences: &'a Presences,
}

/// The Notes to self at the top of the room list.
#[derive(Clone, Copy)]
pub enum Notes<'a> {
    /// Not started yet: the row starts them.
    Missing,
    Room(&'a RoomSummary),
}

pub fn room_list<'a>(view: RoomListView<'a>) -> Element<'a, Message> {
    let RoomListView {
        rooms,
        selected_space,
        add_menu,
        filter,
        ..
    } = view;
    let title = match selected_space {
        Some(id) => rooms
            .iter()
            .find(|r| &r.id == id)
            .map(|r| r.name.as_str())
            .unwrap_or("Space"),
        None => "Home",
    };

    let add = button(text("+").size(18).font(fonts::SEMIBOLD))
        .padding([0, 8])
        .style(style::list_item(add_menu))
        .on_press(Message::AddMenu(!add_menu));
    // A space's name opens its settings, with its address and link.
    let heading: Element<'a, Message> = match selected_space {
        Some(space) => hint(
            button(ellipsis(title).size(16).font(fonts::SEMIBOLD))
                .padding([4, 8])
                .style(style::list_item(false))
                .on_press(Message::ShowInfo(info::Target::Room(space.clone()))),
            "Space settings, address and link",
            Position::Bottom,
        ),
        None => container(ellipsis(title).size(16).font(fonts::SEMIBOLD))
            .padding([4, 8])
            .into(),
    };
    let mut list = column![
        row![
            container(heading).width(Length::Fill),
            with_tooltip(add, "New room, chat or category; join a room".into()),
        ]
        .align_y(Alignment::Center)
    ]
    .padding(8);
    if add_menu {
        let space = selected_space.map(|id| (id.clone(), title.to_owned()));
        list = list.push(add_menu_view(space));
    }
    list = list.push(
        container(
            text_input(
                &format!("Search rooms and chats  ({})", shortcut("K")),
                filter,
            )
            .id(SEARCH)
            .on_input(Message::RoomFilter)
            .on_submit(Message::OpenFirstMatch)
            .style(style::input)
            .padding([5, 8])
            .size(13),
        )
        .padding([4, 0]),
    );
    let filter = filter.trim().to_lowercase();
    if filter.is_empty() {
        if let Some(notes) = view.notes {
            list = list.push(view.notes_entry(notes));
        }
        list = list.extend(arranged(&view));
    } else {
        let found = search(rooms, selected_space, view.arrange, &view.grouping, &filter);
        if found.is_empty() {
            let none = if selected_space.is_some() {
                "No rooms or chats in this space match."
            } else {
                "No rooms or chats at Home match."
            };
            list = list.push(container(text(none).size(12).style(text::secondary)).padding([8, 8]));
        } else {
            list = list.push(
                container(section_title("Results")).padding(Padding::new(8.0).top(14).bottom(4)),
            );
        }
        for room in found {
            list = list.push(view.room_entry(room));
        }
    }
    if !view.loaded {
        list =
            list.push(container(text("Syncing…").size(12).style(text::secondary)).padding([8, 8]));
    }
    let list = scrollable(list)
        .id(ROOM_LIST)
        .on_scroll(|_| Message::RoomListScrolled)
        .height(Length::Fill);
    container(out_of_view_arrows(list.into(), view.out_of_view))
        .width(view.width)
        .height(Length::Fill)
        .style(style::room_list(view.tint))
        .into()
}

/// The sections of the room list, with menus, the category being renamed,
/// and drop targets while dragging.
fn arranged<'a>(view: &RoomListView<'a>) -> Vec<Element<'a, Message>> {
    let arrange = view.arrange;
    let layout = arrange
        .categories
        .layout(view.selected_space.map(|s| s.as_ref()));
    let dragging = arrange.dragging().is_some();
    let space = view.selected_space;
    let mut elements = Vec::new();
    for (section, entries) in sections(view) {
        let shown = match section {
            Section::Invites => !entries.is_empty(),
            Section::Category(_) => true,
            _ => dragging || !entries.is_empty(),
        };
        if !shown {
            continue;
        }
        let collapsed = layout.is_collapsed(&section);
        let renaming = arrange
            .naming
            .as_ref()
            .filter(|n| matches!(&section, Section::Category(id) if n.category == *id));
        let mut block = column![match renaming {
            Some(naming) => name_input(&naming.name),
            None => section_header(&section, collapsed, space, arrange),
        }];
        if let Section::Category(id) = &section
            && arrange.category_menu.as_ref() == Some(id)
        {
            block = block.push(category_menu(layout, space, id));
        }
        let empty = entries.is_empty();
        for room in entries {
            if !view.shows(room, collapsed) {
                continue;
            }
            block = block.push(view.room_entry(room));
            if arrange.room_menu.as_ref() == Some(&room.id) {
                block = block.push(room_menu(room, space, layout, &view.grouping));
            }
        }
        if empty && !collapsed && matches!(section, Section::Category(_)) {
            block = block.push(
                container(text("Drag rooms here").size(12).style(text::secondary)).padding([4, 8]),
            );
        }
        elements.push(if dragging && section != Section::Invites {
            let target = arrange.target.as_ref() == Some(&section);
            mouse_area(
                container(block)
                    .width(Length::Fill)
                    .style(move |theme| style::drop_target(theme, target)),
            )
            .on_enter(arrange_message(arrange::Message::Target(
                section.clone(),
                true,
            )))
            .on_exit(arrange_message(arrange::Message::Target(section, false)))
            .into()
        } else {
            block.into()
        });
    }
    elements
}

fn arrange_message(message: arrange::Message) -> Message {
    Message::Arrange(message)
}

/// A section's name in the room list of `space` (Home for `None`). Home's
/// rooms not in a category are those in no space, unless it lists all.
fn section_name<'a>(
    arrange: &'a Arrange,
    space: Option<&OwnedRoomId>,
    section: &Section,
) -> &'a str {
    let categories = &arrange.categories;
    match section {
        Section::Invites => "Invites",
        Section::Favourites => "Favourites",
        Section::Category(id) => (categories.layout(space.map(|s| s.as_ref())).category(id))
            .map_or("", |c| c.name.as_str()),
        Section::Rooms if space.is_none() && !categories.at_home.all_rooms => "Rooms with no space",
        Section::Rooms => "Rooms",
        Section::People => "Direct messages",
        Section::LowPriority => "Low priority",
    }
}

/// A section's name, which collapses and expands it; a category's also
/// has its menu.
fn section_header<'a>(
    section: &Section,
    collapsed: bool,
    space: Option<&OwnedRoomId>,
    arrange: &Arrange,
) -> Element<'a, Message> {
    let chevron = if collapsed { "▸" } else { "▾" };
    let title = button(
        row![
            text(chevron).size(10).style(text::secondary),
            section_title(section_name(arrange, space, section)),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding(0)
    .width(Length::Fill)
    .style(style::text_button)
    .on_press(arrange_message(arrange::Message::ToggleCollapsed(
        section.clone(),
    )));
    let mut header = row![title].align_y(Alignment::Center);
    if let Section::Category(id) = section {
        let open = arrange.category_menu.as_ref() == Some(id);
        header = header.push(with_tooltip(
            button(text("⋯").size(13).style(text::secondary))
                .padding([0, 6])
                .style(style::list_item(open))
                .on_press(arrange_message(arrange::Message::CategoryMenu(
                    (!open).then(|| id.clone()),
                ))),
            "Rename, sort, move or delete the category".into(),
        ));
    }
    container(header)
        .padding(Padding::new(8.0).top(14).bottom(4))
        .into()
}

/// Small upper-case label above a group of rooms.
fn section_title<'a>(title: &str) -> Element<'a, Message> {
    text(title.to_uppercase())
        .size(11)
        .font(fonts::SEMIBOLD)
        .style(text::secondary)
        .into()
}

/// The field renaming a category: Enter saves, Esc cancels.
fn name_input<'a>(name: &str) -> Element<'a, Message> {
    container(
        text_input("Category name", name)
            .id(NAME_INPUT)
            .on_input(|name| arrange_message(arrange::Message::Name(name)))
            .on_submit(arrange_message(arrange::Message::SaveName))
            .style(style::input)
            .padding([4, 8])
            .size(13),
    )
    .padding(Padding::new(0.0).top(10).bottom(4))
    .into()
}

fn menu<'a>(items: Vec<(String, Message)>) -> Element<'a, Message> {
    let items = items.into_iter().map(|(label, message)| {
        button(text(label).size(13))
            .width(Length::Fill)
            .padding([4, 10])
            .style(style::list_item(false))
            .on_press(message)
            .into()
    });
    container(column(items).spacing(1))
        .padding(4)
        .width(Length::Fill)
        .style(style::code_block)
        .into()
}

/// Settings, low priority and categories for a room (the star next to the
/// menu makes it a favourite).
fn room_menu<'a>(
    room: &RoomSummary,
    space: Option<&OwnedRoomId>,
    layout: &Layout,
    grouping: &Grouping,
) -> Element<'a, Message> {
    let move_to = |section| arrange_message(arrange::Message::MoveTo(room.id.clone(), section));
    let mut items = Vec::new();
    if room.kind == RoomKind::Direct {
        items.push(("Info".to_owned(), Message::ShowInfo(info::target_of(room))));
    }
    items.push((
        "Settings".to_owned(),
        Message::ShowInfo(info::Target::Room(room.id.clone())),
    ));
    if grouping.low_priority {
        items.push(match room.tag {
            RoomTag::LowPriority => ("Remove from low priority".into(), move_to(Section::Rooms)),
            _ => ("Low priority".into(), move_to(Section::LowPriority)),
        });
    }
    let current = layout.category_of(&room.id).map(|c| c.id.as_str());
    for category in &layout.categories {
        items.push(if Some(category.id.as_str()) == current {
            (
                format!("Remove from {}", category.name),
                move_to(Section::Rooms),
            )
        } else {
            (
                format!("Move to {}", category.name),
                move_to(Section::Category(category.id.clone())),
            )
        });
    }
    items.push((
        "New category…".into(),
        Message::OpenDialog(Kind::Category {
            space: space.cloned(),
            category: None,
            room: Some(room.id.clone()),
        }),
    ));
    menu(items)
}

/// Rename, choose rooms, sort, reorder and delete a category.
fn category_menu<'a>(
    layout: &Layout,
    in_space: Option<&OwnedRoomId>,
    id: &str,
) -> Element<'a, Message> {
    let Some(at) = layout.categories.iter().position(|c| c.id == id) else {
        return space().into();
    };
    let id = id.to_owned();
    let item = |label: &str, message| (label.to_owned(), arrange_message(message));
    let mut items = vec![
        item("Rename…", arrange::Message::Rename(id.clone())),
        (
            "Choose rooms…".to_owned(),
            Message::OpenDialog(Kind::Category {
                space: in_space.cloned(),
                category: Some(id.clone()),
                room: None,
            }),
        ),
    ];
    items.push(match layout.categories[at].sort {
        Sort::Activity => item("Sort A–Z", arrange::Message::Sort(id.clone(), Sort::Name)),
        Sort::Name => item(
            "Sort by recent activity",
            arrange::Message::Sort(id.clone(), Sort::Activity),
        ),
    });
    if at > 0 {
        items.push(item(
            "Move up",
            arrange::Message::MoveCategory(id.clone(), -1),
        ));
    }
    if at + 1 < layout.categories.len() {
        items.push(item(
            "Move down",
            arrange::Message::MoveCategory(id.clone(), 1),
        ));
    }
    items.push(item("Delete category", arrange::Message::Delete(id)));
    menu(items)
}

/// The room being dragged, following the cursor.
pub fn drag_preview<'a>(
    rooms: &'a [RoomSummary],
    arrange: &Arrange,
    avatars: &Avatars,
) -> Option<Element<'a, Message>> {
    let drag = arrange.dragging()?;
    let at = drag.at?;
    let room = rooms.iter().find(|r| r.id == drag.room)?;
    let chip = container(
        row![
            avatars.view(
                room.id.as_str(),
                &room.name,
                room.avatar.as_deref(),
                ROOM_AVATAR,
                Shape::Circle,
            ),
            text(&room.name).size(14),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([4, 10])
    .style(style::toolbar);
    Some(
        container(chip)
            .padding(Padding::ZERO.left(at.x + 12.0).top(at.y + 6.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

/// What the room list's "+" offers; more inside a space.
fn add_menu_view<'a>(space: Option<(OwnedRoomId, String)>) -> Element<'a, Message> {
    let item = |label: String, message: Message| {
        button(text(label).size(14))
            .width(Length::Fill)
            .padding([5, 10])
            .style(style::list_item(false))
            .on_press(message)
    };
    let dialog = |label: String, kind: Kind| item(label, Message::OpenDialog(kind));
    let mut menu = column![].spacing(1);
    menu = menu.push(dialog(
        match &space {
            Some((_, name)) => format!("New room in {name}"),
            None => "New room".into(),
        },
        Kind::NewRoom {
            space: space.clone(),
        },
    ));
    menu = menu
        .push(dialog("New direct chat".into(), Kind::DirectChat))
        .push(dialog(
            "New category…".into(),
            Kind::Category {
                space: space.as_ref().map(|(id, _)| id.clone()),
                category: None,
                room: None,
            },
        ))
        .push(dialog("Join a room…".into(), Kind::Join));
    if let Some((id, name)) = space {
        menu = menu
            .push(dialog(
                "Add existing rooms…".into(),
                Kind::AddExisting {
                    space: id.clone(),
                    name: name.clone(),
                },
            ))
            .push(dialog(
                "Invite people to the space…".into(),
                Kind::Invite {
                    room: id.clone(),
                    name: name.clone(),
                },
            ))
            .push(item(
                "Space settings…".into(),
                Message::ShowInfo(info::Target::Room(id.clone())),
            ))
            .push(dialog(
                "Leave the space…".into(),
                Kind::Leave { room: id, name },
            ));
    }
    container(menu)
        .padding(4)
        .width(Length::Fill)
        .style(style::code_block)
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

impl<'a> RoomListView<'a> {
    /// Whether `room`, in a section `collapsed` or not, has its row:
    /// collapsed sections still show the open room and unread ones.
    fn shows(&self, room: &RoomSummary, collapsed: bool) -> bool {
        !collapsed || self.open == Some(&room.id) || room.unread_messages > 0
    }

    /// The pinned Notes row: like a room's, but not dragged or tagged, and
    /// greyed until the notes are started, which pressing it does.
    fn notes_entry(&self, notes: Notes<'a>) -> Element<'a, Message> {
        let room = match notes {
            Notes::Room(room) => room,
            Notes::Missing => {
                let line = row![
                    container(icons::notebook(16.0, faint)).center(ROOM_AVATAR),
                    text("Notes").size(14).style(|theme: &Theme| text::Style {
                        color: Some(faint(theme)),
                    }),
                ]
                .spacing(8)
                .align_y(Alignment::Center);
                return hint(
                    button(container(line).center_y(Length::Fill))
                        .width(Length::Fill)
                        .height(ROOM_ROW)
                        .padding([0, 8])
                        .style(style::list_item(false))
                        .on_press(Message::StartNotes),
                    "Start your notes to self",
                    Position::Bottom,
                );
            }
        };
        let selected = self.open == Some(&room.id);
        let menu_open = self.arrange.room_menu.as_ref() == Some(&room.id);
        let font = if room.unread_messages > 0 {
            fonts::SEMIBOLD
        } else {
            fonts::INTER
        };
        let name = ellipsis("Notes").size(14).font(font);
        let name = match unread_count(room) {
            Some(count) => followed_by(name, count, 6.0),
            None => name.width(Length::Fill).into(),
        };
        let mut line = row![
            container(icons::notebook(16.0, |theme| {
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(0.7)
            }))
            .center(ROOM_AVATAR),
            name
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let toggle_menu = arrange_message(arrange::Message::RoomMenu(
            (!menu_open).then(|| room.id.clone()),
        ));
        if self.arrange.notes_hovered || menu_open {
            line = line.push(with_tooltip(
                button(text("⋯").size(13))
                    .padding([0, 4])
                    .style(style::list_item(menu_open))
                    .on_press(toggle_menu.clone()),
                "Settings (or right-click)".into(),
            ));
        } else if let Some(later) = Later::of(self.scheduled, &room.id) {
            line = line.push(later.view());
        }
        let entry = button(container(line).center_y(Length::Fill))
            .width(Length::Fill)
            .height(ROOM_ROW)
            .padding([0, 8])
            .style(style::list_item(selected))
            .on_press(Message::OpenRoom(room.id.clone()));
        let area = mouse_area(entry)
            .on_enter(arrange_message(arrange::Message::NotesRow(true)))
            .on_exit(arrange_message(arrange::Message::NotesRow(false)))
            .on_right_press(toggle_menu);
        let mut block = column![area];
        if menu_open {
            block = block.push(menu(vec![(
                "Settings".to_owned(),
                Message::ShowInfo(info::Target::Room(room.id.clone())),
            )]));
        }
        // Found by `find_out_of_view` when unread.
        if room.unread_messages > 0 {
            container(block).id(unread_id(&room.id)).into()
        } else {
            block.into()
        }
    }

    /// A room's row: opens the room on click, can be dragged to another
    /// section, and has a menu on right click or its "⋯".
    fn room_entry(&self, room: &'a RoomSummary) -> Element<'a, Message> {
        let selected = self.open == Some(&room.id);
        let menu_open = self.arrange.room_menu.as_ref() == Some(&room.id);
        let hovered = self.arrange.hovered.as_ref() == Some(&room.id);
        let font = if room.unread_messages > 0 {
            fonts::SEMIBOLD
        } else {
            fonts::INTER
        };
        // People keep their pictures; rooms may show whether they are open.
        let avatar = if self.room_pictures || room.kind != RoomKind::Room {
            let picture = self.avatars.view(
                room.id.as_str(),
                &room.name,
                room.avatar.as_deref(),
                ROOM_AVATAR,
                Shape::Circle,
            );
            let person = room
                .direct_user
                .as_ref()
                .filter(|_| room.kind == RoomKind::Direct);
            let presence = person.and_then(|user| self.presences.get(user));
            let tint = self.tint;
            presence::with_dot(picture, presence, ROOM_AVATAR, move |theme| {
                tint.color(theme)
            })
        } else {
            access_symbol(room.public)
        };
        let name = ellipsis(&room.name).size(14).font(font);
        // Right after the name, so nothing moves when the star and menu
        // show on hover.
        let name = match unread_count(room) {
            Some(count) => followed_by(name, count, 6.0),
            None => name.width(Length::Fill).into(),
        };
        let mut line = row![avatar, name].spacing(8).align_y(Alignment::Center);
        let menu = arrange_message(arrange::Message::RoomMenu(
            (!menu_open).then(|| room.id.clone()),
        ));
        let dragging = self.arrange.dragging().is_some();
        let actions = (hovered || menu_open) && !room.invited && !dragging;
        // On hover the star and menu take the place of the scheduled-message
        // clock, so nothing else moves. Favourites have their own section.
        if actions {
            line = line.push(star_button(&room.id, room.tag == RoomTag::Favourite));
            line = line.push(with_tooltip(
                button(text("⋯").size(13))
                    .padding([0, 4])
                    .style(style::list_item(menu_open))
                    .on_press(menu.clone()),
                "Settings, low priority, categories (or right-click)".into(),
            ));
        } else if let Some(later) = Later::of(self.scheduled, &room.id) {
            line = line.push(later.view());
        }
        let entry = button(container(line).center_y(Length::Fill))
            .width(Length::Fill)
            .height(ROOM_ROW)
            .padding([0, 8])
            .style(style::list_item(selected))
            .on_press(Message::OpenRoom(room.id.clone()));
        let row = |enter| arrange_message(arrange::Message::Row(room.id.clone(), enter));
        let mut area = mouse_area(entry).on_enter(row(true)).on_exit(row(false));
        if !room.invited {
            area = area.on_right_press(menu);
        }
        // Found by `find_out_of_view` when unread.
        if room.unread_messages > 0 {
            container(area).id(unread_id(&room.id)).into()
        } else {
            area.into()
        }
    }
}

/// Greyed text and icons, e.g. of Notes not started yet.
fn faint(theme: &Theme) -> Color {
    theme
        .extended_palette()
        .background
        .base
        .text
        .scale_alpha(0.45)
}

/// In place of a room's picture: # for a public room, a lock for one only
/// invited people (or a space's members) may join.
fn access_symbol<'a>(public: bool) -> Element<'a, Message> {
    let symbol: Element<'a, Message> = if public {
        text("#")
            .size(16)
            .font(fonts::SEMIBOLD)
            .style(|theme: &Theme| text::Style {
                color: Some(
                    theme
                        .extended_palette()
                        .background
                        .base
                        .text
                        .scale_alpha(0.55),
                ),
            })
            .into()
    } else {
        icons::lock(14.0, |theme| {
            theme
                .extended_palette()
                .background
                .base
                .text
                .scale_alpha(0.5)
        })
    };
    container(symbol).center(ROOM_AVATAR).into()
}

/// Unread messages, in red when they mention the user.
fn unread_count<'a>(room: &RoomSummary) -> Option<Element<'a, Message>> {
    let highlight = room.unread_mentions > 0;
    let count = if highlight {
        room.unread_mentions
    } else {
        room.unread_messages
    };
    (count > 0).then(|| {
        container(text(count.to_string()).size(11).font(fonts::SEMIBOLD))
            .padding([1, 6])
            .style(move |t| style::badge(t, highlight))
            .into()
    })
}

/// A gold star on favourites; an outline making a room one.
pub fn star_button<'a>(room: &OwnedRoomId, favourite: bool) -> Element<'a, Message> {
    let tag = if favourite {
        RoomTag::None
    } else {
        RoomTag::Favourite
    };
    star_for(
        arrange_message(arrange::Message::Tag(room.clone(), tag)),
        favourite,
    )
}

/// A star sending `message` when clicked.
pub fn star_for<'a>(message: Message, favourite: bool) -> Element<'a, Message> {
    let tip = if favourite {
        "Remove from favourites"
    } else {
        "Favourite"
    };
    with_tooltip(
        button(icons::star(14.0, favourite, |theme| {
            theme
                .extended_palette()
                .background
                .base
                .text
                .scale_alpha(0.55)
        }))
        .padding([2, 3])
        .style(style::list_item(false))
        .on_press(message),
        tip.to_owned(),
    )
}

fn with_tooltip<'a>(content: impl Into<Element<'a, Message>>, tip: String) -> Element<'a, Message> {
    hint(content, tip, Position::Right)
}

/// The room list's scrollable.
const ROOM_LIST: &str = "room-list";
/// Room left visible beyond an unread room scrolled to (the arrow covers
/// the list's edge).
const SCROLL_MARGIN: f32 = 44.0;

fn unread_id(room: &OwnedRoomId) -> Id {
    Id::from(format!("unread:{room}"))
}

/// Unread rooms scrolled out of the room list's view, above and below:
/// how many, and the offset showing the nearest.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OutOfView {
    pub above: Option<(usize, f32)>,
    pub below: Option<(usize, f32)>,
}

/// Look where the unread rooms' rows are in the room list.
pub fn find_out_of_view(rooms: &[RoomSummary]) -> Task<OutOfView> {
    let ids = rooms
        .iter()
        .filter(|r| r.kind != RoomKind::Space && r.unread_messages > 0)
        .map(|r| unread_id(&r.id))
        .collect();
    iced::advanced::widget::operate(FindUnread {
        ids,
        list: None,
        rows: Vec::new(),
    })
}

/// Scroll the room list to `offset`.
pub fn scroll_to(offset: f32) -> Task<Message> {
    iced::widget::operation::scroll_to(
        ROOM_LIST,
        iced::widget::operation::AbsoluteOffset {
            x: None,
            y: Some(offset.max(0.0)),
        },
    )
}

struct FindUnread {
    ids: Vec<Id>,
    /// The list's bounds and scroll offset.
    list: Option<(Rectangle, Vector)>,
    /// Unread rows, in content positions.
    rows: Vec<Rectangle>,
}

impl Operation<OutOfView> for FindUnread {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<OutOfView>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        if id == Some(&Id::new(ROOM_LIST)) {
            self.list = Some((bounds, translation));
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id.is_some_and(|id| self.ids.contains(id)) {
            self.rows.push(bounds);
        }
    }

    fn finish(&self) -> operation::Outcome<OutOfView> {
        operation::Outcome::Some(match self.list {
            Some((list, scrolled)) => out_of_view(list, scrolled.y, &self.rows),
            None => OutOfView::default(),
        })
    }
}

/// Which `rows` (in window positions, unscrolled) of a `list` scrolled by
/// `scrolled` lie wholly above or below its view.
fn out_of_view(list: Rectangle, scrolled: f32, rows: &[Rectangle]) -> OutOfView {
    let (top, bottom) = (scrolled, scrolled + list.height);
    // Row tops and bottoms relative to the list's content.
    let rows = rows
        .iter()
        .map(|row| (row.y - list.y, row.y - list.y + row.height));
    let above: Vec<f32> = rows
        .clone()
        .filter(|(_, end)| *end <= top)
        .map(|(y, _)| y)
        .collect();
    let below: Vec<f32> = rows
        .filter(|(y, _)| *y >= bottom)
        .map(|(_, end)| end)
        .collect();
    let nearest_above = above.iter().copied().reduce(f32::max);
    let nearest_below = below.iter().copied().reduce(f32::min);
    OutOfView {
        above: nearest_above.map(|y| (above.len(), y - SCROLL_MARGIN)),
        below: nearest_below.map(|end| (below.len(), end - list.height + SCROLL_MARGIN)),
    }
}

/// Arrows over the list's edges pointing at unread rooms out of view.
fn out_of_view_arrows<'a>(
    list: Element<'a, Message>,
    out_of_view: OutOfView,
) -> Element<'a, Message> {
    let arrow = |up: bool, (count, offset): (usize, f32)| {
        let label = match count {
            1 => "1 unread".to_owned(),
            n => format!("{n} unread"),
        };
        let tip = if up {
            "Scroll up to unread rooms"
        } else {
            "Scroll down to unread rooms"
        };
        hint(
            button(
                row![
                    icons::arrow(12.0, up, |theme| theme.extended_palette().primary.base.text),
                    text(label).size(12).font(fonts::SEMIBOLD),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding([3, 10])
            .style(|theme, status| {
                let style = style::primary_button(theme, status);
                button::Style {
                    border: style.border.rounded(12),
                    shadow: iced::Shadow {
                        color: Color::BLACK.scale_alpha(0.2),
                        offset: Vector::new(0.0, 1.0),
                        blur_radius: 4.0,
                    },
                    ..style
                }
            })
            .on_press(Message::ScrollRoomList(offset)),
            tip,
            Position::Right,
        )
    };
    let mut layers = stack![list];
    if let Some(above) = out_of_view.above {
        layers = layers.push(
            container(arrow(true, above))
                .center_x(Length::Fill)
                .padding(Padding::ZERO.top(6)),
        );
    }
    if let Some(below) = out_of_view.below {
        layers = layers.push(
            container(arrow(false, below))
                .center_x(Length::Fill)
                .align_bottom(Length::Fill)
                .padding(Padding::ZERO.bottom(8)),
        );
    }
    layers.into()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use mtrx_core::OwnedUserId;

    use super::*;

    fn room(id: &str, kind: RoomKind, parents: &[&str]) -> RoomSummary {
        RoomSummary {
            id: id.try_into().unwrap(),
            name: id.into(),
            avatar: None,
            topic: None,
            kind,
            invited: false,
            encrypted: false,
            public: false,
            unread_messages: 0,
            unread_mentions: 0,
            unread_notifications: 0,
            last_activity: None,
            parents: parents.iter().map(|p| (*p).try_into().unwrap()).collect(),
            tag: RoomTag::None,
            direct_user: None,
            alias: None,
            joined_members: 0,
        }
    }

    /// A space with a room below it, a direct chat with one of its members
    /// and a room outside it.
    fn rooms() -> Vec<RoomSummary> {
        vec![
            room("!space:x", RoomKind::Space, &[]),
            room("!inside:x", RoomKind::Room, &["!space:x"]),
            RoomSummary {
                direct_user: Some("@bob:x".try_into().unwrap()),
                ..room("!bob:x", RoomKind::Direct, &[])
            },
            room("!loose:x", RoomKind::Room, &[]),
        ]
    }

    fn unread_in(rooms: &mut [RoomSummary], id: &str) {
        let room = rooms.iter_mut().find(|r| r.id == id).unwrap();
        room.unread_messages = 1;
    }

    fn space() -> OwnedRoomId {
        "!space:x".try_into().unwrap()
    }

    #[test]
    fn an_unread_direct_chat_with_a_member_lights_the_space() {
        let mut rooms = rooms();
        unread_in(&mut rooms, "!bob:x");
        let mut arrange = Arrange::default();
        // Before its members are known the space lists no direct chats.
        assert!(!unread(
            &rooms,
            Some(&space()),
            &arrange,
            &Grouping::default()
        ));
        // Loaded for every top-level space, selected or not.
        assert_eq!(arrange.space_members.due(&rooms), [(space(), 0)]);
        let bob: OwnedUserId = "@bob:x".try_into().unwrap();
        arrange
            .space_members
            .loaded(space(), 0, HashSet::from([bob]));
        assert!(unread(
            &rooms,
            Some(&space()),
            &arrange,
            &Grouping::default()
        ));
        assert!(unread(&rooms, None, &arrange, &Grouping::default()));
    }

    #[test]
    fn search_finds_what_the_space_or_home_lists() {
        let rooms = rooms();
        let mut arrange = Arrange::default();
        let grouping = Grouping::default();
        let found = |space: Option<&OwnedRoomId>, arrange: &Arrange| -> Vec<String> {
            search(&rooms, space, arrange, &grouping, "")
                .iter()
                .map(|r| r.id.to_string())
                .collect()
        };
        let first = |space: Option<&OwnedRoomId>, arrange: &Arrange, filter| {
            first_match(&rooms, space, arrange, &grouping, filter).map(|r| r.id.to_string())
        };
        assert_eq!(found(Some(&space()), &arrange), ["!inside:x"]);
        assert_eq!(first(Some(&space()), &arrange, " LOO"), None);
        // Home leaves out the space's room.
        assert_eq!(found(None, &arrange), ["!bob:x", "!loose:x"]);
        assert_eq!(first(None, &arrange, " LOO").as_deref(), Some("!loose:x"));
        assert_eq!(first(None, &arrange, "ins"), None);
        // Unless it lists all rooms.
        arrange.categories.at_home.all_rooms = true;
        assert_eq!(found(None, &arrange), ["!bob:x", "!inside:x", "!loose:x"]);
        assert_eq!(first(None, &arrange, "ins").as_deref(), Some("!inside:x"));
    }

    #[test]
    fn home_lists_chats_invites_and_rooms_in_no_space_unless_set() {
        let mut rooms = rooms();
        rooms.push(RoomSummary {
            tag: RoomTag::Favourite,
            ..room("!star:x", RoomKind::Room, &["!space:x"])
        });
        rooms.push(RoomSummary {
            invited: true,
            ..room("!offer:x", RoomKind::Room, &["!space:x"])
        });
        unread_in(&mut rooms, "!inside:x");
        let (avatars, presences) = (Avatars::new(), Presences::default());
        let mut arrange = Arrange::default();
        let grouping = Grouping::default();
        // The rooms of each non-empty section, with its name.
        let home = |arrange: &Arrange| -> Vec<(String, Vec<String>)> {
            let view = list(&rooms, None, "", arrange, &avatars, &presences);
            sections(&view)
                .into_iter()
                .filter(|(_, entries)| !entries.is_empty())
                .map(|(section, entries)| {
                    let name = section_name(arrange, None, &section).to_owned();
                    (name, entries.iter().map(|r| r.id.to_string()).collect())
                })
                .collect()
        };
        let section = |name: &str, ids: &[&str]| -> (String, Vec<String>) {
            (name.into(), ids.iter().map(|id| id.to_string()).collect())
        };

        assert_eq!(
            home(&arrange),
            [
                section("Invites", &["!offer:x"]),
                section("Rooms with no space", &["!loose:x"]),
                section("Direct messages", &["!bob:x"]),
            ]
        );
        // The space's unread room does not light Home.
        assert!(!unread(&rooms, None, &arrange, &grouping));

        arrange.categories.at_home.favourites = true;
        assert_eq!(home(&arrange)[1], section("Favourites", &["!star:x"]));
        assert!(!unread(&rooms, None, &arrange, &grouping));

        arrange.categories.at_home.all_rooms = true;
        assert_eq!(
            home(&arrange),
            [
                section("Invites", &["!offer:x"]),
                section("Favourites", &["!star:x"]),
                section("Rooms", &["!inside:x", "!loose:x"]),
                section("Direct messages", &["!bob:x"]),
            ]
        );
        assert!(unread(&rooms, None, &arrange, &grouping));
        // A space's rooms are always just "Rooms".
        arrange.categories.at_home.all_rooms = false;
        assert_eq!(
            section_name(&arrange, Some(&space()), &Section::Rooms),
            "Rooms"
        );
    }

    #[test]
    fn an_unread_room_in_a_category_of_the_space_lights_it() {
        let mut rooms = rooms();
        unread_in(&mut rooms, "!loose:x");
        let mut arrange = Arrange::default();
        assert!(!unread(
            &rooms,
            Some(&space()),
            &arrange,
            &Grouping::default()
        ));
        let layout = arrange.categories.layout_mut(Some(&space()));
        let category = layout.add_category("Mine");
        let loose: OwnedRoomId = "!loose:x".try_into().unwrap();
        layout.place(&loose, Some(&category));
        assert!(unread(
            &rooms,
            Some(&space()),
            &arrange,
            &Grouping::default()
        ));
    }

    /// The room list in `space` (Home for `None`), searching `filter`.
    fn list<'a>(
        rooms: &'a [RoomSummary],
        space: Option<&'a OwnedRoomId>,
        filter: &'a str,
        arrange: &'a Arrange,
        avatars: &'a Avatars,
        presences: &'a Presences,
    ) -> RoomListView<'a> {
        RoomListView {
            rooms,
            selected_space: space,
            open: None,
            loaded: true,
            avatars,
            scheduled: &[],
            add_menu: false,
            filter,
            arrange,
            grouping: Grouping::default(),
            out_of_view: OutOfView::default(),
            width: 260.0,
            tint: Tint::default(),
            room_pictures: false,
            notes: None,
            presences,
        }
    }

    #[test]
    fn follows_the_people_of_the_direct_chats_listed() {
        let own: OwnedUserId = "@me:x".try_into().unwrap();
        let mut rooms = rooms();
        rooms.push(RoomSummary {
            direct_user: Some("@carol:x".try_into().unwrap()),
            ..room("!carol:x", RoomKind::Direct, &[])
        });
        // The Notes to self, not pinned, are listed but not followed.
        rooms.push(RoomSummary {
            direct_user: Some(own.clone()),
            ..room("!notes:x", RoomKind::Direct, &[])
        });
        let (avatars, presences) = (Avatars::new(), Presences::default());
        let mut arrange = Arrange::default();
        let people = |space, filter, arrange: &Arrange| {
            let view = list(&rooms, space, filter, arrange, &avatars, &presences);
            let mut people: Vec<String> = direct_users(&view, &own)
                .into_iter()
                .map(|u| u.to_string())
                .collect();
            people.sort();
            people
        };
        assert_eq!(people(None, "", &arrange), ["@bob:x", "@carol:x"]);
        assert_eq!(people(None, "car", &arrange), ["@carol:x"]);
        let space = space();
        assert!(people(Some(&space), "", &arrange).is_empty());
        assert_eq!(arrange.space_members.due(&rooms), [(space.clone(), 0)]);
        let bob: OwnedUserId = "@bob:x".try_into().unwrap();
        arrange
            .space_members
            .loaded(space.clone(), 0, HashSet::from([bob]));
        assert_eq!(people(Some(&space), "", &arrange), ["@bob:x"]);
    }

    #[test]
    fn keys_go_to_the_rooms_in_the_order_shown() {
        let mut rooms = rooms();
        rooms.push(RoomSummary {
            tag: RoomTag::Favourite,
            ..room("!fav:x", RoomKind::Room, &[])
        });
        rooms.push(room("!notes:x", RoomKind::Direct, &[]));
        let (avatars, presences) = (Avatars::new(), Presences::default());
        let mut arrange = Arrange::default();
        let ids = |view: &RoomListView| -> Vec<String> {
            rows(view).iter().map(|r| r.id.to_string()).collect()
        };
        let id = |id: &str| -> OwnedRoomId { id.try_into().unwrap() };

        // What Home shows: not the space's room.
        let home = list(&rooms, None, "", &arrange, &avatars, &presences);
        assert_eq!(ids(&home), ["!fav:x", "!loose:x", "!bob:x", "!notes:x"]);
        assert_eq!(
            go_to(&home, Go::Row(1))
                .map(|r| r.id.to_string())
                .as_deref(),
            Some("!loose:x")
        );
        arrange.categories.at_home.all_rooms = true;

        // Sections in their order, the pinned Notes first.
        let mut home = list(&rooms, None, "", &arrange, &avatars, &presences);
        home.notes = Some(Notes::Room(&rooms[5]));
        assert_eq!(
            ids(&home),
            ["!notes:x", "!fav:x", "!inside:x", "!loose:x", "!bob:x"]
        );
        let to = |view: &RoomListView, go| go_to(view, go).map(|r| r.id.to_string());
        // From outside the list, the first or last.
        assert_eq!(to(&home, Go::Below).as_deref(), Some("!notes:x"));
        assert_eq!(to(&home, Go::Above).as_deref(), Some("!bob:x"));
        let inside = id("!inside:x");
        home.open = Some(&inside);
        assert_eq!(to(&home, Go::Above).as_deref(), Some("!fav:x"));
        assert_eq!(to(&home, Go::Below).as_deref(), Some("!loose:x"));
        assert_eq!(to(&home, Go::Row(1)).as_deref(), Some("!fav:x"));
        assert_eq!(to(&home, Go::Row(8)), None);
        // Not past the ends.
        let bob = id("!bob:x");
        home.open = Some(&bob);
        assert_eq!(to(&home, Go::Below), None);

        // A collapsed section shows only the open room and unread ones. The
        // Notes not pinned are a direct chat.
        (arrange.categories.layout_mut(None)).toggle_collapsed(&Section::Rooms);
        unread_in(&mut rooms, "!loose:x");
        let mut home = list(&rooms, None, "", &arrange, &avatars, &presences);
        assert_eq!(ids(&home), ["!fav:x", "!loose:x", "!bob:x", "!notes:x"]);
        home.open = Some(&inside);
        assert_eq!(
            ids(&home),
            ["!fav:x", "!inside:x", "!loose:x", "!bob:x", "!notes:x"]
        );

        // While searching, what it found.
        let found = list(&rooms, None, "o", &arrange, &avatars, &presences);
        assert_eq!(ids(&found), ["!bob:x", "!loose:x", "!notes:x"]);
    }

    #[test]
    fn keys_for_going_to_rooms() {
        use iced::keyboard::key::{Code, NativeCode, Physical};

        let none = Physical::Unidentified(NativeCode::Unidentified);
        let go = |key: Key, physical, modifiers| Go::from_key(&key, physical, modifiers);
        let command_alt = Modifiers::COMMAND | Modifiers::ALT;
        let arrow = |named| Key::Named(named);
        assert_eq!(
            go(arrow(Named::ArrowUp), none, command_alt),
            Some(Go::Above)
        );
        assert_eq!(
            go(arrow(Named::ArrowDown), none, command_alt),
            Some(Go::Below)
        );
        assert_eq!(go(arrow(Named::ArrowDown), none, Modifiers::ALT), None);
        let tab = arrow(Named::Tab);
        assert_eq!(go(tab.clone(), none, Modifiers::CTRL), Some(Go::Below));
        let back = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(go(tab.clone(), none, back), Some(Go::Above));
        assert_eq!(go(tab, none, Modifiers::empty()), None);
        let digit = |c: &str| Key::Character(c.into());
        assert_eq!(go(digit("1"), none, Modifiers::COMMAND), Some(Go::Row(0)));
        assert_eq!(go(digit("9"), none, Modifiers::COMMAND), Some(Go::Row(8)));
        assert_eq!(go(digit("0"), none, Modifiers::COMMAND), None);
        assert_eq!(go(digit("1"), none, Modifiers::empty()), None);
        // On AZERTY ⌘ and the key of 1 gives "&".
        let one = Physical::Code(Code::Digit1);
        assert_eq!(go(digit("&"), one, Modifiers::COMMAND), Some(Go::Row(0)));
    }

    #[test]
    fn finds_unread_rooms_out_of_view() {
        // A list at y 100, 300 high, scrolled down by 200: it shows
        // content 200..500.
        let list = Rectangle::new(iced::Point::new(0.0, 100.0), iced::Size::new(260.0, 300.0));
        let row = |y: f32| {
            Rectangle::new(
                iced::Point::new(0.0, 100.0 + y),
                iced::Size::new(260.0, 32.0),
            )
        };
        let rows = [
            row(0.0),
            row(150.0),
            row(190.0),
            row(400.0),
            row(520.0),
            row(700.0),
        ];
        let found = out_of_view(list, 200.0, &rows);
        // 0 and 150 are above (190 shows partly); the nearest is 150.
        assert_eq!(found.above, Some((2, 150.0 - SCROLL_MARGIN)));
        // 520 and 700 are below; scrolling shows 520's bottom (552).
        assert_eq!(found.below, Some((2, 552.0 - 300.0 + SCROLL_MARGIN)));
        assert_eq!(
            out_of_view(list, 200.0, &[row(300.0)]),
            OutOfView::default()
        );
    }
}
