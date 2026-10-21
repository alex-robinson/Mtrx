//! A room's settings as the user sees them: what they may change, and when
//! it notifies them.

use std::{path::Path, time::Duration};

use matrix_sdk::{
    Room,
    notification_settings::{IsEncrypted, IsOneToOne, RoomNotificationMode},
    ruma::{
        OwnedRoomId, RoomId,
        events::{
            StateEventType,
            room::join_rules::{AllowRule, JoinRule, RoomJoinRulesEventContent},
        },
    },
};
use tokio::sync::broadcast::error::RecvError;

use crate::{
    Error, Result, Session, media::guess_mimetype, permissions::power_levels, rooms::space_children,
};

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

/// Who may join a room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Only invited people.
    Invite,
    /// Members of the spaces the room is in (and invited people).
    SpaceMembers,
    /// Anyone.
    Public,
}

impl From<Option<JoinRule>> for Access {
    fn from(rule: Option<JoinRule>) -> Self {
        match rule {
            Some(JoinRule::Public) => Self::Public,
            Some(JoinRule::Restricted(_) | JoinRule::KnockRestricted(_)) => Self::SpaceMembers,
            _ => Self::Invite,
        }
    }
}

/// How long a changed mode may take to come back through sync.
const CONFIRM: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomSettings {
    pub can_rename: bool,
    pub can_change_topic: bool,
    pub can_change_picture: bool,
    pub can_invite: bool,
    pub notify: NotifyMode,
    pub access: Access,
    pub can_change_access: bool,
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
            access: room.join_rule().into(),
            can_change_access: can(StateEventType::RoomJoinRules),
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

    /// Change when a room notifies, returning once the push rules that sync
    /// brings back give it that mode (or after a while): a further change
    /// then starts from the server's rules, not from older ones still on
    /// their way.
    ///
    /// The rules are read from `notification_settings` after it applied a
    /// sync, not from the synced event itself: it handles the same event
    /// concurrently, and an older event from midway through the change may
    /// still arrive after the change was applied locally.
    pub async fn set_notify_mode(&self, room_id: &RoomId, mode: NotifyMode) -> Result<()> {
        let settings = &self.notification_settings;
        let mode = RoomNotificationMode::from(mode);
        let has_mode = || async {
            settings
                .get_user_defined_room_notification_mode(room_id)
                .await
                == Some(mode)
        };
        if has_mode().await {
            // Nothing to change, so nothing comes back.
            return Ok(());
        }
        let mut synced = settings.subscribe_to_changes();
        settings
            .set_room_notification_mode(room_id, mode)
            .await
            .map_err(|e| Error::Notifications(e.to_string()))?;
        let confirmed = async {
            loop {
                match synced.recv().await {
                    Err(RecvError::Closed) => return,
                    // A change missed while lagging still left its rules.
                    _ if has_mode().await => return,
                    _ => {}
                }
            }
        };
        // The server took the change either way.
        let _ = tokio::time::timeout(CONFIRM, confirmed).await;
        Ok(())
    }

    /// Change who may join; [`Access::SpaceMembers`] lets in members of the
    /// joined spaces the room is in.
    pub async fn set_room_access(&self, room_id: &RoomId, access: Access) -> Result<()> {
        let room = self.room(room_id)?;
        let rule = match access {
            Access::Invite => JoinRule::Invite,
            Access::Public => JoinRule::Public,
            Access::SpaceMembers => {
                let spaces = self.parent_spaces(room_id).await;
                if spaces.is_empty() {
                    return Err(Error::Permissions("the room is in no space".into()));
                }
                RoomJoinRulesEventContent::restricted(
                    spaces.into_iter().map(AllowRule::room_membership).collect(),
                )
                .join_rule
            }
        };
        room.send_state_event(RoomJoinRulesEventContent::new(rule))
            .await?;
        Ok(())
    }

    /// Joined spaces listing `room_id` as a child.
    async fn parent_spaces(&self, room_id: &RoomId) -> Vec<OwnedRoomId> {
        let mut parents = Vec::new();
        for space in self
            .client
            .joined_rooms()
            .into_iter()
            .filter(|r| r.is_space())
        {
            if space_children(&space)
                .await
                .iter()
                .any(|child| child == room_id)
            {
                parents.push(space.room_id().to_owned());
            }
        }
        parents
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
