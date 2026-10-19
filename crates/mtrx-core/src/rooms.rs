//! Live room list (rooms, spaces, DMs) and room management.

use std::{collections::HashMap, sync::Arc, time::SystemTime};

use eyeball_im::Vector;
use futures_util::{Stream, StreamExt, pin_mut};
use matrix_sdk::{
    Client, Room, RoomState,
    deserialized_responses::SyncOrStrippedState,
    ruma::{
        OwnedMxcUri, OwnedRoomId, OwnedServerName, OwnedUserId, RoomId, RoomOrAliasId, UserId,
        api::client::room::create_room::v3::{
            CreationContent, Request as CreateRoomRequest, RoomPreset,
        },
        events::{
            InitialStateEvent, SyncStateEvent,
            room::encryption::RoomEncryptionEventContent,
            space::{child::SpaceChildEventContent, parent::SpaceParentEventContent},
        },
        room::RoomType,
        serde::Raw,
    },
};
use matrix_sdk_ui::room_list_service::{
    RoomListItem, RoomListLoadingState, RoomListService, State as ServiceState,
    filters::new_filter_non_left,
};
use tokio::{sync::watch, task::JoinHandle};
use tokio_stream::wrappers::WatchStream;
use tracing::warn;

use crate::{
    Error, Result, RoomTag, Session,
    permissions::{Permissions, open_power_levels_raw},
    system_time,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomKind {
    Room,
    Space,
    Direct,
}

/// What a frontend needs to show one entry of the room list.
#[derive(Clone, Debug, PartialEq)]
pub struct RoomSummary {
    pub id: OwnedRoomId,
    pub name: String,
    /// The room's picture; for a DM without one, the other user's.
    pub avatar: Option<OwnedMxcUri>,
    pub topic: Option<String>,
    pub kind: RoomKind,
    /// We are invited but have not joined yet.
    pub invited: bool,
    pub encrypted: bool,
    pub unread_messages: u64,
    pub unread_mentions: u64,
    pub unread_notifications: u64,
    pub last_activity: Option<SystemTime>,
    /// Joined spaces that list this room as a child.
    pub parents: Vec<OwnedRoomId>,
    pub tag: RoomTag,
    /// For a direct chat, the other user.
    pub direct_user: Option<OwnedUserId>,
}

/// Snapshot published by [`RoomList`].
#[derive(Clone, Debug, Default)]
pub struct RoomListState {
    /// Joined and invited rooms, most recently active first.
    pub rooms: Vec<RoomSummary>,
    /// Synced with the server since the list was created, and every room the
    /// server reported is in `rooms`. Before that, `rooms` may be a stale cache.
    pub loaded: bool,
}

/// Keeps a [`RoomListState`] up to date while it is alive.
pub struct RoomList {
    state: watch::Receiver<RoomListState>,
    task: JoinHandle<()>,
}

impl Drop for RoomList {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl RoomList {
    pub(crate) async fn new(client: Client, service: Arc<RoomListService>) -> Result<Self> {
        let (tx, state) = watch::channel(RoomListState::default());
        let task = tokio::spawn(async move {
            if let Err(e) = run_room_list(client, service, tx).await {
                warn!("room list stopped: {e}");
            }
        });
        Ok(Self { state, task })
    }

    pub fn current(&self) -> RoomListState {
        self.state.borrow().clone()
    }

    /// The current state, then every change.
    pub fn updates(&self) -> impl Stream<Item = RoomListState> + use<> {
        WatchStream::new(self.state.clone())
    }

    /// Wait until [`RoomListState::loaded`]; requires sync to be running.
    pub async fn wait_loaded(&self) -> RoomListState {
        let mut state = self.state.clone();
        match state.wait_for(|s| s.loaded).await {
            Ok(s) => s.clone(),
            // Task ended (error is logged); return what we have.
            Err(_) => self.current(),
        }
    }
}

async fn run_room_list(
    client: Client,
    service: Arc<RoomListService>,
    tx: watch::Sender<RoomListState>,
) -> Result<()> {
    let list = service
        .all_rooms()
        .await
        .map_err(|e| Error::RoomList(e.to_string()))?;
    // One page that holds every room; the frontends do their own scrolling.
    let (entries, controller) = list.entries_with_dynamic_adapters(u32::MAX as usize);
    controller.set_filter(Box::new(new_filter_non_left()));
    // `loading` turns Loaded straight from the on-disk cache; `service_state`
    // only reaches Running after server responses in this process.
    let loading = list.loading_state();
    let mut loading_stream = loading.clone();
    let service_state = service.state();
    let mut service_state_stream = service_state.clone();
    // Space membership changes do not show up as room list diffs.
    let mut room_updates = client.subscribe_to_all_room_updates();
    pin_mut!(entries);

    let mut items: Vector<RoomListItem> = Vector::new();
    loop {
        tokio::select! {
            diffs = entries.next() => match diffs {
                Some(diffs) => diffs.into_iter().for_each(|d| d.apply(&mut items)),
                None => return Ok(()),
            },
            update = room_updates.recv() => {
                if let Err(tokio::sync::broadcast::error::RecvError::Closed) = update {
                    return Ok(());
                }
            }
            Some(_) = loading_stream.next() => {}
            Some(_) = service_state_stream.next() => {}
        }

        let synced = matches!(service_state.get(), ServiceState::Running);
        let loaded = synced
            && match loading.get() {
                RoomListLoadingState::NotLoaded => false,
                RoomListLoadingState::Loaded {
                    maximum_number_of_rooms,
                } => items.len() >= maximum_number_of_rooms.unwrap_or(0) as usize,
            };
        let rooms = summarize(&items).await;
        tx.send_replace(RoomListState { rooms, loaded });
    }
}

async fn summarize(items: &Vector<RoomListItem>) -> Vec<RoomSummary> {
    let mut parents: HashMap<OwnedRoomId, Vec<OwnedRoomId>> = HashMap::new();
    for space in items
        .iter()
        .filter(|r| r.is_space() && r.state() == RoomState::Joined)
    {
        for child in space_children(space).await {
            parents
                .entry(child)
                .or_default()
                .push(space.room_id().to_owned());
        }
    }

    let mut rooms = Vec::with_capacity(items.len());
    for room in items.iter() {
        let kind = if room.is_space() {
            RoomKind::Space
        } else if room.is_direct().await.unwrap_or(false) {
            RoomKind::Direct
        } else {
            RoomKind::Room
        };
        let tag = if room.is_favourite() {
            RoomTag::Favourite
        } else if room.is_low_priority() {
            RoomTag::LowPriority
        } else {
            RoomTag::None
        };
        let direct_user = match kind {
            RoomKind::Direct => room
                .direct_targets()
                .into_iter()
                .find_map(|t| t.into_user_id()),
            _ => None,
        };
        let avatar = match (room.avatar_url(), kind) {
            (None, RoomKind::Direct) => direct_avatar(room).await,
            (avatar, _) => avatar,
        };
        rooms.push(RoomSummary {
            id: room.room_id().to_owned(),
            name: room
                .cached_display_name()
                .map(|n| n.to_string())
                .unwrap_or_else(|| room.room_id().to_string()),
            avatar,
            topic: room.topic().filter(|t| !t.trim().is_empty()),
            kind,
            invited: room.state() == RoomState::Invited,
            encrypted: room.encryption_state().is_encrypted(),
            unread_messages: room.num_unread_messages(),
            unread_mentions: room.num_unread_mentions(),
            unread_notifications: room.num_unread_notifications(),
            last_activity: room.latest_event_timestamp().map(system_time),
            parents: parents.remove(room.room_id()).unwrap_or_default(),
            tag,
            direct_user,
        });
    }
    rooms
}

/// The other user's avatar in a DM: from the sync's room heroes, else from
/// their member event in the store.
async fn direct_avatar(room: &Room) -> Option<OwnedMxcUri> {
    let target = room
        .direct_targets()
        .into_iter()
        .find_map(|t| t.into_user_id())?;
    if let Some(hero) = room
        .heroes()
        .await
        .into_iter()
        .find(|h| h.user_id == target)
    {
        return hero.avatar_url;
    }
    match room.get_member_no_sync(&target).await {
        Ok(member) => member?.avatar_url().map(ToOwned::to_owned),
        Err(e) => {
            warn!(room = %room.room_id(), "could not read DM member: {e}");
            None
        }
    }
}

/// Children listed by `m.space.child` events. Events without `via` mean the
/// child was removed.
pub(crate) async fn space_children(space: &Room) -> Vec<OwnedRoomId> {
    let events = match space
        .get_state_events_static::<SpaceChildEventContent>()
        .await
    {
        Ok(events) => events,
        Err(e) => {
            warn!(space = %space.room_id(), "could not read m.space.child events: {e}");
            return Vec::new();
        }
    };
    events
        .into_iter()
        .filter_map(|raw| match raw.deserialize() {
            Ok(SyncOrStrippedState::Sync(SyncStateEvent::Original(e)))
                if !e.content.via.is_empty() =>
            {
                Some(e.state_key)
            }
            Ok(SyncOrStrippedState::Stripped(e))
                if e.content.via.as_ref().is_some_and(|v| !v.is_empty()) =>
            {
                Some(e.state_key)
            }
            _ => None,
        })
        .collect()
}

/// Options for [`Session::create_room`].
#[derive(Clone, Debug, Default)]
pub struct NewRoom {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub space: bool,
    pub encrypted: bool,
    pub invite: Vec<OwnedUserId>,
    pub direct: bool,
    pub permissions: Permissions,
    /// Space to create the room in.
    pub parent: Option<OwnedRoomId>,
}

/// A user found by [`Session::search_users`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserSummary {
    pub id: OwnedUserId,
    pub name: Option<String>,
    pub avatar: Option<OwnedMxcUri>,
}

impl Session {
    /// A live room list. Sync must be running for it to fill.
    pub async fn room_list(&self) -> Result<RoomList> {
        RoomList::new(self.client.clone(), self.sync.room_list_service()).await
    }

    pub async fn create_room(&self, new: NewRoom) -> Result<OwnedRoomId> {
        let mut request = CreateRoomRequest::new();
        request.name = new.name;
        request.topic = new.topic;
        request.invite = new.invite;
        request.is_direct = new.direct;
        request.preset = Some(if new.direct {
            RoomPreset::TrustedPrivateChat
        } else {
            RoomPreset::PrivateChat
        });
        // Direct chats make both people admins anyway.
        if new.permissions == Permissions::Everyone && !new.direct {
            request.power_level_content_override = Some(open_power_levels_raw()?);
        }
        if new.space {
            let mut creation = CreationContent::new();
            creation.room_type = Some(RoomType::Space);
            request.creation_content = Some(Raw::new(&creation)?);
        }
        if new.encrypted {
            let event = InitialStateEvent::with_empty_state_key(
                RoomEncryptionEventContent::with_recommended_defaults(),
            );
            request.initial_state.push(Raw::new(&event)?.cast());
        }
        if let Some(parent) = &new.parent {
            let mut content = SpaceParentEventContent::new(self.via());
            content.canonical = true;
            let event = InitialStateEvent::new(parent.clone(), content);
            request.initial_state.push(Raw::new(&event)?.cast());
        }
        let room = self.client.create_room(request).await?.room_id().to_owned();
        if let Some(parent) = &new.parent {
            self.add_to_space(parent, &room).await?;
        }
        Ok(room)
    }

    /// The direct chat with `user`: the existing one, or a new encrypted one.
    pub async fn direct_chat(&self, user: &UserId) -> Result<OwnedRoomId> {
        let room = match self.client.get_dm_room(user) {
            Some(room) => room,
            None => self.client.create_dm(user).await?,
        };
        Ok(room.room_id().to_owned())
    }

    pub async fn set_room_name(&self, room: &RoomId, name: &str) -> Result<()> {
        self.room(room)?.set_name(name.to_owned()).await?;
        Ok(())
    }

    pub async fn set_room_topic(&self, room: &RoomId, topic: &str) -> Result<()> {
        self.room(room)?.set_room_topic(topic).await?;
        Ok(())
    }

    /// Users matching `term` by name or ID in the server's user directory
    /// (people sharing a room with us, or everyone if the server allows).
    pub async fn search_users(&self, term: &str, limit: u64) -> Result<Vec<UserSummary>> {
        let response = self.client.search_users(term, limit).await?;
        Ok(response
            .results
            .into_iter()
            .map(|user| UserSummary {
                id: user.user_id,
                name: user.display_name,
                avatar: user.avatar_url,
            })
            .collect())
    }

    /// Servers to name in space links: ours.
    fn via(&self) -> Vec<OwnedServerName> {
        vec![self.user_id().server_name().to_owned()]
    }

    /// Join by room ID or alias, or accept an invite.
    pub async fn join(&self, room: &RoomOrAliasId) -> Result<OwnedRoomId> {
        Ok(self
            .client
            .join_room_by_id_or_alias(room, &[])
            .await?
            .room_id()
            .to_owned())
    }

    pub async fn leave(&self, room: &OwnedRoomId) -> Result<()> {
        Ok(self.room(room)?.leave().await?)
    }

    pub async fn invite(&self, room: &OwnedRoomId, user: &OwnedUserId) -> Result<()> {
        Ok(self.room(room)?.invite_user_by_id(user).await?)
    }

    /// Add `child` to `space` (sends `m.space.child` in the space).
    pub async fn add_to_space(&self, space: &OwnedRoomId, child: &OwnedRoomId) -> Result<()> {
        self.room(space)?
            .send_state_event_for_key(child, SpaceChildEventContent::new(self.via()))
            .await?;
        Ok(())
    }

    /// Find a known room by ID, alias or exact display name.
    ///
    /// Names are matched against rooms in the local store, so a fresh
    /// profile needs to have synced first.
    pub async fn find_room(&self, query: &str) -> Result<OwnedRoomId> {
        if let Ok(id) = <&RoomOrAliasId>::try_from(query) {
            match OwnedRoomId::try_from(id.as_str()) {
                Ok(room_id) => return Ok(room_id),
                Err(_) => {
                    let alias = id
                        .as_str()
                        .try_into()
                        .expect("alias is valid if not a room ID");
                    return Ok(self.client.resolve_room_alias(alias).await?.room_id);
                }
            }
        }
        let matches: Vec<_> = self
            .client
            .rooms()
            .into_iter()
            .filter(|r| r.state() != RoomState::Left)
            .filter(|r| {
                r.cached_display_name()
                    .is_some_and(|n| n.to_string() == query)
            })
            .map(|r| r.room_id().to_owned())
            .collect();
        match matches.as_slice() {
            [id] => Ok(id.clone()),
            [] => Err(Error::RoomNotFound(query.to_owned())),
            _ => Err(Error::AmbiguousRoom(query.to_owned(), matches)),
        }
    }

    pub(crate) fn room(&self, id: &RoomId) -> Result<Room> {
        self.client
            .get_room(id)
            .ok_or_else(|| Error::RoomNotFound(id.to_string()))
    }
}
