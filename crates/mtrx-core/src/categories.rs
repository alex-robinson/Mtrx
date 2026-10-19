//! How the room list is arranged: the user's own categories per space (and
//! Home), plus built-in sections for favourites, direct chats and low
//! priority rooms.
//!
//! Categories are personal. They are stored as account data, so they sync
//! to the user's devices but nobody else sees them. Favourites and low
//! priority are the standard Matrix room tags, shared with other clients.
//!
//! Changes show at once: [`Session::change_room_categories`] publishes the
//! new arrangement before saving it on the server.

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use futures_util::Stream;
use matrix_sdk::{
    Client,
    event_handler::EventHandlerHandle,
    ruma::{
        OwnedRoomId, RoomId,
        events::{GlobalAccountDataEvent, macros::EventContent},
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, watch};
use tokio_stream::wrappers::WatchStream;
use tracing::warn;

use crate::{Result, RoomKind, RoomSummary, Session, scheduled::random_id};

/// The user's arrangement of the room list, for Home and each space.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RoomCategories {
    #[serde(default)]
    pub home: Layout,
    #[serde(default)]
    pub spaces: BTreeMap<OwnedRoomId, Layout>,
}

/// The arrangement of one space's room list (or Home's).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    /// In display order.
    #[serde(default)]
    pub categories: Vec<Category>,
    /// Sections shown as their heading only.
    #[serde(default)]
    pub collapsed: Vec<Section>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
    /// A room is in at most one category of a layout.
    #[serde(default)]
    pub rooms: Vec<OwnedRoomId>,
    #[serde(default)]
    pub sort: Sort,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    /// Most recently active first.
    #[default]
    Activity,
    /// A–Z.
    Name,
}

/// A section of the room list, in display order.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Invites,
    Favourites,
    /// One of the user's categories, by id.
    Category(String),
    /// Rooms not in a category.
    Rooms,
    /// Direct chats not in a category.
    People,
    LowPriority,
}

/// The standard room tags Mtrx shows as sections.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoomTag {
    #[default]
    None,
    Favourite,
    LowPriority,
}

/// Which built-in sections the room list has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grouping {
    pub favourites: bool,
    /// Direct chats apart from rooms.
    pub people: bool,
    pub low_priority: bool,
    /// Rooms with unread messages first in each section.
    pub unread_first: bool,
}

impl Default for Grouping {
    fn default() -> Self {
        Self {
            favourites: true,
            people: true,
            low_priority: true,
            unread_first: false,
        }
    }
}

impl RoomCategories {
    /// The layout of `space`, or Home's for `None`.
    pub fn layout(&self, space: Option<&RoomId>) -> &Layout {
        static EMPTY: Layout = Layout {
            categories: Vec::new(),
            collapsed: Vec::new(),
        };
        match space {
            Some(space) => self.spaces.get(space).unwrap_or(&EMPTY),
            None => &self.home,
        }
    }

    pub fn layout_mut(&mut self, space: Option<&RoomId>) -> &mut Layout {
        match space {
            Some(space) => self.spaces.entry(space.to_owned()).or_default(),
            None => &mut self.home,
        }
    }
}

impl Layout {
    /// Add an empty category at the end; returns its id.
    pub fn add_category(&mut self, name: &str) -> String {
        let id = random_id();
        self.categories.push(Category {
            id: id.clone(),
            name: name.trim().to_owned(),
            rooms: Vec::new(),
            sort: Sort::default(),
        });
        id
    }

    pub fn category(&self, id: &str) -> Option<&Category> {
        self.categories.iter().find(|c| c.id == id)
    }

    pub fn category_mut(&mut self, id: &str) -> Option<&mut Category> {
        self.categories.iter_mut().find(|c| c.id == id)
    }

    /// Delete a category; its rooms go back to their default section.
    pub fn remove_category(&mut self, id: &str) {
        self.categories.retain(|c| c.id != id);
        self.collapsed
            .retain(|s| *s != Section::Category(id.to_owned()));
    }

    /// Move a category one place up (`-1`) or down (`1`).
    pub fn move_category(&mut self, id: &str, by: isize) {
        let Some(from) = self.categories.iter().position(|c| c.id == id) else {
            return;
        };
        let to = from
            .saturating_add_signed(by)
            .min(self.categories.len() - 1);
        let category = self.categories.remove(from);
        self.categories.insert(to, category);
    }

    /// Put `room` into a category, or out of all of them for `None`.
    pub fn place(&mut self, room: &RoomId, category: Option<&str>) {
        for c in &mut self.categories {
            c.rooms.retain(|r| r != room);
        }
        if let Some(c) = category.and_then(|id| self.category_mut(id)) {
            c.rooms.push(room.to_owned());
        }
    }

    pub fn category_of(&self, room: &RoomId) -> Option<&Category> {
        self.categories
            .iter()
            .find(|c| c.rooms.iter().any(|r| r == room))
    }

    pub fn is_collapsed(&self, section: &Section) -> bool {
        self.collapsed.contains(section)
    }

    pub fn toggle_collapsed(&mut self, section: &Section) {
        if self.is_collapsed(section) {
            self.collapsed.retain(|s| s != section);
        } else {
            self.collapsed.push(section.clone());
        }
    }

    /// The section `room` belongs in.
    pub fn section_of(&self, room: &RoomSummary, grouping: &Grouping) -> Section {
        if room.invited {
            Section::Invites
        } else if grouping.favourites && room.tag == RoomTag::Favourite {
            Section::Favourites
        } else if grouping.low_priority && room.tag == RoomTag::LowPriority {
            Section::LowPriority
        } else if let Some(category) = self.category_of(&room.id) {
            Section::Category(category.id.clone())
        } else if grouping.people && room.kind == RoomKind::Direct {
            Section::People
        } else {
            Section::Rooms
        }
    }

    /// `rooms` (most recently active first) sorted into sections, in display
    /// order. Every section the grouping has is listed, even when empty.
    pub fn group<'a>(
        &self,
        rooms: impl IntoIterator<Item = &'a RoomSummary>,
        grouping: &Grouping,
    ) -> Vec<(Section, Vec<&'a RoomSummary>)> {
        let mut sections = vec![(Section::Invites, Vec::new())];
        if grouping.favourites {
            sections.push((Section::Favourites, Vec::new()));
        }
        sections.extend(
            self.categories
                .iter()
                .map(|c| (Section::Category(c.id.clone()), Vec::new())),
        );
        sections.push((Section::Rooms, Vec::new()));
        if grouping.people {
            sections.push((Section::People, Vec::new()));
        }
        if grouping.low_priority {
            sections.push((Section::LowPriority, Vec::new()));
        }
        for room in rooms {
            let section = self.section_of(room, grouping);
            if let Some((_, list)) = sections.iter_mut().find(|(s, _)| *s == section) {
                list.push(room);
            }
        }
        for (section, list) in &mut sections {
            let by_name = matches!(section, Section::Category(id)
                if self.category(id).is_some_and(|c| c.sort == Sort::Name));
            if by_name {
                list.sort_by_cached_key(|r| r.name.to_lowercase());
            }
            if grouping.unread_first {
                list.sort_by_key(|r| r.unread_messages == 0);
            }
        }
        sections
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, EventContent)]
#[ruma_event(type = "org.mtrx.room_categories", kind = GlobalAccountData)]
struct RoomCategoriesEventContent {
    #[serde(flatten)]
    categories: RoomCategories,
}

/// The session's room categories: published at once on change, saved in
/// the background, and updated from other devices through sync.
pub(crate) struct Categories {
    client: Client,
    value: Arc<watch::Sender<RoomCategories>>,
    /// Changes not yet saved; synced values are stale meanwhile.
    unsaved: Arc<AtomicUsize>,
    /// Saves one at a time, so the last change is saved last.
    save: Mutex<()>,
    handler: EventHandlerHandle,
}

impl Categories {
    pub(crate) async fn load(client: Client) -> Self {
        let value = Arc::new(watch::Sender::new(stored(&client).await));
        let unsaved = Arc::new(AtomicUsize::new(0));
        let handler = client.add_event_handler({
            let (value, unsaved) = (value.clone(), unsaved.clone());
            move |event: GlobalAccountDataEvent<RoomCategoriesEventContent>| async move {
                if unsaved.load(Ordering::SeqCst) == 0 {
                    value.send_if_modified(|current| {
                        let changed = *current != event.content.categories;
                        *current = event.content.categories;
                        changed
                    });
                }
            }
        });
        Self {
            client,
            value,
            unsaved,
            save: Mutex::new(()),
            handler,
        }
    }

    async fn change(&self, change: impl FnOnce(&mut RoomCategories)) -> Result<()> {
        self.unsaved.fetch_add(1, Ordering::SeqCst);
        self.value.send_modify(change);
        let saved = {
            let _guard = self.save.lock().await;
            let categories = self.value.borrow().clone();
            self.client
                .account()
                .set_account_data(RoomCategoriesEventContent { categories })
                .await
        };
        self.unsaved.fetch_sub(1, Ordering::SeqCst);
        if let Err(e) = saved {
            // Back to what the server has.
            self.value.send_replace(stored(&self.client).await);
            return Err(e.into());
        }
        Ok(())
    }
}

impl Drop for Categories {
    fn drop(&mut self) {
        self.client.remove_event_handler(self.handler.clone());
    }
}

async fn stored(client: &Client) -> RoomCategories {
    let raw = match client
        .account()
        .account_data::<RoomCategoriesEventContent>()
        .await
    {
        Ok(raw) => raw,
        Err(e) => {
            warn!("could not read room categories: {e}");
            return RoomCategories::default();
        }
    };
    match raw.map(|raw| raw.deserialize()).transpose() {
        Ok(content) => content.map(|c| c.categories).unwrap_or_default(),
        Err(e) => {
            warn!("could not read room categories: {e}");
            RoomCategories::default()
        }
    }
}

impl Session {
    pub fn room_categories(&self) -> RoomCategories {
        self.categories.value.borrow().clone()
    }

    /// The current categories, then every change, from this device or others.
    pub fn room_categories_updates(&self) -> impl Stream<Item = RoomCategories> + use<> {
        WatchStream::new(self.categories.value.subscribe())
    }

    /// Change the categories. The change is published at once and then saved
    /// on the server; if saving fails, the server's version is restored.
    pub async fn change_room_categories(
        &self,
        change: impl FnOnce(&mut RoomCategories),
    ) -> Result<()> {
        self.categories.change(change).await
    }

    /// Set the room's favourite or low priority tag, or remove both.
    pub async fn set_room_tag(&self, room: &RoomId, tag: RoomTag) -> Result<()> {
        let room = self.room(room)?;
        match tag {
            RoomTag::Favourite => room.set_is_favourite(true, None).await?,
            RoomTag::LowPriority => room.set_is_low_priority(true, None).await?,
            RoomTag::None => {
                if room.is_favourite() {
                    room.set_is_favourite(false, None).await?;
                }
                if room.is_low_priority() {
                    room.set_is_low_priority(false, None).await?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use matrix_sdk::ruma::owned_room_id;

    use super::*;

    fn room(id: &str, name: &str, kind: RoomKind) -> RoomSummary {
        RoomSummary {
            id: OwnedRoomId::try_from(id).unwrap(),
            name: name.into(),
            avatar: None,
            topic: None,
            kind,
            invited: false,
            encrypted: false,
            unread_messages: 0,
            unread_mentions: 0,
            unread_notifications: 0,
            last_activity: None,
            parents: Vec::new(),
            tag: RoomTag::None,
            direct_user: None,
        }
    }

    fn names(sections: &[(Section, Vec<&RoomSummary>)], section: &Section) -> Vec<String> {
        sections
            .iter()
            .find(|(s, _)| s == section)
            .map(|(_, rooms)| rooms.iter().map(|r| r.name.clone()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn rooms_go_to_tags_then_categories_then_their_kind() {
        let mut rooms = vec![
            room("!a:x", "zeta", RoomKind::Room),
            room("!b:x", "alpha", RoomKind::Room),
            room("!c:x", "bob", RoomKind::Direct),
            room("!d:x", "fav", RoomKind::Room),
            room("!e:x", "later", RoomKind::Room),
            room("!f:x", "plain", RoomKind::Room),
        ];
        rooms[3].tag = RoomTag::Favourite;
        rooms[4].tag = RoomTag::LowPriority;
        let mut layout = Layout::default();
        let work = layout.add_category("Work");
        for id in ["!a:x", "!b:x", "!c:x", "!d:x"] {
            layout.place(&OwnedRoomId::try_from(id).unwrap(), Some(&work));
        }
        let grouping = Grouping::default();
        let sections = layout.group(&rooms, &grouping);
        let order: Vec<_> = sections.iter().map(|(s, _)| s.clone()).collect();
        assert_eq!(
            order,
            [
                Section::Invites,
                Section::Favourites,
                Section::Category(work.clone()),
                Section::Rooms,
                Section::People,
                Section::LowPriority,
            ]
        );
        let category = Section::Category(work.clone());
        assert_eq!(names(&sections, &category), ["zeta", "alpha", "bob"]);
        assert_eq!(names(&sections, &Section::Favourites), ["fav"]);
        assert_eq!(names(&sections, &Section::LowPriority), ["later"]);
        assert_eq!(names(&sections, &Section::Rooms), ["plain"]);

        layout.category_mut(&work).unwrap().sort = Sort::Name;
        let sections = layout.group(&rooms, &grouping);
        assert_eq!(names(&sections, &category), ["alpha", "bob", "zeta"]);

        // Without the built-in sections, tags and kinds are ignored.
        let plain = Grouping {
            favourites: false,
            people: false,
            low_priority: false,
            unread_first: false,
        };
        let sections = layout.group(&rooms, &plain);
        assert_eq!(names(&sections, &category), ["alpha", "bob", "fav", "zeta"]);
        assert_eq!(names(&sections, &Section::Rooms), ["later", "plain"]);
    }

    #[test]
    fn unread_first_and_category_changes() {
        let mut rooms = vec![
            room("!a:x", "quiet", RoomKind::Room),
            room("!b:x", "busy", RoomKind::Room),
        ];
        rooms[1].unread_messages = 3;
        let grouping = Grouping {
            unread_first: true,
            ..Grouping::default()
        };
        let mut layout = Layout::default();
        assert_eq!(
            names(&layout.group(&rooms, &grouping), &Section::Rooms),
            ["busy", "quiet"]
        );

        let a = layout.add_category("A");
        let b = layout.add_category("B");
        let room = owned_room_id!("!a:x");
        layout.place(&room, Some(&a));
        layout.place(&room, Some(&b));
        assert_eq!(
            layout.category_of(&room).map(|c| c.id.clone()),
            Some(b.clone())
        );
        layout.move_category(&b, -1);
        assert_eq!(layout.categories[0].id, b);
        layout.move_category(&b, -1);
        assert_eq!(layout.categories[0].id, b);
        layout.toggle_collapsed(&Section::Category(b.clone()));
        layout.remove_category(&b);
        assert!(layout.category_of(&room).is_none());
        assert!(layout.collapsed.is_empty());
    }

    #[test]
    fn stored_format() {
        let mut categories = RoomCategories::default();
        let id = categories
            .layout_mut(Some(&owned_room_id!("!space:x")))
            .add_category("Projects");
        categories.home.collapsed.push(Section::People);
        categories
            .home
            .collapsed
            .push(Section::Category(id.clone()));
        let json = serde_json::to_value(RoomCategoriesEventContent {
            categories: categories.clone(),
        })
        .unwrap();
        assert_eq!(
            json["home"]["collapsed"],
            serde_json::json!(["people", { "category": id }])
        );
        let back: RoomCategoriesEventContent = serde_json::from_value(json).unwrap();
        assert_eq!(back.categories, categories);
        // Older or partial content still reads.
        let empty: RoomCategoriesEventContent = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.categories, RoomCategories::default());
    }
}
