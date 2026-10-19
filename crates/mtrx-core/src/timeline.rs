//! Live room timeline: messages, state changes, sending.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
    time::SystemTime,
};

use eyeball_im::Vector;
use futures_util::{Stream, StreamExt, stream};
use matrix_sdk::ruma::{
    EventId, OwnedEventId, OwnedMxcUri, OwnedRoomId, OwnedTransactionId, OwnedUserId, UInt,
    api::client::receipt::create_receipt::v3::ReceiptType,
    events::{
        relation::Reply as ReplyRelation,
        room::{
            MediaSource,
            message::{MessageType, Relation, RoomMessageEventContent},
        },
    },
};
use matrix_sdk::{Room, room::edit::EditedContent, send_queue::RoomSendQueueUpdate};
use matrix_sdk_ui::timeline::{
    EventSendState, EventTimelineItem, InReplyToDetails, MembershipChange as SdkMembershipChange,
    MsgLikeKind, RoomExt, Timeline, TimelineDetails, TimelineEventItemId, TimelineItem,
    TimelineItemContent, TimelineItemKind, VirtualTimelineItem,
};
use tokio::{
    sync::{Mutex, broadcast, mpsc, watch},
    task::JoinHandle,
};
use tokio_stream::wrappers::{BroadcastStream, WatchStream};
use tracing::warn;

use crate::{
    Error, Result, Session,
    media::{self, MediaRef},
    system_time,
};
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
    pub read_by: Vec<Reader>,
}

/// Someone who read up to an event.
#[derive(Clone, Debug, PartialEq)]
pub struct Reader {
    pub id: OwnedUserId,
    /// Display name and avatar, once known.
    pub name: Option<String>,
    pub avatar: Option<OwnedMxcUri>,
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

// Most events are messages, so boxing would only add indirection.
#[allow(clippy::large_enum_variant)]
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
    /// The file of an image, video, audio or file message.
    pub media: Option<Media>,
}

/// A file sent in a message.
#[derive(Clone, Debug, PartialEq)]
pub struct Media {
    pub kind: MediaKind,
    pub filename: String,
    /// In bytes.
    pub size: Option<u64>,
    pub mimetype: Option<String>,
    /// In pixels, for images and videos.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// For downloading it, see [`Session::media_content`].
    pub source: MediaRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    File,
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

    /// Queue the file at `path`, e.g. an image, to be uploaded and sent. Its
    /// local echo shows as [`SendState::Sending`] until the server confirms.
    /// The MIME type is guessed from the file name.
    pub async fn send_file(&self, path: &Path) -> Result<()> {
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Error::Timeline(format!("not a file: {}", path.display())))?;
        let bytes = tokio::fs::read(path).await?;
        let mimetype = media::guess_mimetype(filename);
        self.send_file_bytes(filename, mimetype.as_ref(), bytes)
            .await
    }

    /// Queue `bytes` to be sent as a file named `filename` of MIME type
    /// `mimetype`, like [`RoomTimeline::send_file`].
    pub async fn send_file_bytes(
        &self,
        filename: &str,
        mimetype: &str,
        bytes: Vec<u8>,
    ) -> Result<()> {
        let mimetype: mime_guess::Mime = mimetype
            .parse()
            .map_err(|_| Error::InvalidMediaType(mimetype.to_owned()))?;
        // Images are decoded for their size and thumbnail.
        let (bytes, config, mimetype) = tokio::task::spawn_blocking(move || {
            let config = media::attachment_config(&mimetype, &bytes);
            (bytes, config, mimetype)
        })
        .await
        .map_err(timeline_error)?;
        self.timeline
            .room()
            .send_queue()
            .send_attachment(filename, mimetype, bytes, config)
            .await
            .map_err(timeline_error)?;
        Ok(())
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
            profiles: HashMap::new(),
            missing_profiles: Vec::new(),
        };
        let mut items = items;
        let mut converted: Vector<Option<TimelineEntry>> =
            items.iter().map(|item| converter.convert(item)).collect();
        let (tx, entries) = watch::channel(flatten(&converted));
        let mut stream = Box::pin(stream);
        let (profile_tx, mut profile_rx) = mpsc::unbounded_channel();
        let task = tokio::spawn({
            let timeline = timeline.clone();
            async move {
                // Replies whose original is not loaded, and readers not seen
                // as senders, are fetched once.
                let mut requested = HashSet::new();
                let mut requested_profiles = HashSet::new();
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
                    for user in converter.missing_profiles.drain(..) {
                        if requested_profiles.insert(user.clone()) {
                            let (room, profile_tx) = (timeline.room().clone(), profile_tx.clone());
                            tokio::spawn(async move {
                                let profile = member_profile(&room, &user).await;
                                profile_tx.send((user, profile)).ok();
                            });
                        }
                    }
                    tokio::select! {
                        diffs = stream.next() => {
                            let Some(diffs) = diffs else {
                                return;
                            };
                            for diff in diffs {
                                diff.clone().apply(&mut items);
                                diff.map(|item| converter.convert(&item))
                                    .apply(&mut converted);
                            }
                        }
                        Some((user, profile)) = profile_rx.recv() => {
                            converter.profiles.insert(user.clone(), profile);
                            // Redraw the events this reader is on.
                            for (index, item) in items.iter().enumerate() {
                                let reads = item
                                    .as_event()
                                    .is_some_and(|e| e.read_receipts().contains_key(&user));
                                if reads {
                                    converted.set(index, converter.convert(item));
                                }
                            }
                        }
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

/// A member's name and avatar from the store, else from their profile.
async fn member_profile(room: &Room, user: &OwnedUserId) -> Profile {
    if let Ok(Some(member)) = room.get_member_no_sync(user).await {
        return (
            member.display_name().map(ToOwned::to_owned),
            member.avatar_url().map(ToOwned::to_owned),
        );
    }
    match room.client().account().fetch_user_profile_of(user).await {
        Ok(profile) => (
            profile
                .get_static::<matrix_sdk::ruma::api::client::profile::DisplayName>()
                .ok()
                .flatten(),
            profile
                .get_static::<matrix_sdk::ruma::api::client::profile::AvatarUrl>()
                .ok()
                .flatten(),
        ),
        Err(e) => {
            warn!("could not load the profile of {user}: {e}");
            (None, None)
        }
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
    /// Names and avatars of senders and readers.
    profiles: HashMap<OwnedUserId, Profile>,
    /// Readers whose profile is not known.
    missing_profiles: Vec<OwnedUserId>,
}

/// Display name and avatar.
type Profile = (Option<String>, Option<OwnedMxcUri>);

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
                    media: media(message.msgtype()),
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
        if sender_name.is_some() || sender_avatar.is_some() {
            self.profiles.insert(
                event.sender().to_owned(),
                (sender_name.clone(), sender_avatar.clone()),
            );
        }
        let own = self.own.clone();
        let read_by = event
            .read_receipts()
            .keys()
            .filter(|user| **user != own)
            .map(|user| self.reader(user))
            .collect();

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
            read_by,
        })
    }

    fn reader(&mut self, user: &OwnedUserId) -> Reader {
        let (name, avatar) = match self.profiles.get(user) {
            Some(profile) => profile.clone(),
            None => {
                self.missing_profiles.push(user.clone());
                (None, None)
            }
        };
        Reader {
            id: user.clone(),
            name,
            avatar,
        }
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

fn media(msgtype: &MessageType) -> Option<Media> {
    let media = |kind, filename: &str, source: &MediaSource, thumbnail| Media {
        kind,
        filename: filename.to_owned(),
        size: None,
        mimetype: None,
        width: None,
        height: None,
        source: MediaRef::new(source, thumbnail),
    };
    let pixels = |n: Option<UInt>| n.and_then(|n| u32::try_from(n).ok());
    Some(match msgtype {
        MessageType::Image(c) => {
            let info = c.info.as_deref();
            Media {
                size: info.and_then(|i| i.size).map(u64::from),
                mimetype: info.and_then(|i| i.mimetype.clone()),
                width: pixels(info.and_then(|i| i.width)),
                height: pixels(info.and_then(|i| i.height)),
                ..media(
                    MediaKind::Image,
                    c.filename(),
                    &c.source,
                    info.and_then(|i| i.thumbnail_source.as_ref()),
                )
            }
        }
        MessageType::Video(c) => {
            let info = c.info.as_deref();
            Media {
                size: info.and_then(|i| i.size).map(u64::from),
                mimetype: info.and_then(|i| i.mimetype.clone()),
                width: pixels(info.and_then(|i| i.width)),
                height: pixels(info.and_then(|i| i.height)),
                ..media(
                    MediaKind::Video,
                    c.filename(),
                    &c.source,
                    info.and_then(|i| i.thumbnail_source.as_ref()),
                )
            }
        }
        MessageType::Audio(c) => {
            let info = c.info.as_deref();
            Media {
                size: info.and_then(|i| i.size).map(u64::from),
                mimetype: info.and_then(|i| i.mimetype.clone()),
                ..media(MediaKind::Audio, c.filename(), &c.source, None)
            }
        }
        MessageType::File(c) => {
            let info = c.info.as_deref();
            Media {
                size: info.and_then(|i| i.size).map(u64::from),
                mimetype: info.and_then(|i| i.mimetype.clone()),
                ..media(
                    MediaKind::File,
                    c.filename(),
                    &c.source,
                    info.and_then(|i| i.thumbnail_source.as_ref()),
                )
            }
        }
        _ => return None,
    })
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
