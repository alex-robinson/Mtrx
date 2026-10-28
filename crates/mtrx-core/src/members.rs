//! The people in a room.

use std::collections::HashSet;

use matrix_sdk::{
    RoomMemberships,
    ruma::{
        Int, OwnedMxcUri, OwnedUserId, RoomId, UserId,
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
    /// Their power level; `None` for a creator of the room, above all levels.
    pub level: Option<i64>,
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
    /// The highest power level we may give them, if we may change theirs:
    /// ours, or below it for ourselves.
    pub up_to: Option<i64>,
    /// A plain member's level in the room.
    pub default_level: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Admin,
    Moderator,
    Member,
}

impl Role {
    /// The power level the role stands for; `default` for a member.
    pub fn level(self, default: i64) -> i64 {
        match self {
            Role::Admin => 100,
            Role::Moderator => 50,
            Role::Member => default,
        }
    }
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
                let level = member.power_level();
                let manage = match &levels {
                    Some(levels) => manage(levels, &own, &id),
                    None => Manage::default(),
                };
                Member {
                    name: member.display_name().map(ToOwned::to_owned),
                    avatar: member.avatar_url().map(ToOwned::to_owned),
                    role: role(level),
                    level: match level {
                        UserPowerLevel::Int(level) => Some(level.into()),
                        _ => None,
                    },
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

    /// Give someone, or ourselves, a power level in a room.
    pub async fn set_level(&self, room_id: &RoomId, user: &UserId, level: i64) -> Result<()> {
        let room = self.room(room_id)?;
        room.update_power_levels(vec![(user, Int::new_saturating(level))])
            .await?;
        Ok(())
    }
}

/// What `own` may do to `target` (perhaps themselves).
fn manage(levels: &RoomPowerLevels, own: &UserId, target: &UserId) -> Manage {
    let default_level = levels.users_default.into();
    let ours = match levels.for_user(own) {
        UserPowerLevel::Int(level) => i64::from(level),
        // A creator gives any level.
        _ => i64::from(Int::MAX),
    };
    // At most our own; lowering ourselves, below it.
    let up_to = levels
        .user_can_change_user_power_level(own, target)
        .then(|| if target == own { ours - 1 } else { ours });
    if target == own {
        return Manage {
            up_to,
            default_level,
            ..Manage::default()
        };
    }
    Manage {
        remove: levels.user_can_kick_user(own, target),
        ban: levels.user_can_ban_user(own, target),
        up_to,
        default_level,
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
