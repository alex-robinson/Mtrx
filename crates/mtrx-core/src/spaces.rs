//! The room list by space, as the desktop shows it: a column of Home and the
//! top-level spaces, and the rooms the selected one lists.

use std::collections::{HashMap, HashSet};

use matrix_sdk::ruma::{OwnedRoomId, OwnedUserId, RoomId};

use crate::{Layout, RoomCategories, RoomKind, RoomSummary};

/// Joined spaces in no joined space, in the order of `rooms` (the list's,
/// most recently active first). Nested spaces show through these.
pub fn top_level_spaces(rooms: &[RoomSummary]) -> Vec<&RoomSummary> {
    let ids: HashSet<&OwnedRoomId> = rooms.iter().map(|r| &r.id).collect();
    rooms
        .iter()
        .filter(|r| r.kind == RoomKind::Space && !r.invited)
        .filter(|r| !r.parents.iter().any(|p| ids.contains(p)))
        .collect()
}

/// Rooms below `space`, through nested spaces too.
pub fn descendants(rooms: &[RoomSummary], space: &RoomId) -> HashSet<OwnedRoomId> {
    let mut children: HashMap<&RoomId, Vec<&OwnedRoomId>> = HashMap::new();
    for room in rooms {
        for parent in &room.parents {
            children.entry(parent).or_default().push(&room.id);
        }
    }
    let mut found = HashSet::new();
    let mut stack = vec![space];
    while let Some(id) = stack.pop() {
        for child in children.get(id).into_iter().flatten() {
            // Space graphs may contain cycles.
            if found.insert((*child).clone()) {
                stack.push(child);
            }
        }
    }
    found
}

/// Whether a room list may show `room`: rooms and direct chats, and spaces
/// only when invited to, to be answered.
fn listed(room: &RoomSummary) -> bool {
    room.kind != RoomKind::Space || room.invited
}

/// What Home lists, in the order of `rooms`: every room and direct chat, and
/// invites to spaces.
pub fn home_rooms(rooms: &[RoomSummary]) -> Vec<&RoomSummary> {
    rooms.iter().filter(|r| listed(r)).collect()
}

/// What a space lists, in the order of `rooms`: everything below it
/// (through nested spaces), direct chats with its joined `members`, and rooms
/// the user put into its categories (`layout`). Spaces are not listed, except
/// invites to them, to be answered.
pub fn space_rooms<'a>(
    rooms: &'a [RoomSummary],
    space: &RoomId,
    members: Option<&HashSet<OwnedUserId>>,
    layout: &Layout,
) -> Vec<&'a RoomSummary> {
    let inside = descendants(rooms, space);
    rooms
        .iter()
        .filter(|r| listed(r))
        .filter(|r| {
            inside.contains(&r.id)
                || r.direct_user
                    .as_ref()
                    .is_some_and(|u| members.is_some_and(|m| m.contains(u)))
                || layout.category_of(&r.id).is_some()
        })
        .collect()
}

/// The room list as the desktop's two columns, for frontends that show them
/// one at a time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomsBySpace {
    /// The [`top_level_spaces`], each with its [`space_rooms`].
    pub spaces: Vec<SpaceRooms>,
    /// Rooms none of them lists, in the order of the list. No direct chats
    /// (they have their own list) and no joined spaces.
    pub outside: Vec<OwnedRoomId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceRooms {
    pub space: OwnedRoomId,
    pub rooms: Vec<OwnedRoomId>,
}

impl RoomsBySpace {
    /// `members` holds the joined members of the spaces known so far (by
    /// space); a space without them lists no direct chats yet.
    pub fn new(
        rooms: &[RoomSummary],
        members: &HashMap<OwnedRoomId, HashSet<OwnedUserId>>,
        categories: &RoomCategories,
    ) -> Self {
        let spaces: Vec<SpaceRooms> = top_level_spaces(rooms)
            .into_iter()
            .map(|space| SpaceRooms {
                space: space.id.clone(),
                rooms: space_rooms(
                    rooms,
                    &space.id,
                    members.get(&space.id),
                    categories.layout(Some(&space.id)),
                )
                .into_iter()
                .map(|r| r.id.clone())
                .collect(),
            })
            .collect();
        let in_spaces: HashSet<&OwnedRoomId> = spaces.iter().flat_map(|s| &s.rooms).collect();
        let outside = rooms
            .iter()
            .filter(|r| listed(r) && r.kind != RoomKind::Direct)
            .filter(|r| !in_spaces.contains(&r.id))
            .map(|r| r.id.clone())
            .collect();
        Self { spaces, outside }
    }
}

#[cfg(test)]
mod tests {
    use matrix_sdk::ruma::{owned_room_id, owned_user_id};

    use super::*;
    use crate::RoomTag;

    fn room(id: &str, kind: RoomKind, parents: &[&str]) -> RoomSummary {
        RoomSummary {
            id: OwnedRoomId::try_from(id).unwrap(),
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
            parents: parents
                .iter()
                .map(|p| OwnedRoomId::try_from(*p).unwrap())
                .collect(),
            tag: RoomTag::None,
            direct_user: None,
            alias: None,
            joined_members: 0,
        }
    }

    fn direct(id: &str, user: &str) -> RoomSummary {
        RoomSummary {
            direct_user: Some(OwnedUserId::try_from(user).unwrap()),
            ..room(id, RoomKind::Direct, &[])
        }
    }

    fn ids(rooms: &[OwnedRoomId]) -> Vec<&str> {
        rooms.iter().map(|r| r.as_str()).collect()
    }

    /// In list order: two top-level spaces, one nested, rooms in and out of
    /// them, direct chats, invites.
    fn list() -> Vec<RoomSummary> {
        let mut invited_room = room("!offer:x", RoomKind::Room, &["!team:x"]);
        invited_room.invited = true;
        let mut invited_space = room("!guild:x", RoomKind::Space, &[]);
        invited_space.invited = true;
        let mut invited_nested = room("!club:x", RoomKind::Space, &["!team:x"]);
        invited_nested.invited = true;
        vec![
            direct("!bob:x", "@bob:x"),
            room("!lab:x", RoomKind::Room, &["!team:x"]),
            room("!other:x", RoomKind::Space, &[]),
            invited_room,
            room("!loose:x", RoomKind::Room, &[]),
            room("!deep:x", RoomKind::Room, &["!sub:x"]),
            room("!sub:x", RoomKind::Space, &["!team:x"]),
            direct("!carol:x", "@carol:x"),
            invited_space,
            invited_nested,
            room("!team:x", RoomKind::Space, &[]),
            room("!both:x", RoomKind::Room, &["!team:x", "!other:x"]),
        ]
    }

    #[test]
    fn spaces_list_their_rooms_in_list_order() {
        let rooms = list();
        let members = HashMap::from([(
            owned_room_id!("!team:x"),
            HashSet::from([owned_user_id!("@bob:x"), owned_user_id!("@me:x")]),
        )]);
        let by_space = RoomsBySpace::new(&rooms, &members, &RoomCategories::default());
        let spaces: Vec<_> = by_space.spaces.iter().map(|s| s.space.as_str()).collect();
        assert_eq!(spaces, ["!other:x", "!team:x"]);
        let team = &by_space.spaces[1];
        assert_eq!(
            ids(&team.rooms),
            [
                "!bob:x", "!lab:x", "!offer:x", "!deep:x", "!club:x", "!both:x"
            ]
        );
        assert_eq!(ids(&by_space.spaces[0].rooms), ["!both:x"]);
        assert_eq!(ids(&by_space.outside), ["!loose:x", "!guild:x"]);
    }

    #[test]
    fn home_lists_rooms_chats_and_invites_but_no_joined_spaces() {
        let rooms = list();
        let home: Vec<_> = home_rooms(&rooms).iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            home,
            [
                "!bob:x", "!lab:x", "!offer:x", "!loose:x", "!deep:x", "!carol:x", "!guild:x",
                "!club:x", "!both:x"
            ]
        );
    }

    #[test]
    fn spaces_without_members_list_no_direct_chats() {
        let rooms = list();
        let by_space = RoomsBySpace::new(&rooms, &HashMap::new(), &RoomCategories::default());
        assert!(!by_space.spaces[1].rooms.contains(&owned_room_id!("!bob:x")));
        // Direct chats are never outside.
        assert!(!by_space.outside.contains(&owned_room_id!("!bob:x")));
    }

    #[test]
    fn rooms_put_into_a_category_of_a_space_are_listed_there() {
        let rooms = list();
        let mut categories = RoomCategories::default();
        let layout = categories.layout_mut(Some(&owned_room_id!("!other:x")));
        let id = layout.add_category("Mine");
        layout.place(&owned_room_id!("!loose:x"), Some(&id));
        layout.place(&owned_room_id!("!carol:x"), Some(&id));
        let by_space = RoomsBySpace::new(&rooms, &HashMap::new(), &categories);
        assert_eq!(
            ids(&by_space.spaces[0].rooms),
            ["!loose:x", "!carol:x", "!both:x"]
        );
        assert_eq!(ids(&by_space.outside), ["!guild:x"]);
    }

    #[test]
    fn cycles_end() {
        let rooms = vec![
            room("!a:x", RoomKind::Space, &["!b:x"]),
            room("!b:x", RoomKind::Space, &["!a:x"]),
            room("!r:x", RoomKind::Room, &["!a:x"]),
        ];
        assert!(top_level_spaces(&rooms).is_empty());
        let below = descendants(&rooms, &owned_room_id!("!a:x"));
        assert_eq!(below.len(), 3);
    }
}
