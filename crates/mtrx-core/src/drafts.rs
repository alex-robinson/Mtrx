//! Drafts: what one wrote in a room's composer and did not send, with the
//! message it answers, kept on this device (the SDK's store) until sent or
//! cleared. Matrix has no way to share them between devices.

use std::collections::BTreeSet;

use futures_util::Stream;
use matrix_sdk::{
    Client, ComposerDraft, ComposerDraftType,
    ruma::{OwnedEventId, OwnedRoomId, RoomId},
};
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;
use tracing::warn;

use crate::{Result, Session};

/// A room's unsent message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    pub text: String,
    /// The message it answers.
    pub reply_to: Option<OwnedEventId>,
}

impl Draft {
    /// Nothing to keep.
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.reply_to.is_none()
    }
}

/// The rooms with a draft, for the room list to mark.
pub(crate) struct Drafts {
    rooms: watch::Sender<BTreeSet<OwnedRoomId>>,
}

impl Drafts {
    /// The rooms of `client` with a draft in its store.
    pub(crate) async fn load(client: &Client) -> Self {
        let mut rooms = BTreeSet::new();
        for room in client.rooms() {
            match room.load_composer_draft(None).await {
                Ok(Some(_)) => {
                    rooms.insert(room.room_id().to_owned());
                }
                Ok(None) => {}
                Err(e) => warn!(room = %room.room_id(), "could not read the draft: {e}"),
            }
        }
        Self {
            rooms: watch::Sender::new(rooms),
        }
    }

    fn set(&self, room_id: &RoomId, has_draft: bool) {
        self.rooms.send_if_modified(|rooms| {
            if has_draft {
                rooms.insert(room_id.to_owned())
            } else {
                rooms.remove(room_id)
            }
        });
    }
}

impl Session {
    /// The draft left in room `room_id`, if any.
    pub async fn draft(&self, room_id: &RoomId) -> Result<Option<Draft>> {
        let room = self.room(room_id)?;
        let Some(draft) = room.load_composer_draft(None).await? else {
            return Ok(None);
        };
        let reply_to = match draft.draft_type {
            ComposerDraftType::Reply { event_id } => Some(event_id),
            ComposerDraftType::NewMessage | ComposerDraftType::Edit { .. } => None,
        };
        Ok(Some(Draft {
            text: draft.plain_text,
            reply_to,
        }))
    }

    /// Keep `draft` for room `room_id`; an empty one clears it.
    pub async fn save_draft(&self, room_id: &RoomId, draft: &Draft) -> Result<()> {
        let room = self.room(room_id)?;
        if draft.is_empty() {
            room.clear_composer_draft(None).await?;
        } else {
            let draft_type = match &draft.reply_to {
                Some(event_id) => ComposerDraftType::Reply {
                    event_id: event_id.clone(),
                },
                None => ComposerDraftType::NewMessage,
            };
            room.save_composer_draft(
                ComposerDraft {
                    plain_text: draft.text.clone(),
                    html_text: None,
                    draft_type,
                    attachments: Vec::new(),
                },
                None,
            )
            .await?;
        }
        self.drafts.set(room_id, !draft.is_empty());
        Ok(())
    }

    /// The rooms with a draft now, then after every change.
    pub fn rooms_with_drafts_updates(&self) -> impl Stream<Item = BTreeSet<OwnedRoomId>> + use<> {
        WatchStream::new(self.drafts.rooms.subscribe())
    }
}
