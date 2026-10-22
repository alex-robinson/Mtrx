//! Arranging the room list: the user's categories, favourites and low
//! priority, by dragging rooms between sections or through menus.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use iced::{Point, Subscription, Task, event, mouse};
use mtrx_core::{
    Grouping, OwnedRoomId, OwnedUserId, RoomCategories, RoomSummary, RoomTag, Section, Session,
    Sort,
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
    /// A space's joined members.
    SpaceMembers(OwnedRoomId, Result<HashSet<OwnedUserId>, String>),
    /// The cursor entered (`true`) or left a room row.
    Row(OwnedRoomId, bool),
    /// The cursor entered or left the pinned Notes row, which is not dragged.
    NotesRow(bool),
    /// The left mouse button, anywhere.
    Pressed,
    Released,
    Moved(Point),
    /// While dragging, the cursor entered (`true`) or left a section.
    Target(Section, bool),
    ToggleCollapsed(Section),
    RoomMenu(Option<OwnedRoomId>),
    CategoryMenu(Option<String>),
    MoveTo(OwnedRoomId, Section),
    /// Favourite (the star), low priority or neither, keeping the room's
    /// category.
    Tag(OwnedRoomId, RoomTag),
    /// Name a new category, then put the room in it if there is one.
    NewCategory(Option<OwnedRoomId>),
    Rename(String),
    Name(String),
    SaveName,
    Delete(String),
    MoveCategory(String, isize),
    Sort(String, Sort),
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

/// A category being named.
pub struct Naming {
    /// `None` for a new one.
    pub category: Option<String>,
    pub name: String,
    /// Put into the new category.
    room: Option<OwnedRoomId>,
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
    /// Joined members of spaces, for the direct chats listed in them.
    space_members: HashMap<OwnedRoomId, HashSet<OwnedUserId>>,
    /// The room row under the cursor.
    pub hovered: Option<OwnedRoomId>,
    pub notes_hovered: bool,
    pub drag: Option<Drag>,
    /// The section a drag would drop into.
    pub target: Option<Section>,
    pub room_menu: Option<OwnedRoomId>,
    pub category_menu: Option<String>,
    pub naming: Option<Naming>,
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

    /// Members of `space` known so far.
    pub fn members(&self, space: &OwnedRoomId) -> Option<&HashSet<OwnedUserId>> {
        self.space_members.get(space)
    }

    /// Load the members of a space shown, for its direct chats.
    pub fn space_selected(&self, session: &Arc<Session>, space: OwnedRoomId) -> Task<app::Message> {
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
            move |result| app::Message::Arrange(Message::SpaceMembers(space.clone(), result)),
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
            || self.naming.is_some();
        self.drag = None;
        self.target = None;
        self.room_menu = None;
        self.category_menu = None;
        self.naming = None;
        open
    }

    pub fn update(&mut self, message: Message, context: Context<'_>) -> Task<app::Message> {
        let space = context.space.cloned();
        match message {
            Message::Categories(categories) => self.categories = categories,
            Message::SpaceMembers(space, Ok(joined)) => {
                self.space_members.insert(space, joined);
            }
            Message::SpaceMembers(space, Err(e)) => {
                warn!(%space, "could not load the space's members: {e}");
            }
            Message::Row(room, true) => self.hovered = Some(room),
            Message::Row(room, false) => {
                if self.hovered.as_ref() == Some(&room) {
                    self.hovered = None;
                }
            }
            Message::NotesRow(hovered) => self.notes_hovered = hovered,
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
            Message::NewCategory(room) => {
                self.room_menu = None;
                self.naming = Some(Naming {
                    category: None,
                    name: String::new(),
                    room,
                });
                return iced::widget::operation::focus(NAME_INPUT);
            }
            Message::Rename(id) => {
                self.category_menu = None;
                let layout = self.categories.layout(space.as_deref());
                let name = layout.category(&id).map(|c| c.name.clone());
                self.naming = Some(Naming {
                    category: Some(id),
                    name: name.unwrap_or_default(),
                    room: None,
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
                    let layout = c.layout_mut(space.as_deref());
                    match &naming.category {
                        Some(id) => {
                            if let Some(category) = layout.category_mut(id) {
                                category.name = name;
                            }
                        }
                        None => {
                            let id = layout.add_category(&name);
                            if let Some(room) = &naming.room {
                                layout.place(room, Some(&id));
                            }
                        }
                    }
                });
            }
            Message::Delete(id) => {
                self.category_menu = None;
                return self.change(context.session, move |c| {
                    c.layout_mut(space.as_deref()).remove_category(&id)
                });
            }
            Message::MoveCategory(id, by) => {
                return self.change(context.session, move |c| {
                    c.layout_mut(space.as_deref()).move_category(&id, by)
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
        // A tag that decides the section goes when moving elsewhere.
        let tag = match section {
            Section::Favourites => RoomTag::Favourite,
            Section::LowPriority => RoomTag::LowPriority,
            _ => match summary.tag {
                RoomTag::Favourite if grouping.favourites => RoomTag::None,
                RoomTag::LowPriority if grouping.low_priority => RoomTag::None,
                tag => tag,
            },
        };
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

    /// The cursor, while a room may be dragged.
    pub fn subscription(&self) -> Option<Subscription<app::Message>> {
        self.drag.as_ref()?;
        Some(event::listen_with(|event, _status, _window| match event {
            iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(app::Message::Arrange(Message::Moved(position)))
            }
            _ => None,
        }))
    }
}

fn failed(error: String) -> Task<app::Message> {
    Task::done(app::Message::ArrangeFailed(error))
}
