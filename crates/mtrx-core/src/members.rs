//! The people in a room.

use std::collections::HashSet;

use matrix_sdk::{
    RoomMemberships,
    ruma::{
        OwnedMxcUri, OwnedUserId, RoomId, UserId,
        events::room::member::MembershipState,
        events::room::power_levels::{RoomPowerLevels, UserPowerLevel},
        int,
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
    /// What we may do to them.
    pub manage: Manage,
}

/// What we may do to a member, by the room's power levels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manage {
    pub remove: bool,
    pub ban: bool,
    /// The roles we may give them, besides theirs.
    pub roles: Vec<Role>,
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
        let levels = crate::permissions::power_levels(&room).await.ok();
        let own = self.user_id();
        let mut members: Vec<Member> = room
            .members(RoomMemberships::JOIN | RoomMemberships::INVITE)
            .await?
            .into_iter()
            .map(|member| {
                let id = member.user_id().to_owned();
                let role = role(member.power_level());
                let manage = match &levels {
                    Some(levels) if id != own => manage(levels, &own, &id, role),
                    _ => Manage::default(),
                };
                Member {
                    name: member.display_name().map(ToOwned::to_owned),
                    avatar: member.avatar_url().map(ToOwned::to_owned),
                    role,
                    invited: *member.membership() == MembershipState::Invite,
                    manage,
                    id,
                }
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

    /// Remove someone from a room (kick), or also keep them out (ban).
    pub async fn remove_member(
        &self,
        room_id: &RoomId,
        user: &UserId,
        reason: Option<&str>,
        ban: bool,
    ) -> Result<()> {
        let room = self.room(room_id)?;
        if ban {
            room.ban_user(user, reason).await?;
        } else {
            room.kick_user(user, reason).await?;
        }
        Ok(())
    }

    /// Make someone an admin, moderator or plain member of a room.
    pub async fn set_role(&self, room_id: &RoomId, user: &UserId, role: Role) -> Result<()> {
        let room = self.room(room_id)?;
        let level = match role {
            Role::Admin => int!(100),
            Role::Moderator => int!(50),
            Role::Member => crate::permissions::power_levels(&room).await?.users_default,
        };
        room.update_power_levels(vec![(user, level)]).await?;
        Ok(())
    }
}

/// What `own` may do to `target`, whose role is `role`.
fn manage(levels: &RoomPowerLevels, own: &UserId, target: &UserId, role: Role) -> Manage {
    let ours = self::role(levels.for_user(own));
    let roles = if levels.user_can_change_user_power_level(own, target) {
        // At most our own: an admin makes admins, a moderator moderators.
        [Role::Admin, Role::Moderator, Role::Member]
            .into_iter()
            .filter(|r| *r != role && *r >= ours)
            .collect()
    } else {
        Vec::new()
    };
    Manage {
        remove: levels.user_can_kick_user(own, target),
        ban: levels.user_can_ban_user(own, target),
        roles,
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
