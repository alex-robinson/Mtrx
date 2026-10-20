//! A room's settings as the user sees them: what they may change, and when
//! it notifies them.

use std::path::Path;

use matrix_sdk::{
    Room,
    notification_settings::{IsEncrypted, IsOneToOne, RoomNotificationMode},
    ruma::{RoomId, events::StateEventType},
};

use crate::{Error, Result, Session, media::guess_mimetype, permissions::power_levels};

/// When a room notifies (push rules, so on all the user's devices).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyMode {
    All,
    /// Only for mentions and keywords.
    Mentions,
    Mute,
}

impl From<RoomNotificationMode> for NotifyMode {
    fn from(mode: RoomNotificationMode) -> Self {
        match mode {
            RoomNotificationMode::AllMessages => Self::All,
            RoomNotificationMode::MentionsAndKeywordsOnly => Self::Mentions,
            RoomNotificationMode::Mute => Self::Mute,
        }
    }
}

impl From<NotifyMode> for RoomNotificationMode {
    fn from(mode: NotifyMode) -> Self {
        match mode {
            NotifyMode::All => Self::AllMessages,
            NotifyMode::Mentions => Self::MentionsAndKeywordsOnly,
            NotifyMode::Mute => Self::Mute,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomSettings {
    pub can_rename: bool,
    pub can_change_topic: bool,
    pub can_change_picture: bool,
    pub can_invite: bool,
    pub notify: NotifyMode,
}

impl Session {
    pub async fn room_settings(&self, room_id: &RoomId) -> Result<RoomSettings> {
        let room = self.room(room_id)?;
        let levels = power_levels(&room).await?;
        let own = self.user_id();
        let can = |state| levels.user_can_send_state(&own, state);
        Ok(RoomSettings {
            can_rename: can(StateEventType::RoomName),
            can_change_topic: can(StateEventType::RoomTopic),
            can_change_picture: can(StateEventType::RoomAvatar),
            can_invite: levels.user_can_invite(&own),
            notify: self.notify_mode(&room).await.into(),
        })
    }

    /// The room's own mode, else the default for rooms like it.
    async fn notify_mode(&self, room: &Room) -> RoomNotificationMode {
        let settings = &self.notification_settings;
        if let Some(mode) = settings
            .get_user_defined_room_notification_mode(room.room_id())
            .await
        {
            return mode;
        }
        let encrypted = room.encryption_state().is_encrypted();
        // Two people, as push rules count them.
        let one_to_one = room.active_members_count() == 2;
        settings
            .get_default_room_notification_mode(
                IsEncrypted::from(encrypted),
                IsOneToOne::from(one_to_one),
            )
            .await
    }

    pub async fn set_notify_mode(&self, room_id: &RoomId, mode: NotifyMode) -> Result<()> {
        self.notification_settings
            .set_room_notification_mode(room_id, mode.into())
            .await
            .map_err(|e| Error::Notifications(e.to_string()))
    }

    /// Make the image at `path` the room's picture.
    pub async fn set_room_picture(&self, room_id: &RoomId, path: &Path) -> Result<()> {
        let room = self.room(room_id)?;
        let mimetype = guess_mimetype(&path.to_string_lossy());
        if mimetype.type_() != mime_guess::mime::IMAGE {
            return Err(Error::InvalidMediaType(mimetype.to_string()));
        }
        let data = tokio::fs::read(path).await?;
        room.upload_avatar(&mimetype, data, None).await?;
        Ok(())
    }

    pub async fn remove_room_picture(&self, room_id: &RoomId) -> Result<()> {
        self.room(room_id)?.remove_avatar().await?;
        Ok(())
    }
}
