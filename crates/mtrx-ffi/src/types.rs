//! Plain data handed to the apps, converted from mtrx-core's types. IDs are
//! strings; times are `Date` (Swift) and `Instant` (Kotlin).

use std::time::{SystemTime, UNIX_EPOCH};

use mtrx_core as core;

/// Whether the server is reachable and still accepts the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionState {
    Online,
    /// The server is unreachable; sync retries until it answers. Messages
    /// wait in the send queue meanwhile.
    Offline,
    /// The server no longer accepts the session (it expired or was signed
    /// out elsewhere): call `Session.forget()` and sign in again.
    SignedOut,
}

impl From<core::Connection> for ConnectionState {
    fn from(connection: core::Connection) -> Self {
        match connection {
            core::Connection::Online => Self::Online,
            core::Connection::Offline => Self::Offline,
            core::Connection::SignedOut => Self::SignedOut,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RoomKind {
    Room,
    Space,
    /// A direct chat with one person.
    Direct,
}

/// One entry of the room list.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RoomSummary {
    /// `!…:server`; pass it to `Session.timeline`.
    pub id: String,
    pub name: String,
    /// `mxc://` URI, for `Session.avatar`. For a direct chat without its own
    /// picture, the other person's.
    pub avatar_url: Option<String>,
    pub topic: Option<String>,
    pub kind: RoomKind,
    /// We are invited but have not joined yet.
    pub invited: bool,
    pub encrypted: bool,
    pub unread_messages: u64,
    pub unread_mentions: u64,
    pub unread_notifications: u64,
    pub last_activity: Option<SystemTime>,
    /// For a direct chat, the other person's user ID.
    pub direct_user_id: Option<String>,
    /// The room's address, e.g. `#ice-sheets:example.org`.
    pub alias: Option<String>,
    pub joined_members: u64,
}

impl From<&core::RoomSummary> for RoomSummary {
    fn from(room: &core::RoomSummary) -> Self {
        Self {
            id: room.id.to_string(),
            name: room.name.clone(),
            avatar_url: room.avatar.as_ref().map(ToString::to_string),
            topic: room.topic.clone(),
            kind: match room.kind {
                core::RoomKind::Room => RoomKind::Room,
                core::RoomKind::Space => RoomKind::Space,
                core::RoomKind::Direct => RoomKind::Direct,
            },
            invited: room.invited,
            encrypted: room.encrypted,
            unread_messages: room.unread_messages,
            unread_mentions: room.unread_mentions,
            unread_notifications: room.unread_notifications,
            last_activity: room.last_activity,
            direct_user_id: room.direct_user.as_ref().map(ToString::to_string),
            alias: room.alias.clone(),
            joined_members: room.joined_members,
        }
    }
}

/// The room list: every joined and invited room (spaces too), most recently
/// active first.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RoomListState {
    pub rooms: Vec<RoomSummary>,
    /// Synced with the server since the list was created. Before that,
    /// `rooms` may be a stale cache, or empty on the first sign-in.
    pub loaded: bool,
}

impl From<core::RoomListState> for RoomListState {
    fn from(state: core::RoomListState) -> Self {
        Self {
            rooms: state.rooms.iter().map(RoomSummary::from).collect(),
            loaded: state.loaded,
        }
    }
}

/// One row of a timeline.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineItem {
    /// Unique in the timeline and stable while the item stays (also when a
    /// sent message's local echo becomes the server's event): use it as the
    /// row's identity in lists.
    pub id: String,
    pub content: TimelineItemContent,
}

// Nearly all items are events, so boxing would only add indirection.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum TimelineItemContent {
    /// The room's beginning; back-pagination reached it.
    Start,
    Event {
        event: TimelineEvent,
    },
    /// The first event of a new day follows.
    DayDivider {
        date: SystemTime,
    },
    /// Everything below was not read yet.
    ReadMarker,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineEvent {
    /// The server's ID; `None` while a message we sent is not sent yet.
    pub event_id: Option<String>,
    pub sender_id: String,
    pub sender_name: Option<String>,
    /// `mxc://` URI, for `Session.avatar`.
    pub sender_avatar_url: Option<String>,
    pub timestamp: SystemTime,
    /// We sent it.
    pub own: bool,
    pub content: EventContent,
    pub send_state: SendState,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum SendState {
    Sent,
    Sending,
    /// Not sent because the server is unreachable; sent once it is back.
    Waiting,
    Failed {
        error: String,
    },
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum EventContent {
    Message {
        message: Message,
    },
    /// Deleted.
    Redacted,
    /// Encrypted, and no key (yet). Keys may still arrive.
    UnableToDecrypt,
    Membership {
        user_id: String,
        name: Option<String>,
        change: MembershipChange,
    },
    DisplayNameChange {
        user_id: String,
        new_name: Option<String>,
    },
    /// A state event shown only by its type, e.g. `m.room.topic`.
    OtherState {
        event_type: String,
    },
    /// An event shown only by a description, e.g. `poll`.
    Other {
        description: String,
    },
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Message {
    pub kind: MessageKind,
    /// The text, without formatting. Math keeps its LaTeX source.
    pub body: String,
    pub edited: bool,
    /// The file of an image, video, audio or file message.
    pub media: Option<Media>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
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

/// A file sent in a message. Pass it to `Session.media_preview` or
/// `Session.media_file`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, uniffi::Record)]
pub struct Media {
    pub kind: MediaKind,
    pub filename: String,
    /// In bytes.
    pub size: Option<u64>,
    pub mimetype: Option<String>,
    /// In pixels, for images and videos: lay out before the image loads.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Where the file is (and its decryption key): opaque, for the library.
    pub source: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    File,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
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

/// The timeline as the apps see it, oldest first.
pub(crate) fn timeline_items(entries: &[core::TimelineEntry]) -> Vec<TimelineItem> {
    entries.iter().map(timeline_item).collect()
}

fn timeline_item(entry: &core::TimelineEntry) -> TimelineItem {
    match entry {
        core::TimelineEntry::Start => TimelineItem {
            id: "start".into(),
            content: TimelineItemContent::Start,
        },
        core::TimelineEntry::Event(event) => TimelineItem {
            id: event.id.clone(),
            content: TimelineItemContent::Event {
                event: TimelineEvent::from(event),
            },
        },
        core::TimelineEntry::DayDivider(date) => TimelineItem {
            // One per day.
            id: format!(
                "day-{}",
                date.duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            ),
            content: TimelineItemContent::DayDivider { date: *date },
        },
        core::TimelineEntry::ReadMarker => TimelineItem {
            id: "read-marker".into(),
            content: TimelineItemContent::ReadMarker,
        },
    }
}

impl From<&core::Event> for TimelineEvent {
    fn from(event: &core::Event) -> Self {
        Self {
            event_id: event.event_id.as_ref().map(ToString::to_string),
            sender_id: event.sender.to_string(),
            sender_name: event.sender_name.clone(),
            sender_avatar_url: event.sender_avatar.as_ref().map(ToString::to_string),
            timestamp: event.timestamp,
            own: event.own,
            content: EventContent::from(&event.content),
            send_state: match &event.send_state {
                core::SendState::Sent => SendState::Sent,
                core::SendState::Sending => SendState::Sending,
                core::SendState::Waiting => SendState::Waiting,
                core::SendState::Failed(error) => SendState::Failed {
                    error: error.clone(),
                },
            },
        }
    }
}

impl From<&core::EventContent> for EventContent {
    fn from(content: &core::EventContent) -> Self {
        match content {
            core::EventContent::Message(message) => Self::Message {
                message: Message::from(message),
            },
            core::EventContent::Redacted => Self::Redacted,
            core::EventContent::UnableToDecrypt => Self::UnableToDecrypt,
            core::EventContent::Membership { user, name, change } => Self::Membership {
                user_id: user.to_string(),
                name: name.clone(),
                change: membership_change(*change),
            },
            core::EventContent::DisplayNameChange { user, new_name } => Self::DisplayNameChange {
                user_id: user.to_string(),
                new_name: new_name.clone(),
            },
            core::EventContent::OtherState { event_type } => Self::OtherState {
                event_type: event_type.clone(),
            },
            core::EventContent::Other { description } => Self::Other {
                description: description.clone(),
            },
        }
    }
}

impl From<&core::Message> for Message {
    fn from(message: &core::Message) -> Self {
        Self {
            kind: match message.kind {
                core::MessageKind::Text => MessageKind::Text,
                core::MessageKind::Notice => MessageKind::Notice,
                core::MessageKind::Emote => MessageKind::Emote,
                core::MessageKind::Image => MessageKind::Image,
                core::MessageKind::Video => MessageKind::Video,
                core::MessageKind::Audio => MessageKind::Audio,
                core::MessageKind::File => MessageKind::File,
                core::MessageKind::Location => MessageKind::Location,
                core::MessageKind::Other => MessageKind::Other,
            },
            body: message.document.to_plain_text(),
            edited: message.edited,
            media: message.media.as_ref().map(Media::from),
        }
    }
}

impl From<&core::Media> for Media {
    fn from(media: &core::Media) -> Self {
        Self {
            kind: match media.kind {
                core::MediaKind::Image => MediaKind::Image,
                core::MediaKind::Video => MediaKind::Video,
                core::MediaKind::Audio => MediaKind::Audio,
                core::MediaKind::File => MediaKind::File,
            },
            filename: media.filename.clone(),
            size: media.size,
            mimetype: media.mimetype.clone(),
            width: media.width,
            height: media.height,
            source: serde_json::to_string(&media.source).expect("media sources serialize"),
        }
    }
}

impl Media {
    /// The core's reference to the file, from [`Media::source`].
    pub(crate) fn media_ref(&self) -> Result<core::MediaRef, crate::MtrxError> {
        serde_json::from_str(&self.source).map_err(crate::MtrxError::invalid)
    }
}

fn membership_change(change: core::MembershipChange) -> MembershipChange {
    match change {
        core::MembershipChange::Joined => MembershipChange::Joined,
        core::MembershipChange::Left => MembershipChange::Left,
        core::MembershipChange::Invited => MembershipChange::Invited,
        core::MembershipChange::InvitationAccepted => MembershipChange::InvitationAccepted,
        core::MembershipChange::InvitationRejected => MembershipChange::InvitationRejected,
        core::MembershipChange::InvitationRevoked => MembershipChange::InvitationRevoked,
        core::MembershipChange::Kicked => MembershipChange::Kicked,
        core::MembershipChange::Banned => MembershipChange::Banned,
        core::MembershipChange::Unbanned => MembershipChange::Unbanned,
        core::MembershipChange::Knocked => MembershipChange::Knocked,
        core::MembershipChange::Other => MembershipChange::Other,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mtrx_format::Document;

    use super::*;

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    fn event(id: &str, content: core::EventContent) -> core::Event {
        core::Event {
            id: id.into(),
            key: core::EventKey::local(id),
            event_id: None,
            sender: "@alice:example.org".try_into().unwrap(),
            sender_name: Some("Alice".into()),
            sender_avatar: Some("mxc://example.org/alice".into()),
            timestamp: at(1_700_000_000),
            own: true,
            editable: true,
            content,
            send_state: core::SendState::Sending,
            read_by: Vec::new(),
        }
    }

    fn message(document: Document, media: Option<core::Media>) -> core::EventContent {
        core::EventContent::Message(core::Message {
            kind: if media.is_some() {
                core::MessageKind::Image
            } else {
                core::MessageKind::Text
            },
            body: "**raw** body".into(),
            document,
            edited: false,
            in_reply_to: None,
            reactions: Vec::new(),
            media,
        })
    }

    #[test]
    fn timeline_items_have_unique_ids() {
        let entries = vec![
            core::TimelineEntry::Start,
            core::TimelineEntry::DayDivider(at(86_400)),
            core::TimelineEntry::Event(event("a", core::EventContent::Redacted)),
            core::TimelineEntry::ReadMarker,
            core::TimelineEntry::DayDivider(at(2 * 86_400)),
            core::TimelineEntry::Event(event("b", core::EventContent::UnableToDecrypt)),
        ];
        let ids: Vec<_> = timeline_items(&entries).into_iter().map(|i| i.id).collect();
        assert_eq!(
            ids,
            [
                "start",
                "day-86400000",
                "a",
                "read-marker",
                "day-172800000",
                "b"
            ]
        );
    }

    #[test]
    fn events_carry_plain_ids_and_text() {
        let entries = [core::TimelineEntry::Event(event(
            "a",
            message(Document::from_html("<b>bold</b> text"), None),
        ))];
        let [item] = timeline_items(&entries).try_into().unwrap();
        let TimelineItemContent::Event { event } = item.content else {
            panic!("not an event: {:?}", item.content);
        };
        assert_eq!(event.sender_id, "@alice:example.org");
        assert_eq!(
            event.sender_avatar_url.as_deref(),
            Some("mxc://example.org/alice")
        );
        assert_eq!(event.timestamp, at(1_700_000_000));
        assert_eq!(event.send_state, SendState::Sending);
        assert_eq!(
            event.content,
            EventContent::Message {
                message: Message {
                    kind: MessageKind::Text,
                    body: "bold text".into(),
                    edited: false,
                    media: None,
                }
            }
        );
    }

    #[test]
    fn media_sources_round_trip() {
        let core_media = core::Media {
            kind: core::MediaKind::Image,
            filename: "ice.png".into(),
            size: Some(1234),
            mimetype: Some("image/png".into()),
            width: Some(800),
            height: Some(600),
            source: core::MediaRef::plain("mxc://example.org/ice"),
        };
        let media = Media::from(&core_media);
        assert_eq!((media.width, media.height), (Some(800), Some(600)));
        assert_eq!(media.media_ref().unwrap(), core_media.source);
        assert_eq!(
            media.media_ref().unwrap().uri().as_str(),
            "mxc://example.org/ice"
        );

        let tampered = Media {
            source: "{}".into(),
            ..media
        };
        assert!(matches!(
            tampered.media_ref(),
            Err(crate::MtrxError::InvalidInput { .. })
        ));
    }

    #[test]
    fn room_summaries_use_plain_ids() {
        let room = core::RoomSummary {
            id: "!room:example.org".try_into().unwrap(),
            name: "Ice sheets".into(),
            avatar: None,
            topic: Some("Greenland".into()),
            kind: core::RoomKind::Direct,
            invited: false,
            encrypted: true,
            public: false,
            unread_messages: 3,
            unread_mentions: 1,
            unread_notifications: 2,
            last_activity: Some(at(5)),
            parents: Vec::new(),
            tag: core::RoomTag::None,
            direct_user: Some("@bob:example.org".try_into().unwrap()),
            alias: None,
            joined_members: 2,
        };
        let state = RoomListState::from(core::RoomListState {
            rooms: vec![room],
            loaded: true,
        });
        assert!(state.loaded);
        let summary = &state.rooms[0];
        assert_eq!(summary.id, "!room:example.org");
        assert_eq!(summary.kind, RoomKind::Direct);
        assert_eq!(summary.direct_user_id.as_deref(), Some("@bob:example.org"));
        assert_eq!(summary.last_activity, Some(at(5)));
        assert_eq!(summary.unread_mentions, 1);
    }

    #[test]
    fn connection_states_map_one_to_one() {
        assert_eq!(
            ConnectionState::from(core::Connection::SignedOut),
            ConnectionState::SignedOut
        );
        assert_eq!(
            ConnectionState::from(core::Connection::Offline),
            ConnectionState::Offline
        );
    }
}
