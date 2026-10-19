//! Notifications for new messages, as the account's push rules decide
//! (mentions, direct chats, rooms set to all messages).

use futures_util::Stream;
use matrix_sdk::{
    Client, Room,
    deserialized_responses::RawAnySyncOrStrippedTimelineEvent,
    ruma::{
        OwnedRoomId,
        events::{
            AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent,
            room::encrypted::OriginalSyncRoomEncryptedEvent,
        },
        serde::Raw,
    },
    sync::Notification as SdkNotification,
};
use tokio::sync::broadcast;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

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

/// Turn the SDK's notifications into [`Notification`]s on a channel.
pub(crate) async fn register(client: &Client) -> broadcast::Sender<Notification> {
    let (sender, _) = broadcast::channel(64);
    let tx = sender.clone();
    client
        .register_notification_handler(move |notification, room, _client| {
            let tx = tx.clone();
            async move {
                if let Some(notification) = describe(notification, room).await {
                    let _ = tx.send(notification);
                }
            }
        })
        .await;
    sender
}

async fn describe(notification: SdkNotification, room: Room) -> Option<Notification> {
    if !notification.actions.iter().any(|a| a.should_notify()) {
        return None;
    }
    let RawAnySyncOrStrippedTimelineEvent::Sync(raw) = notification.event else {
        return None;
    };
    let event = raw.deserialize().ok()?;
    if event.sender() == room.own_user_id() {
        return None;
    }
    let body = match message_body(&event) {
        Some(body) => body,
        None if is_encrypted(&event) => decrypted_body(&room, &raw)
            .await
            .unwrap_or_else(|| "Encrypted message".into()),
        None => return None,
    };
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
        highlight: notification.actions.iter().any(|a| a.is_highlight()),
        direct: room.direct_targets_length() > 0,
    })
}

fn message_body(event: &AnySyncTimelineEvent) -> Option<String> {
    match event {
        AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
            SyncMessageLikeEvent::Original(message),
        )) => Some(message.content.body().to_owned()),
        _ => None,
    }
}

fn is_encrypted(event: &AnySyncTimelineEvent) -> bool {
    matches!(
        event,
        AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomEncrypted(_))
    )
}

async fn decrypted_body(room: &Room, raw: &Raw<AnySyncTimelineEvent>) -> Option<String> {
    let encrypted: Raw<OriginalSyncRoomEncryptedEvent> = raw.clone().cast_unchecked();
    let decrypted = room.decrypt_event(&encrypted, None).await.ok()?;
    message_body(&decrypted.raw().deserialize().ok()?)
}

impl Session {
    /// Notifications for new messages while syncing.
    pub fn notifications(&self) -> impl Stream<Item = Notification> + use<> {
        BroadcastStream::new(self.notifications.subscribe()).filter_map(Result::ok)
    }
}
