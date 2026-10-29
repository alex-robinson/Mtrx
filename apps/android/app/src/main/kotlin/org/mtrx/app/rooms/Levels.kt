package org.mtrx.app.rooms

import uniffi.mtrx.Member

// Members' roles and power levels, as the desktop words them
// (crates/mtrx-app/src/members.rs and dialog.rs).

const val ADMIN_LEVEL = 100L
const val MODERATOR_LEVEL = 50L

/** "Owner" (a creator, above every level), "Admin", "Mod", "Level 20", or nothing for a plain member. */
fun roleLabel(member: Member): String? = when (val level = member.level) {
    null -> "Owner"
    ADMIN_LEVEL -> "Admin"
    MODERATOR_LEVEL -> "Mod"
    member.manage.defaultLevel -> null
    else -> "Level $level"
}

/** "admin", "moderator", "member", "level 20". */
fun roleName(level: Long, defaultLevel: Long): String = when (level) {
    ADMIN_LEVEL -> "admin"
    MODERATOR_LEVEL -> "moderator"
    defaultLevel -> "member"
    else -> "level $level"
}

/** The role with its article: "an admin", "a moderator", "level 20". */
fun aRole(level: Long, defaultLevel: Long): String {
    val role = roleName(level, defaultLevel)
    return when {
        role.startsWith("level") -> role
        role.startsWith('a') -> "an $role"
        else -> "a $role"
    }
}

/** A role in a member's menu: its level, ticked if theirs; [enabled] if we may give it. */
data class LevelChoice(val label: String, val level: Long, val current: Boolean, val enabled: Boolean)

/**
 * The roles a member's menu offers: theirs (ticked; a custom level of
 * theirs too) and those up to [Manage.upTo]; empty if we may not change
 * theirs.
 */
fun levelChoices(member: Member): List<LevelChoice> {
    val upTo = member.manage.upTo ?: return emptyList()
    val defaultLevel = member.manage.defaultLevel
    val levels = listOf("Admin" to ADMIN_LEVEL, "Moderator" to MODERATOR_LEVEL, "Member" to defaultLevel)
        .distinctBy { it.second }
    val custom = member.level?.takeIf { level -> levels.none { it.second == level } }
    val choices = levels.mapNotNull { (label, level) ->
        val current = member.level == level
        val enabled = !current && level <= upTo
        LevelChoice("$label $level", level, current, enabled).takeIf { current || enabled }
    }
    return listOfNotNull(custom?.let { LevelChoice("Level $it", it, current = true, enabled = false) }) + choices
}

/** A change of level to ask about first, as it can't be undone. */
data class LevelConfirmation(val title: String, val message: String, val button: String)

/**
 * What to ask before giving [level] to [member] ([own]: ourselves; [alone]:
 * nobody else has our level); null where it can be undone. Our own level
 * always asks; theirs when it is as high as ours.
 */
fun levelConfirmation(member: Member, name: String, level: Long, own: Boolean, alone: Boolean): LevelConfirmation? {
    val upTo = member.manage.upTo ?: return null
    val defaultLevel = member.manage.defaultLevel
    return when {
        own -> LevelConfirmation(
            title = "Step down to ${roleName(level, defaultLevel)}?",
            message = "You can't undo this: only someone with a higher level can raise you again." +
                if (alone) " Nobody else here has your level, so nobody will be able to do all you can now." else "",
            button = "Step down",
        )
        level < upTo -> null
        level == ADMIN_LEVEL -> LevelConfirmation(
            title = "Make $name ${aRole(level, defaultLevel)}?",
            message = "Admins can do everything you can, including changing the room and its people. " +
                "You can't take it back: an admin can't demote another admin.",
            button = "Change",
        )
        else -> LevelConfirmation(
            title = "Make $name ${aRole(level, defaultLevel)}?",
            message = "They'll have the same level as you. You can't take it back: nobody can lower " +
                "someone with their own level.",
            button = "Change",
        )
    }
}

/** Nobody in [members] but us ([ownUserId]) has our level. */
fun aloneAtOurLevel(members: List<Member>, ownUserId: String): Boolean {
    val us = members.firstOrNull { it.userId == ownUserId } ?: return false
    return members.none { it.userId != ownUserId && !it.invited && it.level == us.level }
}

/** A member's name: their display name, else their user ID. */
fun memberName(member: Member): String = member.name?.takeIf { it.isNotBlank() } ?: member.userId

/** We may do something to [member]: change their level, remove or ban them. */
fun canManage(member: Member): Boolean = member.manage.upTo != null || member.manage.remove || member.manage.ban
