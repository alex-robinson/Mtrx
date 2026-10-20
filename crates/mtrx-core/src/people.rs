//! The people the user knows: whom they share spaces, rooms and direct
//! chats with. For suggesting people and showing who someone is.

use std::collections::HashMap;

use matrix_sdk::{
    Room, RoomMemberships,
    ruma::{OwnedRoomId, OwnedUserId, UserId},
};
use tracing::warn;

use crate::{Result, Session, UserSummary};

/// Someone the user shares spaces, rooms or a direct chat with.
#[derive(Clone, Debug, PartialEq)]
pub struct Contact {
    pub user: UserSummary,
    /// Joined spaces both are in.
    pub spaces: Vec<OwnedRoomId>,
    /// Joined rooms (not spaces) both are in, direct chats included.
    pub rooms: Vec<OwnedRoomId>,
    /// We have a direct chat with them.
    pub direct: bool,
}

impl Contact {
    /// Higher for people the user has more to do with.
    fn closeness(&self) -> usize {
        self.spaces.len() * 4 + self.rooms.len() + usize::from(self.direct) * 6
    }
}

/// Who someone is, for their info card.
#[derive(Clone, Debug, PartialEq)]
pub struct UserInfo {
    pub user: UserSummary,
    /// Spaces and rooms shared with them, as far as known.
    pub shared: Vec<OwnedRoomId>,
}

impl Session {
    /// The people the user shares spaces, rooms or direct chats with (joined
    /// or invited),
    /// closest first. Loads the member lists of spaces from the server once;
    /// rooms count with the members known locally.
    pub async fn contacts(&self) -> Result<Vec<Contact>> {
        let own = self.user_id();
        let mut contacts: HashMap<OwnedUserId, Contact> = HashMap::new();
        for room in self.client.joined_rooms() {
            let space = room.is_space();
            let direct = room.is_direct().await.unwrap_or(false);
            // Invited people count: the user chose them.
            let memberships = RoomMemberships::JOIN | RoomMemberships::INVITE;
            let members = if space {
                room.members(memberships).await
            } else {
                room.members_no_sync(memberships).await
            };
            let members = match members {
                Ok(members) => members,
                Err(e) => {
                    warn!(room = %room.room_id(), "could not read members: {e}");
                    continue;
                }
            };
            for member in members.into_iter().filter(|m| m.user_id() != own) {
                let contact = contacts
                    .entry(member.user_id().to_owned())
                    .or_insert_with(|| Contact {
                        user: UserSummary {
                            id: member.user_id().to_owned(),
                            name: member.display_name().map(ToOwned::to_owned),
                            avatar: member.avatar_url().map(ToOwned::to_owned),
                        },
                        spaces: Vec::new(),
                        rooms: Vec::new(),
                        direct: false,
                    });
                if space {
                    contact.spaces.push(room.room_id().to_owned());
                } else {
                    contact.rooms.push(room.room_id().to_owned());
                }
                contact.direct |= direct;
            }
        }
        let mut contacts: Vec<_> = contacts.into_values().collect();
        contacts.sort_by_cached_key(|c| {
            let name = c.user.name.clone().unwrap_or_default().to_lowercase();
            (std::cmp::Reverse(c.closeness()), name)
        });
        Ok(contacts)
    }

    /// A user's name and picture from their profile, and what we share.
    pub async fn user_info(&self, user: &UserId) -> Result<UserInfo> {
        let profile = self.client.account().fetch_user_profile_of(user).await;
        let (name, avatar) = match profile {
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
        };
        let mut shared = Vec::new();
        for room in self.client.joined_rooms() {
            if is_member(&room, user).await {
                shared.push(room.room_id().to_owned());
            }
        }
        Ok(UserInfo {
            user: UserSummary {
                id: user.to_owned(),
                name,
                avatar,
            },
            shared,
        })
    }
}

/// Joined member of `room`, by the members known locally (spaces load
/// theirs from the server once).
async fn is_member(room: &Room, user: &UserId) -> bool {
    let member = if room.is_space() {
        room.get_member(user).await
    } else {
        room.get_member_no_sync(user).await
    };
    member.ok().flatten().is_some_and(|m| {
        *m.membership() == matrix_sdk::ruma::events::room::member::MembershipState::Join
    })
}
