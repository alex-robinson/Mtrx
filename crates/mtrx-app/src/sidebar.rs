//! Space bar and room list.

use std::collections::{HashMap, HashSet};

use iced::{
    Alignment, Color, Element, Length, Padding, Theme,
    widget::{
        button, column, container, mouse_area, row, scrollable, space, stack, text, text_input,
        tooltip,
    },
};
use mtrx_core::{
    Grouping, Layout, MxcUri, OwnedRoomId, RoomKind, RoomSummary, RoomTag, ScheduledMessage,
    Section, Sort,
};

use crate::{
    app::Message,
    arrange::{self, Arrange, NAME_INPUT},
    avatar::{Avatars, Shape},
    dialog::Kind,
    ellipsis::ellipsis,
    fonts, icons,
    style::{self, ROOM_AVATAR, ROOM_LIST_WIDTH, ROOM_ROW, SPACE_AVATAR, SPACE_BAR_WIDTH},
};

/// The room search field, focused by ⌘K / Ctrl+K.
pub const SEARCH: &str = "room-search";

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

/// Rooms listed for a space: everything below it, direct chats with its
/// members, and rooms put into its categories. Home lists all of them.
/// Invites to spaces are listed too, to be answered.
fn scope<'a>(
    rooms: &'a [RoomSummary],
    space: Option<&OwnedRoomId>,
    arrange: &Arrange,
) -> Vec<&'a RoomSummary> {
    let listable = rooms
        .iter()
        .filter(|r| r.kind != RoomKind::Space || r.invited);
    let Some(space) = space else {
        return listable.collect();
    };
    let inside = descendants(rooms, space);
    let members = arrange.members(space);
    let layout = arrange.categories.layout(Some(space));
    listable
        .filter(|r| {
            inside.contains(&r.id)
                || r.direct_user
                    .as_ref()
                    .is_some_and(|u| members.is_some_and(|m| m.contains(u)))
                || layout.category_of(&r.id).is_some()
        })
        .collect()
}

/// The room list's sections and their rooms, in display order; sections
/// may be empty.
fn sections<'a>(
    rooms: &'a [RoomSummary],
    space: Option<&OwnedRoomId>,
    arrange: &Arrange,
    grouping: &Grouping,
) -> Vec<(Section, Vec<&'a RoomSummary>)> {
    arrange
        .categories
        .layout(space.map(|s| s.as_ref()))
        .group(scope(rooms, space, arrange), grouping)
}

/// Rooms and chats whose name contains `filter` (lower case), names starting
/// with it first.
fn search<'a>(rooms: &'a [RoomSummary], filter: &str) -> Vec<&'a RoomSummary> {
    let mut found: Vec<_> = rooms
        .iter()
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
pub fn first_match<'a>(rooms: &'a [RoomSummary], filter: &str) -> Option<&'a RoomSummary> {
    search(rooms, &filter.trim().to_lowercase())
        .into_iter()
        .next()
}

/// Pictures in the space bar and room list, with their sizes.
pub fn avatars<'a>(view: &RoomListView<'a>) -> Vec<(&'a MxcUri, f32)> {
    let rooms = view.rooms;
    let spaces = top_level_spaces(rooms)
        .into_iter()
        .filter_map(|s| Some((s.avatar.as_deref()?, SPACE_AVATAR)));
    let filter = view.filter.trim().to_lowercase();
    let listed = if filter.is_empty() {
        sections(rooms, view.selected_space, view.arrange, &view.grouping)
            .into_iter()
            .flat_map(|(_, entries)| entries)
            .collect()
    } else {
        search(rooms, &filter)
    };
    let listed = listed
        .into_iter()
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
    let mut list = column![
        row![
            container(ellipsis(title).size(16).font(fonts::SEMIBOLD))
                .padding([4, 8])
                .width(Length::Fill),
            with_tooltip(add, "Add".into()),
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
            text_input("Search rooms  (⌘K)", filter)
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
        list = list.extend(arranged(&view));
    } else {
        let found = search(rooms, &filter);
        if found.is_empty() {
            list = list.push(
                container(text("No rooms match.").size(12).style(text::secondary)).padding([8, 8]),
            );
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
    container(scrollable(list))
        .width(ROOM_LIST_WIDTH)
        .height(Length::Fill)
        .style(style::panel)
        .into()
}

/// The sections of the room list, with menus, the category being named,
/// and drop targets while dragging.
fn arranged<'a>(view: &RoomListView<'a>) -> Vec<Element<'a, Message>> {
    let arrange = view.arrange;
    let layout = arrange
        .categories
        .layout(view.selected_space.map(|s| s.as_ref()));
    let dragging = arrange.dragging().is_some();
    let mut elements = Vec::new();
    for (section, entries) in sections(view.rooms, view.selected_space, arrange, &view.grouping) {
        // A new category appears after the others.
        if section == Section::Rooms
            && let Some(naming) = arrange.naming.as_ref().filter(|n| n.category.is_none())
        {
            elements.push(name_input(&naming.name, "New category"));
        }
        let shown = match section {
            Section::Invites => !entries.is_empty(),
            Section::Category(_) => true,
            _ => dragging || !entries.is_empty(),
        };
        if !shown {
            continue;
        }
        let collapsed = layout.is_collapsed(&section);
        let renaming = arrange.naming.as_ref().filter(
            |n| matches!(&section, Section::Category(id) if n.category.as_ref() == Some(id)),
        );
        let mut block = column![match renaming {
            Some(naming) => name_input(&naming.name, "Category name"),
            None => section_header(layout, &section, collapsed, arrange),
        }];
        if let Section::Category(id) = &section
            && arrange.category_menu.as_ref() == Some(id)
        {
            block = block.push(category_menu(layout, id));
        }
        let empty = entries.is_empty();
        for room in entries {
            // Collapsed sections still show the open room and unread ones.
            if collapsed && view.open != Some(&room.id) && room.unread_messages == 0 {
                continue;
            }
            block = block.push(view.room_entry(room));
            if arrange.room_menu.as_ref() == Some(&room.id) {
                block = block.push(room_menu(room, layout, &view.grouping));
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

fn section_name<'a>(layout: &'a Layout, section: &Section) -> &'a str {
    match section {
        Section::Invites => "Invites",
        Section::Favourites => "Favourites",
        Section::Category(id) => layout.category(id).map_or("", |c| c.name.as_str()),
        Section::Rooms => "Rooms",
        Section::People => "Direct messages",
        Section::LowPriority => "Low priority",
    }
}

/// A section's name, which collapses and expands it; a category's also
/// has its menu.
fn section_header<'a>(
    layout: &Layout,
    section: &Section,
    collapsed: bool,
    arrange: &Arrange,
) -> Element<'a, Message> {
    let chevron = if collapsed { "▸" } else { "▾" };
    let title = button(
        row![
            text(chevron).size(10).style(text::secondary),
            section_title(section_name(layout, section)),
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
        header = header.push(
            button(text("⋯").size(13).style(text::secondary))
                .padding([0, 6])
                .style(style::list_item(open))
                .on_press(arrange_message(arrange::Message::CategoryMenu(
                    (!open).then(|| id.clone()),
                ))),
        );
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

/// The field naming a new or renamed category: Enter saves, Esc cancels.
fn name_input<'a>(name: &str, placeholder: &str) -> Element<'a, Message> {
    container(
        text_input(placeholder, name)
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

/// Favourite, low priority and categories for a room.
fn room_menu<'a>(room: &RoomSummary, layout: &Layout, grouping: &Grouping) -> Element<'a, Message> {
    let move_to = |section| arrange_message(arrange::Message::MoveTo(room.id.clone(), section));
    let mut items = Vec::new();
    if grouping.favourites {
        items.push(match room.tag {
            RoomTag::Favourite => ("Remove from favourites".into(), move_to(Section::Rooms)),
            _ => ("Favourite".into(), move_to(Section::Favourites)),
        });
    }
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
        arrange_message(arrange::Message::NewCategory(Some(room.id.clone()))),
    ));
    menu(items)
}

/// Rename, sort, reorder and delete a category.
fn category_menu<'a>(layout: &Layout, id: &str) -> Element<'a, Message> {
    let Some(at) = layout.categories.iter().position(|c| c.id == id) else {
        return space().into();
    };
    let id = id.to_owned();
    let item = |label: &str, message| (label.to_owned(), arrange_message(message));
    let mut items = vec![item("Rename…", arrange::Message::Rename(id.clone()))];
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
        .push(item(
            "New category".into(),
            arrange_message(arrange::Message::NewCategory(None)),
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
        let avatar = self.avatars.view(
            room.id.as_str(),
            &room.name,
            room.avatar.as_deref(),
            ROOM_AVATAR,
            Shape::Circle,
        );
        let name = ellipsis(&room.name).size(14).font(font).width(Length::Fill);
        let mut line = row![avatar, name].spacing(8).align_y(Alignment::Center);
        if let Some(later) = Later::of(self.scheduled, &room.id) {
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
        let menu = arrange_message(arrange::Message::RoomMenu(
            (!menu_open).then(|| room.id.clone()),
        ));
        let dragging = self.arrange.dragging().is_some();
        if (hovered || menu_open) && !room.invited && !dragging {
            line = line.push(
                button(text("⋯").size(13))
                    .padding([0, 4])
                    .style(style::list_item(menu_open))
                    .on_press(menu.clone()),
            );
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
        area.into()
    }
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
