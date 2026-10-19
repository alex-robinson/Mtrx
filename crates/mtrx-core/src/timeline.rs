//! Live room timeline: messages, state changes, sending.

use std::{sync::Arc, time::SystemTime};

use eyeball_im::Vector;
use futures_util::{Stream, StreamExt, pin_mut};
use matrix_sdk::ruma::{
    OwnedEventId, OwnedRoomId, OwnedTransactionId, OwnedUserId,
    api::client::receipt::create_receipt::v3::ReceiptType,
    events::room::message::{MessageType, RoomMessageEventContent},
};
use matrix_sdk::send_queue::RoomSendQueueUpdate;
use matrix_sdk_ui::timeline::{
    EventSendState, EventTimelineItem, MembershipChange as SdkMembershipChange, MsgLikeKind,
    RoomExt, Timeline, TimelineDetails, TimelineItem, TimelineItemContent, TimelineItemKind,
    VirtualTimelineItem,
};
use tokio::{
    sync::{Mutex, broadcast, watch},
    task::JoinHandle,
};
use tokio_stream::wrappers::WatchStream;

use crate::{Error, Result, Session, system_time};

/// One row of a timeline.
// Nearly all entries are events, so boxing would only add indirection.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum TimelineEntry {
    Event(Event),
    DayDivider(SystemTime),
    /// Everything below was not read yet.
    ReadMarker,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    /// Stable across the local echo becoming the remote event.
    pub id: String,
    pub event_id: Option<OwnedEventId>,
    pub sender: OwnedUserId,
    pub sender_name: Option<String>,
    pub timestamp: SystemTime,
    pub own: bool,
    pub content: EventContent,
    pub send_state: SendState,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SendState {
    Sent,
    Sending,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum EventContent {
    Message(Message),
    Redacted,
    /// Encrypted and no key (yet). Keys may still arrive from backup or other devices.
    UnableToDecrypt,
    Membership {
        user: OwnedUserId,
        name: Option<String>,
        change: MembershipChange,
    },
    DisplayNameChange {
        user: OwnedUserId,
        new_name: Option<String>,
    },
    /// A state event we do not render in detail, e.g. `m.room.topic`.
    OtherState {
        event_type: String,
    },
    /// A message-like event we do not render, e.g. a poll or sticker.
    Other {
        description: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub kind: MessageKind,
    /// Plain-text body (fallback for every kind).
    pub body: String,
    /// `org.matrix.custom.html` body, if any. Math arrives here as
    /// `data-mx-maths` elements.
    pub html: Option<String>,
    pub edited: bool,
    pub in_reply_to: Option<OwnedEventId>,
    /// Reaction key and how many users reacted with it.
    pub reactions: Vec<(String, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Text,
    Notice,
    Emote,
    Image,
    Video,
    Audio,
    File,
    Location,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MembershipChange {
    Joined,
    Left,
    Invited,
    InvitationAccepted,
    InvitationRejected,
    InvitationRevoked,
    Kicked,
    Banned,
    Unbanned,
    Knocked,
    Other,
}

/// A room's timeline, kept up to date while it is alive.
pub struct RoomTimeline {
    room_id: OwnedRoomId,
    timeline: Arc<Timeline>,
    send_lock: Arc<Mutex<()>>,
    entries: watch::Receiver<Vec<TimelineEntry>>,
    task: JoinHandle<()>,
}

impl Drop for RoomTimeline {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl RoomTimeline {
    pub fn room_id(&self) -> &OwnedRoomId {
        &self.room_id
    }

    /// Oldest first.
    pub fn current(&self) -> Vec<TimelineEntry> {
        self.entries.borrow().clone()
    }

    /// The current entries, then every change.
    pub fn updates(&self) -> impl Stream<Item = Vec<TimelineEntry>> + use<> {
        WatchStream::new(self.entries.clone())
    }

    /// Load up to `events` older events. Returns `true` once the start of the
    /// room is reached.
    pub async fn paginate_back(&self, events: u16) -> Result<bool> {
        self.timeline
            .paginate_backwards(events)
            .await
            .map_err(timeline_error)
    }

    /// Queue a message written in Markdown. Its local echo shows as
    /// [`SendState::Sending`] until the server confirms; await
    /// [`PendingSend::sent`] to wait for that.
    pub async fn send_markdown(&self, text: &str) -> Result<PendingSend> {
        let content = RoomMessageEventContent::text_markdown(text);
        // The SDK's send handle does not expose the transaction ID, so we take
        // it from the first new local echo after subscribing. The lock makes
        // sure no other send of this session enqueues in between.
        let _guard = self.send_lock.lock().await;
        let (_, mut updates) = self
            .timeline
            .room()
            .send_queue()
            .subscribe()
            .await
            .map_err(timeline_error)?;
        self.timeline
            .send(content.into())
            .await
            .map_err(timeline_error)?;
        loop {
            match updates.try_recv() {
                Ok(RoomSendQueueUpdate::NewLocalEvent(echo)) => {
                    return Ok(PendingSend {
                        transaction_id: echo.transaction_id,
                        updates,
                    });
                }
                Ok(_) => {}
                Err(e) => return Err(Error::Timeline(format!("no local echo after send: {e}"))),
            }
        }
    }

    /// Mark everything up to the latest event as read.
    pub async fn mark_read(&self) -> Result<()> {
        self.timeline.mark_as_read(ReceiptType::Read).await?;
        Ok(())
    }
}

/// A message in the send queue.
pub struct PendingSend {
    transaction_id: OwnedTransactionId,
    updates: broadcast::Receiver<RoomSendQueueUpdate>,
}

impl PendingSend {
    /// Wait until the server accepted the message.
    pub async fn sent(mut self) -> Result<OwnedEventId> {
        loop {
            match self.updates.recv().await {
                Ok(RoomSendQueueUpdate::SentEvent {
                    transaction_id,
                    event_id,
                }) if transaction_id == self.transaction_id => {
                    return Ok(event_id);
                }
                Ok(RoomSendQueueUpdate::SendError {
                    transaction_id,
                    error,
                    ..
                }) if transaction_id == self.transaction_id => {
                    return Err(Error::Timeline(format!("sending failed: {error}")));
                }
                Ok(RoomSendQueueUpdate::CancelledLocalEvent { transaction_id })
                    if transaction_id == self.transaction_id =>
                {
                    return Err(Error::Timeline("sending was cancelled".into()));
                }
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(Error::Timeline("send queue closed".into()));
                }
            }
        }
    }
}

impl Session {
    pub async fn timeline(&self, room_id: &OwnedRoomId) -> Result<RoomTimeline> {
        let timeline = Arc::new(
            self.room(room_id)?
                .timeline()
                .await
                .map_err(timeline_error)?,
        );
        let (items, stream) = timeline.subscribe().await;
        let (tx, entries) = watch::channel(convert_all(&items));
        let task = tokio::spawn(async move {
            let mut items = items;
            pin_mut!(stream);
            while let Some(diffs) = stream.next().await {
                diffs.into_iter().for_each(|d| d.apply(&mut items));
                tx.send_replace(convert_all(&items));
            }
        });
        Ok(RoomTimeline {
            room_id: room_id.clone(),
            timeline,
            send_lock: self.send_lock.clone(),
            entries,
            task,
        })
    }
}

fn timeline_error(e: impl std::fmt::Display) -> Error {
    Error::Timeline(e.to_string())
}

fn convert_all(items: &Vector<Arc<TimelineItem>>) -> Vec<TimelineEntry> {
    items.iter().filter_map(|item| convert(item)).collect()
}

fn convert(item: &TimelineItem) -> Option<TimelineEntry> {
    match item.kind() {
        TimelineItemKind::Event(event) => Some(TimelineEntry::Event(convert_event(
            event,
            &item.unique_id().0,
        )?)),
        TimelineItemKind::Virtual(VirtualTimelineItem::DateDivider(ts)) => {
            Some(TimelineEntry::DayDivider(system_time(*ts)))
        }
        TimelineItemKind::Virtual(VirtualTimelineItem::ReadMarker) => {
            Some(TimelineEntry::ReadMarker)
        }
        TimelineItemKind::Virtual(_) => None,
    }
}

fn convert_event(event: &EventTimelineItem, id: &str) -> Option<Event> {
    let content = match event.content() {
        TimelineItemContent::MsgLike(msg_like) => match &msg_like.kind {
            MsgLikeKind::Message(message) => EventContent::Message(Message {
                kind: message_kind(message.msgtype()),
                body: message.body().to_owned(),
                html: html_body(message.msgtype()),
                edited: message.is_edited(),
                in_reply_to: msg_like.in_reply_to.as_ref().map(|r| r.event_id.clone()),
                reactions: msg_like
                    .reactions
                    .iter()
                    .map(|(key, senders)| (key.clone(), senders.len()))
                    .collect(),
            }),
            MsgLikeKind::Redacted => EventContent::Redacted,
            MsgLikeKind::UnableToDecrypt(_) => EventContent::UnableToDecrypt,
            MsgLikeKind::Sticker(_) => EventContent::Other {
                description: "sticker".into(),
            },
            MsgLikeKind::Poll(_) => EventContent::Other {
                description: "poll".into(),
            },
            MsgLikeKind::LiveLocation(_) => EventContent::Other {
                description: "live location".into(),
            },
            MsgLikeKind::Other(other) => EventContent::Other {
                description: other.event_type().to_string(),
            },
        },
        TimelineItemContent::MembershipChange(change) => EventContent::Membership {
            user: change.user_id().to_owned(),
            name: change.display_name(),
            change: change.change().map(membership_change)?,
        },
        TimelineItemContent::ProfileChange(change) => EventContent::DisplayNameChange {
            user: change.user_id().to_owned(),
            new_name: change.displayname_change()?.new.clone(),
        },
        TimelineItemContent::OtherState(state) => EventContent::OtherState {
            event_type: state.content().event_type().to_string(),
        },
        _ => return None,
    };

    let send_state = match event.send_state() {
        None | Some(EventSendState::Sent { .. }) => SendState::Sent,
        Some(EventSendState::NotSentYet { .. }) => SendState::Sending,
        Some(EventSendState::SendingFailed { error, .. }) => SendState::Failed(error.to_string()),
    };
    let sender_name = match event.sender_profile() {
        TimelineDetails::Ready(profile) => profile.display_name.clone(),
        _ => None,
    };

    Some(Event {
        id: id.to_owned(),
        event_id: event.event_id().map(ToOwned::to_owned),
        sender: event.sender().to_owned(),
        sender_name,
        timestamp: system_time(event.timestamp()),
        own: event.is_own(),
        content,
        send_state,
    })
}

fn message_kind(msgtype: &MessageType) -> MessageKind {
    match msgtype {
        MessageType::Text(_) => MessageKind::Text,
        MessageType::Notice(_) => MessageKind::Notice,
        MessageType::Emote(_) => MessageKind::Emote,
        MessageType::Image(_) => MessageKind::Image,
        MessageType::Video(_) => MessageKind::Video,
        MessageType::Audio(_) => MessageKind::Audio,
        MessageType::File(_) => MessageKind::File,
        MessageType::Location(_) => MessageKind::Location,
        _ => MessageKind::Other,
    }
}

fn html_body(msgtype: &MessageType) -> Option<String> {
    let formatted = match msgtype {
        MessageType::Text(c) => c.formatted.as_ref(),
        MessageType::Notice(c) => c.formatted.as_ref(),
        MessageType::Emote(c) => c.formatted.as_ref(),
        _ => None,
    }?;
    (formatted.format.as_str() == "org.matrix.custom.html").then(|| formatted.body.clone())
}

fn membership_change(change: SdkMembershipChange) -> MembershipChange {
    match change {
        SdkMembershipChange::Joined => MembershipChange::Joined,
        SdkMembershipChange::Left => MembershipChange::Left,
        SdkMembershipChange::Invited => MembershipChange::Invited,
        SdkMembershipChange::InvitationAccepted => MembershipChange::InvitationAccepted,
        SdkMembershipChange::InvitationRejected => MembershipChange::InvitationRejected,
        SdkMembershipChange::InvitationRevoked => MembershipChange::InvitationRevoked,
        SdkMembershipChange::Kicked => MembershipChange::Kicked,
        SdkMembershipChange::Banned | SdkMembershipChange::KickedAndBanned => {
            MembershipChange::Banned
        }
        SdkMembershipChange::Unbanned => MembershipChange::Unbanned,
        SdkMembershipChange::Knocked => MembershipChange::Knocked,
        _ => MembershipChange::Other,
    }
}
