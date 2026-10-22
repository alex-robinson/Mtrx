//! Notifications for new messages, as the account's push rules decide
//! (mentions, direct chats, rooms set to all messages).
//!
//! Every synced timeline event is looked at, whether or not a timeline is
//! open for its room. An event sync could not decrypt yet is decrypted by
//! the SDK's [`NotificationClient`], which waits for its room key, before
//! the push rules decide about its content: a mention is only seen then.

use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use futures_util::Stream;
use matrix_sdk::{
    Client, Room,
    event_handler::EventHandlerDropGuard,
    notification_settings::{NotificationSettings, RoomNotificationMode},
    ruma::{
        EventId, MilliSecondsSinceUnixEpoch, OwnedEventId, OwnedRoomId, UserId,
        events::{AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent},
        push::Action,
        serde::Raw,
    },
};
use matrix_sdk_ui::{
    notification_client::{
        NotificationClient, NotificationEvent, NotificationProcessSetup, NotificationStatus,
    },
    sync_service::SyncService,
};
use tokio::sync::{OnceCell, broadcast};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tracing::warn;

use crate::Session;

/// A message worth telling the user about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub room_id: OwnedRoomId,
    pub room_name: String,
    pub sender_name: String,
    /// Plain text; "Encrypted message" if it cannot be decrypted yet.
    pub body: String,
    /// Mentions us or matches a keyword.
    pub highlight: bool,
    pub direct: bool,
}

/// How many notified events are remembered, so that one synced again (after
/// the server restarted the sync from scratch) does not notify again.
const REMEMBERED: usize = 10_000;

/// Turns synced events into [`Notification`]s, each event once.
struct Notifier {
    tx: broadcast::Sender<Notification>,
    sync: Arc<SyncService>,
    settings: NotificationSettings,
    /// Events sent before the session started do not notify: sync brings
    /// them as rooms' history (on the first sync, after the server restarted
    /// the sync, or as what came while the app was closed).
    since: MilliSecondsSinceUnixEpoch,
    seen: Mutex<Seen>,
    /// Made on the first event sync could not decrypt.
    decryptor: OnceCell<NotificationClient>,
}

/// Notify of new messages on the returned channel while `client` syncs,
/// until the guard is dropped.
pub(crate) fn register(
    client: &Client,
    sync: Arc<SyncService>,
    settings: NotificationSettings,
) -> (broadcast::Sender<Notification>, EventHandlerDropGuard) {
    let (tx, _) = broadcast::channel(64);
    let notifier = Arc::new(Notifier {
        tx: tx.clone(),
        sync,
        settings,
        since: MilliSecondsSinceUnixEpoch::now(),
        seen: Mutex::new(Seen::new(REMEMBERED)),
        decryptor: OnceCell::new(),
    });
    let handle = client.add_event_handler(
        move |raw: Raw<AnySyncTimelineEvent>, room: Room, actions: Vec<Action>| {
            let notifier = notifier.clone();
            async move { notifier.synced(raw, room, actions).await }
        },
    );
    (tx, client.event_handler_drop_guard(handle))
}

impl Notifier {
    /// `actions` are the push rules' for the event as synced (decrypted if
    /// sync could).
    async fn synced(
        self: Arc<Self>,
        raw: Raw<AnySyncTimelineEvent>,
        room: Room,
        actions: Vec<Action>,
    ) {
        let Ok(event) = raw.deserialize() else {
            return;
        };
        match triage(&event, room.own_user_id(), self.since, &actions) {
            Triage::Ignore => {}
            Triage::Notify => {
                if self.claim(&event) {
                    self.send(describe(&room, &event, &actions).await);
                }
            }
            Triage::Decrypt => {
                if !self.muted(&room).await && self.claim(&event) {
                    tokio::spawn(self.decrypt(room, event, actions));
                }
            }
        }
    }

    /// Notify of an event sync could not decrypt, as the push rules decide
    /// once it is decrypted. If it cannot be, as they decide for the
    /// encrypted event (e.g. "Encrypted message" in a room notifying of all
    /// messages).
    async fn decrypt(
        self: Arc<Self>,
        room: Room,
        event: AnySyncTimelineEvent,
        actions: Vec<Action>,
    ) {
        let status = match self.decryptor(&room).await {
            Ok(decryptor) => decryptor
                .get_notification(room.room_id(), event.event_id())
                .await
                .map_err(|e| e.to_string()),
            Err(e) => Err(e),
        };
        match status {
            Ok(NotificationStatus::Event(item)) => {
                if let NotificationEvent::Timeline(decrypted) = item.event {
                    let actions = item.actions.unwrap_or_default();
                    self.send(describe(&room, &decrypted, &actions).await);
                }
            }
            Ok(NotificationStatus::EventFilteredOut | NotificationStatus::EventRedacted) => {}
            Ok(NotificationStatus::EventNotFound) | Err(_) => {
                if let Err(e) = status {
                    warn!(event = %event.event_id(), "could not decrypt a notification: {e}");
                }
                if should_notify(&actions) {
                    self.send(describe(&room, &event, &actions).await);
                } else {
                    // It may come again, e.g. once the sync restarts.
                    self.seen.lock().unwrap().release(event.event_id());
                }
            }
        }
    }

    async fn decryptor(&self, room: &Room) -> Result<&NotificationClient, String> {
        self.decryptor
            .get_or_try_init(|| {
                NotificationClient::new(
                    room.client(),
                    NotificationProcessSetup::SingleProcess {
                        sync_service: self.sync.clone(),
                    },
                )
            })
            .await
            .map_err(|e| e.to_string())
    }

    async fn muted(&self, room: &Room) -> bool {
        self.settings
            .get_user_defined_room_notification_mode(room.room_id())
            .await
            == Some(RoomNotificationMode::Mute)
    }

    /// Whether the event is not yet notified of (and now is).
    fn claim(&self, event: &AnySyncTimelineEvent) -> bool {
        self.seen.lock().unwrap().claim(event.event_id())
    }

    fn send(&self, notification: Option<Notification>) {
        if let Some(notification) = notification {
            let _ = self.tx.send(notification);
        }
    }
}

/// What to do with a synced event.
#[derive(Debug, PartialEq, Eq)]
enum Triage {
    Ignore,
    /// The push rules decided on its content.
    Notify,
    /// Still encrypted: the push rules can only decide on its content.
    Decrypt,
}

fn triage(
    event: &AnySyncTimelineEvent,
    own_user: &UserId,
    since: MilliSecondsSinceUnixEpoch,
    actions: &[Action],
) -> Triage {
    if event.sender() == own_user || event.origin_server_ts() < since {
        Triage::Ignore
    } else if is_encrypted(event) {
        Triage::Decrypt
    } else if should_notify(actions) {
        Triage::Notify
    } else {
        Triage::Ignore
    }
}

fn should_notify(actions: &[Action]) -> bool {
    actions.iter().any(Action::should_notify)
}

async fn describe(
    room: &Room,
    event: &AnySyncTimelineEvent,
    actions: &[Action],
) -> Option<Notification> {
    let body = body(event)?;
    let sender_name = match room.get_member_no_sync(event.sender()).await {
        Ok(Some(member)) => member.name().to_owned(),
        _ => event.sender().localpart().to_owned(),
    };
    let room_name = match room.display_name().await {
        Ok(name) => name.to_string(),
        Err(_) => room.room_id().to_string(),
    };
    Some(Notification {
        room_id: room.room_id().to_owned(),
        room_name,
        sender_name,
        body,
        highlight: actions.iter().any(Action::is_highlight),
        direct: room.direct_targets_length() > 0,
    })
}

/// The text to show for a message; none for other events.
fn body(event: &AnySyncTimelineEvent) -> Option<String> {
    match event {
        AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
            SyncMessageLikeEvent::Original(message),
        )) => Some(message.content.body().to_owned()),
        _ if is_encrypted(event) => Some("Encrypted message".into()),
        _ => None,
    }
}

fn is_encrypted(event: &AnySyncTimelineEvent) -> bool {
    matches!(
        event,
        AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomEncrypted(
            SyncMessageLikeEvent::Original(_)
        ))
    )
}

/// The most recent events claimed, at most `capacity`.
struct Seen {
    ids: HashSet<OwnedEventId>,
    order: VecDeque<OwnedEventId>,
    capacity: usize,
}

impl Seen {
    fn new(capacity: usize) -> Self {
        Self {
            ids: HashSet::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    /// False if `id` was claimed before.
    fn claim(&mut self, id: &EventId) -> bool {
        if !self.ids.insert(id.to_owned()) {
            return false;
        }
        self.order.push_back(id.to_owned());
        if self.order.len() > self.capacity
            && let Some(oldest) = self.order.pop_front()
        {
            self.ids.remove(&oldest);
        }
        true
    }

    fn release(&mut self, id: &EventId) {
        if self.ids.remove(id) {
            self.order.retain(|claimed| claimed.as_str() != id.as_str());
        }
    }
}

impl Session {
    /// Notifications for new messages while syncing.
    pub fn notifications(&self) -> impl Stream<Item = Notification> + use<> {
        BroadcastStream::new(self.notifications.subscribe()).filter_map(Result::ok)
    }
}

#[cfg(test)]
mod tests {
    use matrix_sdk::ruma::{owned_event_id, uint, user_id};

    use super::*;

    const OWN: &str = "@bob:example.org";

    fn event(
        kind: &str,
        sender: &str,
        ts: u64,
        content: serde_json::Value,
    ) -> AnySyncTimelineEvent {
        serde_json::from_value(serde_json::json!({
            "type": kind,
            "event_id": format!("$e{ts}"),
            "sender": sender,
            "origin_server_ts": ts,
            "content": content,
        }))
        .unwrap()
    }

    fn text(sender: &str, ts: u64, body: &str) -> AnySyncTimelineEvent {
        event(
            "m.room.message",
            sender,
            ts,
            serde_json::json!({ "msgtype": "m.text", "body": body }),
        )
    }

    fn encrypted(sender: &str, ts: u64) -> AnySyncTimelineEvent {
        event(
            "m.room.encrypted",
            sender,
            ts,
            serde_json::json!({
                "algorithm": "m.megolm.v1.aes-sha2",
                "ciphertext": "AwgAEnAC",
                "device_id": "DEVICE",
                "sender_key": "c2VuZGVya2V5",
                "session_id": "c2Vzc2lvbmlk",
            }),
        )
    }

    fn triage_at_10(event: &AnySyncTimelineEvent, actions: &[Action]) -> Triage {
        let since = MilliSecondsSinceUnixEpoch(uint!(10));
        triage(event, user_id!("@bob:example.org"), since, actions)
    }

    const NOTIFY: &[Action] = &[Action::Notify];

    #[test]
    fn messages_notify_as_the_push_rules_say() {
        let message = text("@alice:example.org", 20, "hi");
        assert_eq!(triage_at_10(&message, NOTIFY), Triage::Notify);
        assert_eq!(triage_at_10(&message, &[]), Triage::Ignore);
    }

    #[test]
    fn own_and_earlier_messages_do_not_notify() {
        assert_eq!(triage_at_10(&text(OWN, 20, "mine"), NOTIFY), Triage::Ignore);
        assert_eq!(triage_at_10(&encrypted(OWN, 20), NOTIFY), Triage::Ignore);
        let earlier = text("@alice:example.org", 9, "history");
        assert_eq!(triage_at_10(&earlier, NOTIFY), Triage::Ignore);
        assert_eq!(
            triage_at_10(&encrypted("@alice:example.org", 9), NOTIFY),
            Triage::Ignore
        );
    }

    #[test]
    fn encrypted_messages_are_decrypted_whatever_the_rules_say_of_them() {
        // In a room notifying of mentions only, the rules see no mention in
        // an encrypted message.
        let message = encrypted("@alice:example.org", 10);
        assert_eq!(triage_at_10(&message, &[]), Triage::Decrypt);
        assert_eq!(triage_at_10(&message, NOTIFY), Triage::Decrypt);
    }

    #[test]
    fn bodies_of_messages_and_encrypted_ones() {
        assert_eq!(
            body(&text("@alice:example.org", 1, "hi")).as_deref(),
            Some("hi")
        );
        assert_eq!(
            body(&encrypted("@alice:example.org", 1)).as_deref(),
            Some("Encrypted message")
        );
        let reaction = event(
            "m.reaction",
            "@alice:example.org",
            1,
            serde_json::json!({ "m.relates_to": {
                "rel_type": "m.annotation", "event_id": "$x", "key": "👍"
            }}),
        );
        assert_eq!(body(&reaction), None);
    }

    #[test]
    fn an_event_is_claimed_once() {
        let mut seen = Seen::new(10);
        let id = owned_event_id!("$a");
        assert!(seen.claim(&id));
        assert!(!seen.claim(&id));
        seen.release(&id);
        assert!(seen.claim(&id));
        assert!(!seen.claim(&id));
    }

    #[test]
    fn the_oldest_claims_are_forgotten() {
        let mut seen = Seen::new(2);
        let (a, b, c) = (
            owned_event_id!("$a"),
            owned_event_id!("$b"),
            owned_event_id!("$c"),
        );
        assert!(seen.claim(&a));
        assert!(seen.claim(&b));
        seen.release(&a);
        assert!(seen.claim(&a));
        assert!(seen.claim(&c));
        // `b` was the oldest claim left.
        assert!(seen.claim(&b));
        assert!(!seen.claim(&c));
        assert_eq!(seen.ids.len(), seen.order.len());
    }
}
