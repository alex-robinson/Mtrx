//! Arranging the room list: the user's categories, favourites and low
//! priority, by dragging rooms between sections or through menus.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use iced::{Point, Subscription, Task, event, keyboard, mouse};
use mtrx_core::{
    AtHome, Grouping, OwnedRoomId, OwnedUserId, RoomCategories, RoomSummary, RoomTag, Section,
    Session, Sort, SpaceMembers, home_rooms, space_rooms,
};

use tracing::warn;

use crate::app;

/// The field naming a category.
pub const NAME_INPUT: &str = "category-name";
/// How far the cursor moves before a press on a room becomes a drag.
const DRAG_THRESHOLD: f32 = 5.0;
/// How long a saved tag may still differ from the room list (sync lags
/// behind the server's answer).
const TAG_SYNC: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub enum Message {
    Categories(RoomCategories),
    /// The joined members of a space that had this many.
    SpaceMembers(OwnedRoomId, u64, Result<HashSet<OwnedUserId>, String>),
    /// The cursor entered (`true`) or left a room row.
    Row(OwnedRoomId, bool),
    /// The cursor entered or left the pinned Notes row, which is not dragged.
    NotesRow(bool),
    /// The modifier keys changed, e.g. ⌥ held to show only one section.
    Modifiers(keyboard::Modifiers),
    /// The left mouse button, anywhere.
    Pressed,
    Released,
    Moved(Point),
    /// While dragging, the cursor entered (`true`) or left a section.
    Target(Section, bool),
    ToggleCollapsed(Section),
    /// Collapse (`true`) or expand these sections at once.
    Collapse(Vec<(Section, bool)>),
    /// Open or close the menu of which sections a space shows expanded.
    ShowMenu(bool),
    RoomMenu(Option<OwnedRoomId>),
    CategoryMenu(Option<String>),
    MoveTo(OwnedRoomId, Section),
    /// Favourite (the star), low priority or neither, keeping the room's
    /// category.
    Tag(OwnedRoomId, RoomTag),
    /// Create or change a category, from its dialog.
    SaveCategory(CategoryEdit),
    Rename(String),
    Name(String),
    SaveName,
    Delete(String),
    /// Swap two sections' places.
    SwapSections(Section, Section),
    Sort(String, Sort),
    /// Which rooms of spaces Home lists too, from the settings.
    AtHome(AtHome),
    TagSaved(OwnedRoomId, Result<(), String>),
    Saved(Result<(), String>),
}

/// A room pressed, and dragged once the cursor moved far enough.
pub struct Drag {
    pub room: OwnedRoomId,
    start: Option<Point>,
    pub at: Option<Point>,
    pub active: bool,
}

/// A category being renamed.
pub struct Naming {
    pub category: String,
    pub name: String,
}

/// A new category, or a changed one (`category`), of `space` (Home for
/// `None`): its name and rooms.
#[derive(Debug, Clone)]
pub struct CategoryEdit {
    pub space: Option<OwnedRoomId>,
    pub category: Option<String>,
    pub name: String,
    pub rooms: Vec<OwnedRoomId>,
}

impl CategoryEdit {
    /// Make the change in `categories`.
    fn apply(&self, categories: &mut RoomCategories) {
        let layout = categories.layout_mut(self.space.as_deref());
        let id = match &self.category {
            Some(id) => {
                let Some(category) = layout.category_mut(id) else {
                    return;
                };
                category.name = self.name.clone();
                // Rooms no longer in it go back to their default section.
                let gone: Vec<_> = (category.rooms.iter())
                    .filter(|r| !self.rooms.contains(r))
                    .cloned()
                    .collect();
                for room in gone {
                    layout.place(&room, None);
                }
                id.clone()
            }
            None => layout.add_category(&self.name),
        };
        for room in &self.rooms {
            // Those already in it keep their place.
            if layout.category_of(room).map(|c| &c.id) != Some(&id) {
                layout.place(room, Some(&id));
            }
        }
    }
}

struct PendingTag {
    tag: RoomTag,
    previous: RoomTag,
    saved: Option<Instant>,
}

#[derive(Default)]
pub struct Arrange {
    pub categories: RoomCategories,
    /// Tags set here that the room list does not show yet.
    pending_tags: HashMap<OwnedRoomId, PendingTag>,
    /// Joined members of the top-level spaces, for the direct chats listed
    /// in them.
    pub space_members: SpaceMembers,
    /// The room row under the cursor.
    pub hovered: Option<OwnedRoomId>,
    pub notes_hovered: bool,
    pub drag: Option<Drag>,
    /// The section a drag would drop into.
    pub target: Option<Section>,
    pub room_menu: Option<OwnedRoomId>,
    pub category_menu: Option<String>,
    pub show_menu: bool,
    pub naming: Option<Naming>,
    pub modifiers: keyboard::Modifiers,
}

/// What [`Arrange::update`] works on.
pub struct Context<'a> {
    pub session: &'a Arc<Session>,
    pub space: Option<&'a OwnedRoomId>,
    pub rooms: &'a mut [RoomSummary],
    pub grouping: Grouping,
}

impl Arrange {
    pub fn new(session: &Session) -> Self {
        Self {
            categories: session.room_categories(),
            ..Self::default()
        }
    }

    pub fn dragging(&self) -> Option<&Drag> {
        self.drag.as_ref().filter(|d| d.active)
    }

    /// What `space` lists, or Home for `None`, in the order of `rooms`: the
    /// room list's rooms, what its search finds and what lights the space
    /// bar's dot. A space whose members are not loaded yet lists no direct
    /// chats; Home lists the rooms of spaces as [`AtHome`] says.
    pub fn listed<'a>(
        &self,
        rooms: &'a [RoomSummary],
        space: Option<&OwnedRoomId>,
        grouping: &Grouping,
    ) -> Vec<&'a RoomSummary> {
        match space {
            None => home_rooms(rooms, &self.categories, grouping),
            Some(space) => space_rooms(
                rooms,
                space,
                self.space_members.get(space),
                self.categories.layout(Some(space)),
            ),
        }
    }

    /// Load the members of the top-level spaces that appeared or whose
    /// number of members changed, for their direct chats.
    pub fn load_space_members(
        &mut self,
        session: &Arc<Session>,
        rooms: &[RoomSummary],
    ) -> Task<app::Message> {
        Task::batch(
            self.space_members
                .due(rooms)
                .into_iter()
                .map(|(space, count)| {
                    let session = session.clone();
                    Task::perform(
                        {
                            let space = space.clone();
                            async move {
                                session
                                    .joined_member_ids(&space)
                                    .await
                                    .map_err(|e| e.to_string())
                            }
                        },
                        move |result| {
                            app::Message::Arrange(Message::SpaceMembers(
                                space.clone(),
                                count,
                                result,
                            ))
                        },
                    )
                }),
        )
    }

    /// Keep tags set here until the room list shows them.
    pub fn rooms_updated(&mut self, rooms: &mut [RoomSummary]) {
        self.pending_tags.retain(|id, pending| {
            let Some(room) = rooms.iter_mut().find(|r| &r.id == id) else {
                return false;
            };
            let synced =
                room.tag == pending.tag || pending.saved.is_some_and(|at| at.elapsed() > TAG_SYNC);
            if !synced {
                room.tag = pending.tag;
            }
            !synced
        });
    }

    /// Close what is open; false if nothing was.
    pub fn escape(&mut self) -> bool {
        let open = self.drag.is_some()
            || self.room_menu.is_some()
            || self.category_menu.is_some()
            || self.show_menu
            || self.naming.is_some();
        self.drag = None;
        self.target = None;
        self.room_menu = None;
        self.category_menu = None;
        self.show_menu = false;
        self.naming = None;
        open
    }

    pub fn update(&mut self, message: Message, context: Context<'_>) -> Task<app::Message> {
        let space = context.space.cloned();
        match message {
            Message::Categories(categories) => self.categories = categories,
            Message::SpaceMembers(space, count, Ok(joined)) => {
                self.space_members.loaded(space, count, joined);
            }
            Message::SpaceMembers(space, _, Err(e)) => {
                warn!(%space, "could not load the space's members: {e}");
            }
            Message::Row(room, true) => self.hovered = Some(room),
            Message::Row(room, false) => {
                if self.hovered.as_ref() == Some(&room) {
                    self.hovered = None;
                }
            }
            Message::NotesRow(hovered) => self.notes_hovered = hovered,
            Message::Modifiers(modifiers) => self.modifiers = modifiers,
            Message::Pressed => {
                self.drag = self.hovered.clone().map(|room| Drag {
                    room,
                    start: None,
                    at: None,
                    active: false,
                });
            }
            Message::Moved(at) => {
                if let Some(drag) = &mut self.drag {
                    let start = *drag.start.get_or_insert(at);
                    drag.at = Some(at);
                    if !drag.active && start.distance(at) > DRAG_THRESHOLD {
                        drag.active = true;
                        self.room_menu = None;
                        self.category_menu = None;
                        self.show_menu = false;
                    }
                }
            }
            Message::Released => {
                let drag = self.drag.take();
                if let (Some(drag), Some(target)) = (drag.filter(|d| d.active), self.target.take())
                {
                    return self.move_to(drag.room, target, context);
                }
                self.target = None;
            }
            Message::Target(section, true) => self.target = Some(section),
            Message::Target(section, false) => {
                if self.target.as_ref() == Some(&section) {
                    self.target = None;
                }
            }
            Message::ToggleCollapsed(section) => {
                return self.change(context.session, move |c| {
                    c.layout_mut(space.as_deref()).toggle_collapsed(&section)
                });
            }
            Message::Collapse(sections) => {
                return self.change(context.session, move |c| {
                    let layout = c.layout_mut(space.as_deref());
                    for (section, collapsed) in &sections {
                        layout.set_collapsed(section, *collapsed);
                    }
                });
            }
            Message::ShowMenu(open) => {
                self.show_menu = open;
                self.room_menu = None;
                self.category_menu = None;
            }
            Message::RoomMenu(room) => {
                self.room_menu = room;
                self.category_menu = None;
            }
            Message::CategoryMenu(category) => {
                self.category_menu = category;
                self.room_menu = None;
            }
            Message::MoveTo(room, section) => {
                self.room_menu = None;
                return self.move_to(room, section, context);
            }
            Message::Tag(room, tag) => {
                self.room_menu = None;
                let Some(summary) = context.rooms.iter_mut().find(|r| r.id == room) else {
                    return Task::none();
                };
                return self.set_tag(summary, tag, context.session);
            }
            Message::SaveCategory(edit) => {
                let Context {
                    session,
                    rooms,
                    grouping,
                    ..
                } = context;
                let layout = self.categories.layout(edit.space.as_deref());
                let placed: Vec<OwnedRoomId> = (edit.rooms.iter())
                    .filter(|room| {
                        edit.category.is_none()
                            || layout.category_of(room).map(|c| &c.id) != edit.category.as_ref()
                    })
                    .cloned()
                    .collect();
                // Rooms put into it are tagged as if dragged there.
                let mut tasks: Vec<_> = (rooms.iter_mut())
                    .filter(|r| placed.contains(&r.id))
                    .map(|summary| {
                        let tag = tag_elsewhere(summary.tag, grouping);
                        self.set_tag(summary, tag, session)
                    })
                    .collect();
                tasks.push(self.change(session, move |c| edit.apply(c)));
                return Task::batch(tasks);
            }
            Message::Rename(id) => {
                self.category_menu = None;
                let layout = self.categories.layout(space.as_deref());
                let name = layout.category(&id).map(|c| c.name.clone());
                self.naming = Some(Naming {
                    category: id,
                    name: name.unwrap_or_default(),
                });
                return iced::widget::operation::focus(NAME_INPUT);
            }
            Message::Name(name) => {
                if let Some(naming) = &mut self.naming {
                    naming.name = name;
                }
            }
            Message::SaveName => {
                let Some(naming) = self.naming.take() else {
                    return Task::none();
                };
                let name = naming.name.trim().to_owned();
                if name.is_empty() {
                    return Task::none();
                }
                return self.change(context.session, move |c| {
                    if let Some(category) = c
                        .layout_mut(space.as_deref())
                        .category_mut(&naming.category)
                    {
                        category.name = name;
                    }
                });
            }
            Message::Delete(id) => {
                self.category_menu = None;
                return self.change(context.session, move |c| {
                    c.layout_mut(space.as_deref()).remove_category(&id)
                });
            }
            Message::SwapSections(a, b) => {
                return self.change(context.session, move |c| {
                    c.layout_mut(space.as_deref()).swap_sections(&a, &b)
                });
            }
            Message::Sort(id, sort) => {
                self.category_menu = None;
                return self.change(context.session, move |c| {
                    if let Some(category) = c.layout_mut(space.as_deref()).category_mut(&id) {
                        category.sort = sort;
                    }
                });
            }
            Message::AtHome(at_home) => {
                return self.change(context.session, move |c| c.at_home = at_home);
            }
            Message::TagSaved(room, Ok(())) => {
                if let Some(pending) = self.pending_tags.get_mut(&room) {
                    pending.saved = Some(Instant::now());
                }
            }
            Message::TagSaved(room, Err(e)) => {
                if let Some(pending) = self.pending_tags.remove(&room)
                    && let Some(summary) = context.rooms.iter_mut().find(|r| r.id == room)
                {
                    summary.tag = pending.previous;
                }
                return failed(e);
            }
            Message::Saved(Ok(())) => {}
            Message::Saved(Err(e)) => return failed(e),
        }
        Task::none()
    }

    /// Put a room into a section: into or out of a category of the shown
    /// space, and its tag set to match.
    fn move_to(
        &mut self,
        room: OwnedRoomId,
        section: Section,
        context: Context<'_>,
    ) -> Task<app::Message> {
        let Context {
            session,
            space,
            rooms,
            grouping,
        } = context;
        let Some(summary) = rooms.iter_mut().find(|r| r.id == room) else {
            return Task::none();
        };
        let category = match &section {
            Section::Invites => return Task::none(),
            Section::Category(id) => Some(id.clone()),
            _ => None,
        };
        let tag = tag_in(&section, summary.tag, grouping);
        let mut tasks = vec![self.set_tag(summary, tag, session)];
        let layout = self.categories.layout(space.map(|s| s.as_ref()));
        if layout.category_of(&room).map(|c| &c.id) != category.as_ref() {
            let space = space.cloned();
            tasks.push(self.change(session, move |c| {
                c.layout_mut(space.as_deref())
                    .place(&room, category.as_deref())
            }));
        }
        Task::batch(tasks)
    }

    /// Tag a room here at once, and save it.
    fn set_tag(
        &mut self,
        summary: &mut RoomSummary,
        tag: RoomTag,
        session: &Arc<Session>,
    ) -> Task<app::Message> {
        if tag == summary.tag {
            return Task::none();
        }
        let room = summary.id.clone();
        self.pending_tags.insert(
            room.clone(),
            PendingTag {
                tag,
                previous: summary.tag,
                saved: None,
            },
        );
        summary.tag = tag;
        let session = session.clone();
        Task::perform(
            {
                let room = room.clone();
                async move {
                    session
                        .set_room_tag(&room, tag)
                        .await
                        .map_err(|e| e.to_string())
                }
            },
            move |result| app::Message::Arrange(Message::TagSaved(room.clone(), result)),
        )
    }

    /// Change the categories here at once, and save them.
    fn change(
        &mut self,
        session: &Arc<Session>,
        change: impl FnOnce(&mut RoomCategories) + Clone + Send + 'static,
    ) -> Task<app::Message> {
        change.clone()(&mut self.categories);
        let session = session.clone();
        Task::perform(
            async move {
                session
                    .change_room_categories(change)
                    .await
                    .map_err(|e| e.to_string())
            },
            |result| app::Message::Arrange(Message::Saved(result)),
        )
    }

    /// The modifier keys, and the cursor while a room may be dragged.
    pub fn subscription(&self) -> Subscription<app::Message> {
        let modifiers = event::listen_with(|event, _status, _window| match event {
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                Some(app::Message::Arrange(Message::Modifiers(modifiers)))
            }
            _ => None,
        });
        let cursor = self.drag.as_ref().map(|_| {
            event::listen_with(|event, _status, _window| match event {
                iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    Some(app::Message::Arrange(Message::Moved(position)))
                }
                _ => None,
            })
        });
        Subscription::batch([modifiers].into_iter().chain(cursor))
    }
}

/// The tag of a room with `tag` moved into `section`.
fn tag_in(section: &Section, tag: RoomTag, grouping: Grouping) -> RoomTag {
    match section {
        Section::Favourites => RoomTag::Favourite,
        Section::LowPriority => RoomTag::LowPriority,
        _ => tag_elsewhere(tag, grouping),
    }
}

/// The tag of a room with `tag` moved out of its tag's section: a tag that
/// decides the section goes.
fn tag_elsewhere(tag: RoomTag, grouping: Grouping) -> RoomTag {
    match tag {
        RoomTag::Favourite if grouping.favourites => RoomTag::None,
        RoomTag::LowPriority if grouping.low_priority => RoomTag::None,
        tag => tag,
    }
}

fn failed(error: String) -> Task<app::Message> {
    Task::done(app::Message::ArrangeFailed(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(id: &str) -> OwnedRoomId {
        format!("!{id}:x").try_into().unwrap()
    }

    fn rooms_of(categories: &RoomCategories, id: &str) -> Vec<OwnedRoomId> {
        categories.home.category(id).unwrap().rooms.clone()
    }

    #[test]
    fn a_category_is_created_and_changed_with_its_rooms() {
        let mut categories = RoomCategories::default();
        let teaching = categories.home.add_category("Teaching");
        categories.home.place(&room("seminar"), Some(&teaching));

        // New, empty.
        CategoryEdit {
            space: None,
            category: None,
            name: "Empty".into(),
            rooms: Vec::new(),
        }
        .apply(&mut categories);
        assert_eq!(categories.home.categories[1].name, "Empty");
        assert!(categories.home.categories[1].rooms.is_empty());

        // New, taking a room from another category.
        CategoryEdit {
            space: None,
            category: None,
            name: "Papers".into(),
            rooms: vec![room("paper-a"), room("seminar")],
        }
        .apply(&mut categories);
        let papers = categories.home.categories[2].id.clone();
        assert_eq!(
            rooms_of(&categories, &papers),
            [room("paper-a"), room("seminar")]
        );
        assert!(rooms_of(&categories, &teaching).is_empty());

        // Renamed, one room out and one in; those staying keep their place.
        CategoryEdit {
            space: None,
            category: Some(papers.clone()),
            name: "Drafts".into(),
            rooms: vec![room("paper-b"), room("seminar")],
        }
        .apply(&mut categories);
        let drafts = categories.home.category(&papers).unwrap();
        assert_eq!(drafts.name, "Drafts");
        assert_eq!(drafts.rooms, [room("seminar"), room("paper-b")]);
        assert_eq!(categories.home.category_of(&room("paper-a")), None);
    }
}
