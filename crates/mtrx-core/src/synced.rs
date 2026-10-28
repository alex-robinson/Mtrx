//! Changes to a room's state that return once sync has brought them back,
//! so what is read next (a card's settings, say) shows them rather than
//! the state before.

use std::{borrow::Borrow, time::Duration};

use matrix_sdk::sync::RoomUpdate;
use matrix_sdk::{
    Room,
    deserialized_responses::RawAnySyncOrStrippedState,
    ruma::{OwnedEventId, events::StateEventContent},
};
use tokio::sync::broadcast::{Receiver, error::RecvError};

use crate::Result;

/// How long a change may take to come back through sync; the server took
/// it either way.
pub(crate) const CONFIRM: Duration = Duration::from_secs(10);

/// Send `content` as the room's state under `key`, returning once it is
/// the room's state here too (or after [`CONFIRM`]).
pub(crate) async fn send_state<C, K>(room: &Room, key: &K, content: C) -> Result<()>
where
    C: StateEventContent,
    C::StateKey: Borrow<K>,
    K: AsRef<str> + ?Sized,
{
    let event_type = content.event_type();
    let updates = room.client().subscribe_to_room_updates(room.room_id());
    let sent = room.send_state_event_for_key(key, content).await?.event_id;
    until(updates, || async {
        let state = room.get_state_event(event_type.clone(), key.as_ref()).await;
        matches!(state, Ok(Some(raw)) if event_id(&raw).as_ref() == Some(&sent))
    })
    .await;
    Ok(())
}

/// Wait, through the room's sync updates, until `done` (or [`CONFIRM`]).
pub(crate) async fn until<F, Fut>(mut updates: Receiver<RoomUpdate>, done: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = bool>,
{
    let confirmed = async {
        loop {
            if done().await {
                return;
            }
            // An update missed while lagging still left its state.
            if let Err(RecvError::Closed) = updates.recv().await {
                return;
            }
        }
    };
    let _ = tokio::time::timeout(CONFIRM, confirmed).await;
}

fn event_id(raw: &RawAnySyncOrStrippedState) -> Option<OwnedEventId> {
    match raw {
        RawAnySyncOrStrippedState::Sync(raw) => raw.get_field("event_id").ok().flatten(),
        // Invited: no event IDs.
        RawAnySyncOrStrippedState::Stripped(_) => None,
    }
}
