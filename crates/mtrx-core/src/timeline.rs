//! Live room timeline: messages, state changes, sending.

use std::{collections::HashSet, sync::Arc, time::SystemTime};

use eyeball_im::Vector;
use futures_util::{Stream, StreamExt, stream};
use matrix_sdk::ruma::{
    EventId, OwnedEventId, OwnedMxcUri, OwnedRoomId, OwnedTransactionId, OwnedUserId,
    api::client::receipt::create_receipt::v3::ReceiptType,
    events::{
        relation::Reply as ReplyRelation,
        room::message::{MessageType, Relation, RoomMessageEventContent},
    },
};
use matrix_sdk::{Room, room::edit::EditedContent, send_queue::RoomSendQueueUpdate};
use matrix_sdk_ui::timeline::{
    EventSendState, EventTimelineItem, InReplyToDetails, MembershipChange as SdkMembershipChange,
    MsgLikeKind, RoomExt, Timeline, TimelineDetails, TimelineEventItemId, TimelineItem,
    TimelineItemContent, TimelineItemKind, VirtualTimelineItem,
};
use tokio::{
    sync::{Mutex, broadcast, watch},
    task::JoinHandle,
};
use tokio_stream::wrappers::{BroadcastStream, WatchStream};
use tracing::warn;

use crate::{Error, Result, Session, system_time};
use mtrx_format::{Document, markdown_to_matrix};

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
    /// Identifies the event for actions (reply, edit, react, delete).
    pub key: EventKey,
    pub event_id: Option<OwnedEventId>,
    pub sender: OwnedUserId,
    pub sender_name: Option<String>,
    pub sender_avatar: Option<OwnedMxcUri>,
    pub timestamp: SystemTime,
    pub own: bool,
    /// We may edit it (our own text message).
    pub editable: bool,
    pub content: EventContent,
    pub send_state: SendState,
    /// Others whose read receipt is on this event: they read up to here.
    pub read_by: Vec<OwnedUserId>,
}

/// Identifies a timeline event for actions on it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EventKey(TimelineEventItemId);

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
    /// The formatted body, or the plain body if there is none. Math keeps its
    /// LaTeX source.
    pub document: Document,
    pub edited: bool,
    pub in_reply_to: Option<Reply>,
    pub reactions: Vec<Reaction>,
}

/// The message a reply answers.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub event_id: OwnedEventId,
    /// `None` until loaded.
    pub sender: Option<OwnedUserId>,
    pub sender_name: Option<String>,
    /// The start of its text; `None` until loaded.
    pub preview: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reaction {
    /// Usually an emoji.
    pub key: String,
    pub count: usize,
    /// We reacted with it.
    pub own: bool,
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

    /// Queue a message written in Markdown, with `$…$` / `$$…$$` math. Its local echo shows as
    /// [`SendState::Sending`] until the server confirms; await
    /// [`PendingSend::sent`] to wait for that.
    pub async fn send_markdown(
        &self,
        text: &str,
        reply_to: Option<&EventId>,
    ) -> Result<PendingSend> {
        send_markdown(self.timeline.room(), &self.send_lock, text, reply_to).await
    }

    /// Replace the text of one of our messages.
    pub async fn edit_markdown(&self, event: &EventKey, text: &str) -> Result<()> {
        let content = markdown_content(text);
        self.timeline
            .edit(&event.0, EditedContent::RoomMessage(content.into()))
            .await
            .map_err(timeline_error)
    }

    /// Add our reaction `key` (an emoji) to the event, or remove it.
    pub async fn toggle_reaction(&self, event: &EventKey, key: &str) -> Result<()> {
        self.timeline
            .toggle_reaction(&event.0, key)
            .await
            .map(|_| ())
            .map_err(timeline_error)
    }

    /// Delete (redact) an event.
    pub async fn redact(&self, event: &EventKey) -> Result<()> {
        self.timeline
            .redact(&event.0, None)
            .await
            .map_err(timeline_error)
    }

    /// Tell others we are typing, or stopped. The SDK sends at most one
    /// notice every few seconds.
    pub async fn typing(&self, typing: bool) -> Result<()> {
        Ok(self.timeline.room().typing_notice(typing).await?)
    }

    /// Names of the others typing, then every change.
    pub fn typing_updates(&self) -> impl Stream<Item = Vec<String>> + use<> {
        let room = self.timeline.room().clone();
        let own = room.own_user_id().to_owned();
        let (guard, receiver) = room.subscribe_to_typing_notifications();
        // The handler stays registered only while `guard` lives.
        stream::unfold(
            (guard, BroadcastStream::new(receiver), room),
            move |(guard, mut users, room)| {
                let own = own.clone();
                async move {
                    let ids = loop {
                        match users.next().await? {
                            Ok(ids) => break ids,
                            Err(_lagged) => continue,
                        }
                    };
                    let mut names = Vec::new();
                    for id in ids.into_iter().filter(|id| *id != own) {
                        let member = room.get_member_no_sync(&id).await.ok().flatten();
                        names.push(
                            member
                                .and_then(|m| m.display_name().map(ToOwned::to_owned))
                                .unwrap_or_else(|| id.localpart().to_owned()),
                        );
                    }
                    Some((names, (guard, users, room)))
                }
            },
        )
    }

    /// Mark everything up to the latest event as read.
    pub async fn mark_read(&self) -> Result<()> {
        self.timeline.mark_as_read(ReceiptType::Read).await?;
        Ok(())
    }
}

/// Queue a Markdown message in `room`'s send queue, which persists it and
/// sends it in the background; timelines show it as a local echo.
///
/// The SDK's send handle does not expose the transaction ID, so we take it
/// from the first new local echo after subscribing. `send_lock`, shared by
/// all sends of a session, makes sure no other send enqueues in between.
pub(crate) async fn send_markdown(
    room: &Room,
    send_lock: &Mutex<()>,
    text: &str,
    reply_to: Option<&EventId>,
) -> Result<PendingSend> {
    let mut content = markdown_content(text);
    if let Some(event_id) = reply_to {
        // No reply fallback in the body: Matrix 1.13 dropped it.
        content.relates_to = Some(Relation::Reply(ReplyRelation::with_event_id(
            event_id.to_owned(),
        )));
    }
    let _guard = send_lock.lock().await;
    let queue = room.send_queue();
    let (_, mut updates) = queue.subscribe().await.map_err(timeline_error)?;
    queue.send(content.into()).await.map_err(timeline_error)?;
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

fn markdown_content(text: &str) -> RoomMessageEventContent {
    let formatted = markdown_to_matrix(text);
    match formatted.html {
        Some(html) => RoomMessageEventContent::text_html(formatted.body, html),
        None => RoomMessageEventContent::text_plain(formatted.body),
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
        if self.room(room_id)?.encryption_state().is_encrypted() {
            let (client, keys, room) =
                (self.client.clone(), self.room_keys.clone(), room_id.clone());
            tokio::spawn(async move { keys.download(&client, &room).await });
        }
        let timeline = Arc::new(
            self.room(room_id)?
                .timeline()
                .await
                .map_err(timeline_error)?,
        );
        let (items, stream) = timeline.subscribe().await;
        // Convert each item once, when it is added or changed, by mirroring
        // the SDK's diffs onto converted entries.
        let mut converter = Converter {
            own: self.user_id(),
            missing_replies: Vec::new(),
        };
        let mut converted: Vector<Option<TimelineEntry>> =
            items.iter().map(|item| converter.convert(item)).collect();
        let (tx, entries) = watch::channel(flatten(&converted));
        let mut stream = Box::pin(stream);
        let task = tokio::spawn({
            let timeline = timeline.clone();
            async move {
                // Replies whose original is not loaded are fetched once.
                let mut requested = HashSet::new();
                loop {
                    for event_id in converter.missing_replies.drain(..) {
                        if requested.insert(event_id.clone()) {
                            let timeline = timeline.clone();
                            tokio::spawn(async move {
                                if let Err(e) = timeline.fetch_details_for_event(&event_id).await {
                                    warn!("could not load the replied-to message: {e}");
                                }
                            });
                        }
                    }
                    let Some(diffs) = stream.next().await else {
                        return;
                    };
                    for diff in diffs {
                        diff.map(|item| converter.convert(&item))
                            .apply(&mut converted);
                    }
                    tx.send_replace(flatten(&converted));
                }
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

fn flatten(entries: &Vector<Option<TimelineEntry>>) -> Vec<TimelineEntry> {
    entries.iter().flatten().cloned().collect()
}

/// Converts SDK timeline items, noting replies whose original must be fetched.
struct Converter {
    own: OwnedUserId,
    missing_replies: Vec<OwnedEventId>,
}

impl Converter {
    fn convert(&mut self, item: &TimelineItem) -> Option<TimelineEntry> {
        match item.kind() {
            TimelineItemKind::Event(event) => Some(TimelineEntry::Event(
                self.convert_event(event, &item.unique_id().0)?,
            )),
            TimelineItemKind::Virtual(VirtualTimelineItem::DateDivider(ts)) => {
                Some(TimelineEntry::DayDivider(system_time(*ts)))
            }
            TimelineItemKind::Virtual(VirtualTimelineItem::ReadMarker) => {
                Some(TimelineEntry::ReadMarker)
            }
            TimelineItemKind::Virtual(_) => None,
        }
    }

    fn convert_event(&mut self, event: &EventTimelineItem, id: &str) -> Option<Event> {
        let content = match event.content() {
            TimelineItemContent::MsgLike(msg_like) => match &msg_like.kind {
                MsgLikeKind::Message(message) => EventContent::Message(Message {
                    kind: message_kind(message.msgtype()),
                    body: message.body().to_owned(),
                    document: match html_body(message.msgtype()) {
                        Some(html) => Document::from_html(html),
                        None => Document::from_plain(message.body()),
                    },
                    edited: message.is_edited(),
                    in_reply_to: msg_like.in_reply_to.as_ref().map(|r| self.reply(r)),
                    reactions: msg_like
                        .reactions
                        .iter()
                        .map(|(key, senders)| Reaction {
                            key: key.clone(),
                            count: senders.len(),
                            own: senders.contains_key(&self.own),
                        })
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
            Some(EventSendState::SendingFailed { error, .. }) => {
                SendState::Failed(error.to_string())
            }
        };
        let (sender_name, sender_avatar) = match event.sender_profile() {
            TimelineDetails::Ready(profile) => {
                (profile.display_name.clone(), profile.avatar_url.clone())
            }
            _ => (None, None),
        };

        Some(Event {
            id: id.to_owned(),
            key: EventKey(event.identifier()),
            event_id: event.event_id().map(ToOwned::to_owned),
            sender: event.sender().to_owned(),
            sender_name,
            sender_avatar,
            timestamp: system_time(event.timestamp()),
            own: event.is_own(),
            editable: event.is_editable(),
            content,
            send_state,
            read_by: event
                .read_receipts()
                .keys()
                .filter(|user| **user != self.own)
                .cloned()
                .collect(),
        })
    }

    fn reply(&mut self, reply: &InReplyToDetails) -> Reply {
        let mut out = Reply {
            event_id: reply.event_id.clone(),
            sender: None,
            sender_name: None,
            preview: None,
        };
        match &reply.event {
            TimelineDetails::Ready(original) => {
                out.sender = Some(original.sender.clone());
                if let TimelineDetails::Ready(profile) = &original.sender_profile {
                    out.sender_name = profile.display_name.clone();
                }
                out.preview = Some(match &original.content {
                    TimelineItemContent::MsgLike(m) => match &m.kind {
                        // The formatted text, without Markdown syntax.
                        MsgLikeKind::Message(message) => {
                            preview(&match html_body(message.msgtype()) {
                                Some(html) => Document::from_html(html).to_plain_text(),
                                None => message.body().to_owned(),
                            })
                        }
                        MsgLikeKind::Redacted => "Message deleted".into(),
                        _ => "…".into(),
                    },
                    _ => "…".into(),
                });
            }
            TimelineDetails::Unavailable => self.missing_replies.push(reply.event_id.clone()),
            TimelineDetails::Pending | TimelineDetails::Error(_) => {}
        }
        out
    }
}

/// The first line of a message, shortened.
fn preview(body: &str) -> String {
    const MAX: usize = 120;
    let line = body
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default();
    if line.chars().count() > MAX {
        format!("{}…", line.chars().take(MAX).collect::<String>())
    } else {
        line.to_owned()
    }
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

fn html_body(msgtype: &MessageType) -> Option<&str> {
    let formatted = match msgtype {
        MessageType::Text(c) => c.formatted.as_ref(),
        MessageType::Notice(c) => c.formatted.as_ref(),
        MessageType::Emote(c) => c.formatted.as_ref(),
        _ => None,
    }?;
    (formatted.format.as_str() == "org.matrix.custom.html").then_some(formatted.body.as_str())
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
