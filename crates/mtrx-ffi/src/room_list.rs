//! The room list as the apps get it: the core's list with the rooms by
//! space, which needs the user's categories and the members of each space.

use std::sync::Arc;

use futures_util::StreamExt;
use mtrx_core::{self as core, SpaceMembers};
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
        let by_space = core::RoomsBySpace::new(&state.rooms, &members, &categories);
        let notes = session.notes_room();
        let new = RoomListState::new(&state, by_space, &categories, notes.as_ref());
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
