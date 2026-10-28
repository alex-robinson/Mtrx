//! The people in a room.

use std::collections::HashSet;

use matrix_sdk::{
    RoomMemberships,
    ruma::{
        OwnedMxcUri, OwnedUserId, RoomId, events::room::member::MembershipState,
        events::room::power_levels::UserPowerLevel, int,
    },
};

use crate::{Result, Session};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub id: OwnedUserId,
    pub name: Option<String>,
    pub avatar: Option<OwnedMxcUri>,
    pub role: Role,
    /// Invited, not joined yet.
    pub invited: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Admin,
    Moderator,
    Member,
}

impl Session {
    /// Joined and invited members, admins first, then by name. Loads the
    /// member list from the server if it is not complete yet.
    pub async fn members(&self, room_id: &RoomId) -> Result<Vec<Member>> {
        let room = self.room(room_id)?;
        let mut members: Vec<Member> = room
            .members(RoomMemberships::JOIN | RoomMemberships::INVITE)
            .await?
            .into_iter()
            .map(|member| Member {
                id: member.user_id().to_owned(),
                name: member.display_name().map(ToOwned::to_owned),
                avatar: member.avatar_url().map(ToOwned::to_owned),
                role: role(member.power_level()),
                invited: *member.membership() == MembershipState::Invite,
            })
            .collect();
        members.sort_by_cached_key(|m| {
            let name = m
                .name
                .clone()
                .unwrap_or_else(|| m.id.localpart().to_owned());
            (m.invited, m.role, name.to_lowercase())
        });
        Ok(members)
    }

    /// The IDs of the joined members. Loads the member list from the server
    /// if it is not complete yet.
    pub async fn joined_member_ids(&self, room_id: &RoomId) -> Result<HashSet<OwnedUserId>> {
        let room = self.room(room_id)?;
        Ok(room
            .members(RoomMemberships::JOIN)
            .await?
            .iter()
            .map(|member| member.user_id().to_owned())
            .collect())
    }
}

pub(crate) fn role(level: UserPowerLevel) -> Role {
    match level {
        UserPowerLevel::Infinite => Role::Admin,
        UserPowerLevel::Int(level) if level >= int!(100) => Role::Admin,
        UserPowerLevel::Int(level) if level >= int!(50) => Role::Moderator,
        _ => Role::Member,
    }
}
