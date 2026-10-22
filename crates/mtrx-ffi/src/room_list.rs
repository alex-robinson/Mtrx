//! The room list as the apps get it: the core's list with the rooms by
//! space, which needs the user's categories and the members of each space.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use futures_util::StreamExt;
use mtrx_core::{self as core, OwnedRoomId, OwnedUserId};
use tokio::task::JoinSet;
use tracing::warn;

use crate::{RoomListListener, RoomListState};

/// Call `listener` with the room list, now and on every change, until
/// cancelled.
pub(crate) async fn follow(
    session: Arc<core::Session>,
    list: core::RoomList,
    listener: Arc<dyn RoomListListener>,
) {
    let mut states = Box::pin(list.updates());
    let mut category_updates = Box::pin(session.room_categories_updates());
    let mut categories = session.room_categories();
    let mut members = SpaceMembers::default();
    let mut loads = JoinSet::new();
    let Some(mut state) = states.next().await else {
        return;
    };
    let mut sent: Option<RoomListState> = None;
    loop {
        for (space, count) in members.due(&state.rooms) {
            let session = session.clone();
            loads.spawn(async move {
                let loaded = session.joined_member_ids(&space).await;
                (space, count, loaded)
            });
        }
        let by_space = core::RoomsBySpace::new(&state.rooms, &members.known, &categories);
        let new = RoomListState::new(&state, by_space);
        if sent.as_ref() != Some(&new) {
            listener.on_update(new.clone());
            sent = Some(new);
        }

        tokio::select! {
            next = states.next() => match next {
                Some(next) => state = next,
                None => return,
            },
            Some(next) = category_updates.next() => categories = next,
            Some(Ok((space, count, loaded))) = loads.join_next() => match loaded {
                Ok(ids) => members.loaded(space, count, ids),
                Err(e) => warn!(%space, "could not load the space's members: {e}"),
            },
        }
    }
}

/// The joined members of the top-level spaces, for the direct chats they
/// list. Loaded when a space appears and again when its number of joined
/// members changes; a failed load is tried again then.
#[derive(Default)]
struct SpaceMembers {
    known: HashMap<OwnedRoomId, HashSet<OwnedUserId>>,
    /// The number of joined members each space had when its members were
    /// last asked for.
    asked: HashMap<OwnedRoomId, u64>,
}

impl SpaceMembers {
    /// Spaces whose members to load now, with their number of members.
    fn due(&mut self, rooms: &[core::RoomSummary]) -> Vec<(OwnedRoomId, u64)> {
        let spaces = core::top_level_spaces(rooms);
        self.asked
            .retain(|id, _| spaces.iter().any(|space| &space.id == id));
        self.known
            .retain(|id, _| spaces.iter().any(|space| &space.id == id));
        spaces
            .into_iter()
            .filter(|space| {
                let count = space.joined_members;
                self.asked.insert(space.id.clone(), count) != Some(count)
            })
            .map(|space| (space.id.clone(), space.joined_members))
            .collect()
    }

    /// Members loaded for a space that had `count` joined members; dropped
    /// if a newer load was started since.
    fn loaded(&mut self, space: OwnedRoomId, count: u64, ids: HashSet<OwnedUserId>) {
        if self.asked.get(&space) == Some(&count) {
            self.known.insert(space, ids);
        }
    }
}

#[cfg(test)]
mod tests {
    use mtrx_core::{RoomKind, RoomTag};

    use super::*;

    fn space(id: &str, joined_members: u64) -> core::RoomSummary {
        core::RoomSummary {
            id: id.try_into().unwrap(),
            name: id.into(),
            avatar: None,
            topic: None,
            kind: RoomKind::Space,
            invited: false,
            encrypted: false,
            public: false,
            unread_messages: 0,
            unread_mentions: 0,
            unread_notifications: 0,
            last_activity: None,
            parents: Vec::new(),
            tag: RoomTag::None,
            direct_user: None,
            alias: None,
            joined_members,
        }
    }

    #[test]
    fn members_load_again_when_their_number_changes() {
        let mut members = SpaceMembers::default();
        let id = |s: &str| OwnedRoomId::try_from(s).unwrap();
        let bob: HashSet<OwnedUserId> = HashSet::from(["@bob:x".try_into().unwrap()]);

        let rooms = [space("!a:x", 2)];
        assert_eq!(members.due(&rooms), [(id("!a:x"), 2)]);
        assert!(members.due(&rooms).is_empty(), "asked once");

        // A load for an older number is stale.
        let rooms = [space("!a:x", 3)];
        assert_eq!(members.due(&rooms), [(id("!a:x"), 3)]);
        members.loaded(id("!a:x"), 2, bob.clone());
        assert!(members.known.is_empty());
        members.loaded(id("!a:x"), 3, bob.clone());
        assert_eq!(members.known[&id("!a:x")], bob);

        // Gone spaces are forgotten.
        assert!(members.due(&[]).is_empty());
        assert!(members.known.is_empty() && members.asked.is_empty());
    }
}
