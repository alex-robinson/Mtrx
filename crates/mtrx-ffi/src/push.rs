//! Push notifications: registering a pusher, and reading what a push is
//! about from the process that receives it (docs/mobile.md, section 10).

use std::time::SystemTime;

use mtrx_core::{self as core, OwnedRoomId, Url};

use crate::{MtrxError, Session, runtime::run};

/// A pusher of the account, set by this device or another.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PusherInfo {
    pub pushkey: String,
    pub app_id: String,
    pub app_name: String,
    pub device_name: String,
    pub lang: String,
    /// The push gateway's notify URL; nil for a pusher that is not HTTP.
    #[uniffi(default)]
    pub gateway_url: Option<String>,
    /// The server sends only the event and room IDs.
    #[uniffi(default)]
    pub event_id_only: bool,
}

impl From<core::PusherInfo> for PusherInfo {
    fn from(pusher: core::PusherInfo) -> Self {
        Self {
            pushkey: pusher.pushkey,
            app_id: pusher.app_id,
            app_name: pusher.app_name,
            device_name: pusher.device_name,
            lang: pusher.lang,
            gateway_url: pusher.gateway_url,
            event_id_only: pusher.event_id_only,
        }
    }
}

/// What a push is about, for a local notification.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct NotificationContent {
    pub room_id: String,
    pub event_id: String,
    pub room_name: String,
    /// `mxc://` URI, for `Session.avatar`.
    #[uniffi(default)]
    pub room_avatar_url: Option<String>,
    pub is_direct: bool,
    pub sender_id: String,
    /// The sender's name in the room, else their ID's localpart.
    pub sender_name: String,
    /// `mxc://` URI, for `Session.avatar`.
    #[uniffi(default)]
    pub sender_avatar_url: Option<String>,
    /// A message's text, or what it is: "Image", "Video", "Audio", "File",
    /// "Location", "Sticker", "Call", "Encrypted message" (no key yet),
    /// "Invited you", "New message", "New event".
    pub body: String,
    /// Mentions us or matches a keyword.
    pub is_mention: bool,
    /// The push rules ask for a sound.
    pub is_noisy: bool,
    pub timestamp: SystemTime,
    /// Post it: someone else's event that the push rules notify for.
    /// Events they do not notify for (our own among them) come as nil
    /// instead, so this is false only for our own event when the rules
    /// could not be read. Check it all the same.
    pub should_notify: bool,
}

impl From<core::PushNotification> for NotificationContent {
    fn from(n: core::PushNotification) -> Self {
        Self {
            room_id: n.room_id.to_string(),
            event_id: n.event_id.to_string(),
            room_name: n.room_name,
            room_avatar_url: n.room_avatar_url,
            is_direct: n.is_direct,
            sender_id: n.sender_id.to_string(),
            sender_name: n.sender_name,
            sender_avatar_url: n.sender_avatar_url,
            body: n.body,
            is_mention: n.is_mention,
            is_noisy: n.is_noisy,
            timestamp: n.timestamp,
            should_notify: n.should_notify,
        }
    }
}

#[uniffi::export]
impl Session {
    /// Have the server push this account's notifications to a push gateway
    /// (`gateway_url`, ending in `/_matrix/push/v1/notify`) for `pushkey`
    /// (UnifiedPush: the endpoint URL), sending only event and room IDs.
    /// `app_id` e.g. `org.mtrx.app`; `lang` e.g. `en`. Calling it again
    /// replaces the pusher of the same `pushkey` and `app_id`.
    pub async fn set_pusher(
        &self,
        gateway_url: String,
        pushkey: String,
        app_id: String,
        app_name: String,
        device_name: String,
        lang: String,
    ) -> Result<(), MtrxError> {
        let gateway_url = Url::parse(&gateway_url).map_err(MtrxError::invalid)?;
        let pusher = core::HttpPusher {
            gateway_url,
            pushkey,
            app_id,
            app_name,
            device_name,
            lang,
        };
        let session = self.core();
        run(async move { Ok(session.set_pusher(pusher).await?) }).await
    }

    /// Stop the pushes to `pushkey`; succeeds if there are none.
    pub async fn delete_pusher(&self, pushkey: String) -> Result<(), MtrxError> {
        let session = self.core();
        run(async move { Ok(session.delete_pusher(&pushkey).await?) }).await
    }

    /// The account's pushers, of all its devices.
    pub async fn pushers(&self) -> Result<Vec<PusherInfo>, MtrxError> {
        let session = self.core();
        run(async move {
            Ok(session
                .pushers()
                .await?
                .into_iter()
                .map(PusherInfo::from)
                .collect())
        })
        .await
    }
}

/// See [`crate::Mtrx::notification`].
pub(crate) async fn notification(
    profile: core::Profile,
    room_id: String,
    event_id: String,
) -> Result<Option<NotificationContent>, MtrxError> {
    let room_id = OwnedRoomId::try_from(room_id.as_str()).map_err(MtrxError::invalid)?;
    let event_id = core::OwnedEventId::try_from(event_id.as_str()).map_err(MtrxError::invalid)?;
    run(async move {
        let notification = core::PushNotification::fetch(&profile, &room_id, &event_id).await?;
        Ok(notification.map(NotificationContent::from))
    })
    .await
}
