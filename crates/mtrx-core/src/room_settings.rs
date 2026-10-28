//! A room's settings as the user sees them: what they may change, and when
//! it notifies them.

use std::{path::Path, time::Duration};

use matrix_sdk::{
    Room, RoomMemberships,
    deserialized_responses::SyncOrStrippedState,
    notification_settings::{IsEncrypted, IsOneToOne, RoomNotificationMode},
    ruma::{
        OwnedRoomAliasId, OwnedRoomId, OwnedUserId, RoomAliasId, RoomId, UserId,
        api::client::{
            directory::{get_room_visibility, set_room_visibility},
            room::Visibility,
        },
        events::{
            StateEventType, SyncStateEvent,
            room::{
                canonical_alias::RoomCanonicalAliasEventContent,
                history_visibility::{HistoryVisibility, RoomHistoryVisibilityEventContent},
                join_rules::{AllowRule, JoinRule, RoomJoinRulesEventContent},
            },
            space::child::SpaceChildEventContent,
        },
    },
};
use tokio::sync::broadcast::error::RecvError;

use crate::{
    Error, Permissions, Result, Session,
    media::guess_mimetype,
    permissions::{everyone_can_edit, power_levels},
    rooms::space_children,
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
    /// Anyone may ask to join; members let them in.
    Knock,
    /// Anyone.
    Public,
}

impl From<Option<JoinRule>> for Access {
    fn from(rule: Option<JoinRule>) -> Self {
        match rule {
            Some(JoinRule::Public) => Self::Public,
            Some(JoinRule::Restricted(_) | JoinRule::KnockRestricted(_)) => Self::SpaceMembers,
            Some(JoinRule::Knock) => Self::Knock,
            _ => Self::Invite,
        }
    }
}

/// Who may read a room's history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum History {
    /// Members, from when they joined.
    Joined,
    /// Members, from when they were invited.
    Invited,
    /// Members, all of it.
    Members,
    /// Anyone, members or not.
    Anyone,
}

impl From<HistoryVisibility> for History {
    fn from(visibility: HistoryVisibility) -> Self {
        match visibility {
            HistoryVisibility::Joined => Self::Joined,
            HistoryVisibility::Invited => Self::Invited,
            HistoryVisibility::WorldReadable => Self::Anyone,
            _ => Self::Members,
        }
    }
}

impl From<History> for HistoryVisibility {
    fn from(history: History) -> Self {
        match history {
            History::Joined => Self::Joined,
            History::Invited => Self::Invited,
            History::Members => Self::Shared,
            History::Anyone => Self::WorldReadable,
        }
    }
}

/// A joined space a room is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceLink {
    pub space: OwnedRoomId,
    /// Suggested to the space's members.
    pub suggested: bool,
    /// We may change the space's rooms.
    pub can_change: bool,
}

/// Someone banned from a room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Banned {
    pub id: OwnedUserId,
    pub name: Option<String>,
    pub reason: Option<String>,
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
    /// For a space: may list rooms in it.
    pub can_add_rooms: bool,
    /// The address people join by.
    pub address: Option<OwnedRoomAliasId>,
    pub can_change_address: bool,
    /// Listed in our server's room directory; `None` if it wouldn't say.
    pub listed: Option<bool>,
    pub history: History,
    pub can_change_history: bool,
    pub permissions: Permissions,
    pub can_change_permissions: bool,
    pub encrypted: bool,
    pub can_encrypt: bool,
    /// The room version, as "12".
    pub version: Option<String>,
    /// The joined spaces listing the room.
    pub spaces: Vec<SpaceLink>,
    /// Joined spaces not listing it whose rooms we may change.
    pub can_add_to: Vec<OwnedRoomId>,
    pub banned: Vec<Banned>,
    pub can_unban: bool,
}

impl Session {
    pub async fn room_settings(&self, room_id: &RoomId) -> Result<RoomSettings> {
        let room = self.room(room_id)?;
        let levels = power_levels(&room).await?;
        let own = self.user_id();
        let can = |state| levels.user_can_send_state(&own, state);
        let listed = self
            .client
            .send(get_room_visibility::v3::Request::new(room_id.to_owned()))
            .await
            .ok()
            .map(|response| response.visibility == Visibility::Public);
        let banned = room
            .members_no_sync(RoomMemberships::BAN)
            .await?
            .into_iter()
            .map(|member| Banned {
                id: member.user_id().to_owned(),
                name: member.display_name().map(ToOwned::to_owned),
                reason: member.event().reason().map(ToOwned::to_owned),
            })
            .collect();
        let (spaces, can_add_to) = self.space_links(room_id).await;
        Ok(RoomSettings {
            can_rename: can(StateEventType::RoomName),
            can_change_topic: can(StateEventType::RoomTopic),
            can_change_picture: can(StateEventType::RoomAvatar),
            can_invite: levels.user_can_invite(&own),
            access: room.join_rule().into(),
            can_change_access: can(StateEventType::RoomJoinRules),
            can_add_rooms: can(StateEventType::SpaceChild),
            notify: self.notify_mode(&room).await.into(),
            address: room.canonical_alias(),
            can_change_address: can(StateEventType::RoomCanonicalAlias),
            listed,
            history: room.history_visibility_or_default().into(),
            can_change_history: can(StateEventType::RoomHistoryVisibility),
            permissions: if everyone_can_edit(&levels) {
                Permissions::Everyone
            } else {
                Permissions::Moderators
            },
            can_change_permissions: can(StateEventType::RoomPowerLevels),
            encrypted: room.encryption_state().is_encrypted(),
            can_encrypt: can(StateEventType::RoomEncryption),
            version: room.version().map(|v| v.to_string()),
            spaces,
            can_add_to,
            banned,
            can_unban: levels.user_can_ban(&own),
        })
    }

    /// The joined spaces listing the room, and those we may add it to.
    async fn space_links(&self, room_id: &RoomId) -> (Vec<SpaceLink>, Vec<OwnedRoomId>) {
        let own = self.user_id();
        let (mut links, mut others) = (Vec::new(), Vec::new());
        for space in self.client.joined_rooms() {
            if !space.is_space() || space.room_id() == room_id {
                continue;
            }
            let can_change = power_levels(&space)
                .await
                .is_ok_and(|levels| levels.user_can_send_state(&own, StateEventType::SpaceChild));
            match child_link(&space, room_id).await {
                Some(link) => links.push(SpaceLink {
                    space: space.room_id().to_owned(),
                    suggested: link.suggested,
                    can_change,
                }),
                None if can_change => others.push(space.room_id().to_owned()),
                None => {}
            }
        }
        (links, others)
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
            Access::Knock => JoinRule::Knock,
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

    /// Give the room the address `#name:our.server`, or none.
    pub async fn set_address(&self, room_id: &RoomId, name: Option<&str>) -> Result<()> {
        let room = self.room(room_id)?;
        let alias = match name {
            Some(name) => {
                let alias = RoomAliasId::parse(format!("#{name}:{}", self.user_id().server_name()))
                    .map_err(|_| Error::Permissions(format!("“{name}” can't be an address")))?;
                // Ours already if it was before; taken, the server says so.
                let ours = self
                    .client
                    .resolve_room_alias(&alias)
                    .await
                    .is_ok_and(|r| r.room_id == room_id);
                if !ours {
                    self.client.create_room_alias(&alias, room_id).await?;
                }
                Some(alias)
            }
            None => None,
        };
        let mut content = RoomCanonicalAliasEventContent::new();
        content.alias = alias;
        content.alt_aliases = room.alt_aliases();
        room.send_state_event(content).await?;
        Ok(())
    }

    /// List the room in our server's directory, or not.
    pub async fn set_listed(&self, room_id: &RoomId, listed: bool) -> Result<()> {
        let visibility = if listed {
            Visibility::Public
        } else {
            Visibility::Private
        };
        self.client
            .send(set_room_visibility::v3::Request::new(
                room_id.to_owned(),
                visibility,
            ))
            .await?;
        Ok(())
    }

    pub async fn set_history(&self, room_id: &RoomId, history: History) -> Result<()> {
        self.room(room_id)?
            .send_state_event(RoomHistoryVisibilityEventContent::new(history.into()))
            .await?;
        Ok(())
    }

    /// Encrypt the room's messages from now on; not to be undone.
    pub async fn encrypt_room(&self, room_id: &RoomId) -> Result<()> {
        self.room(room_id)?.enable_encryption().await?;
        Ok(())
    }

    pub async fn unban(&self, room_id: &RoomId, user: &UserId) -> Result<()> {
        self.room(room_id)?.unban_user(user, None).await?;
        Ok(())
    }

    /// Suggest the room to the space's members, or not.
    pub async fn set_suggested(
        &self,
        space_id: &RoomId,
        room_id: &RoomId,
        suggested: bool,
    ) -> Result<()> {
        let space = self.room(space_id)?;
        let mut content = child_link(&space, room_id)
            .await
            .ok_or_else(|| Error::RoomNotFound(room_id.to_string()))?;
        content.suggested = suggested;
        space
            .send_state_event_for_key(&room_id.to_owned(), content)
            .await?;
        Ok(())
    }

    /// Take the room out of the space: the space no longer lists it.
    pub async fn remove_from_space(&self, space_id: &RoomId, room_id: &RoomId) -> Result<()> {
        self.room(space_id)?
            .send_state_event_for_key(&room_id.to_owned(), SpaceChildEventContent::new(Vec::new()))
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

/// The space's link to a room, if it lists it.
async fn child_link(space: &Room, room_id: &RoomId) -> Option<SpaceChildEventContent> {
    let raw = space
        .get_state_event_static_for_key::<SpaceChildEventContent, _>(room_id)
        .await
        .ok()??;
    match raw.deserialize().ok()? {
        SyncOrStrippedState::Sync(SyncStateEvent::Original(event))
            if !event.content.via.is_empty() =>
        {
            Some(event.content)
        }
        SyncOrStrippedState::Stripped(event) => {
            let mut content =
                SpaceChildEventContent::new(event.content.via.filter(|v| !v.is_empty())?);
            content.suggested = event.content.suggested;
            content.order = event.content.order;
            Some(content)
        }
        _ => None,
    }
}
