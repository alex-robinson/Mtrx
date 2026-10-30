package org.mtrx.app.rooms

import java.text.Collator
import java.time.Instant
import org.mtrx.app.model.RoomSort
import org.mtrx.app.model.SpaceEntry
import org.mtrx.app.model.roomIdsOf
import uniffi.mtrx.RoomKind
import uniffi.mtrx.RoomListState
import uniffi.mtrx.RoomSummary

// The lists of the Chats, Rooms and Spaces tabs, from each room list. All
// keep the library's order (most recently active first), except a space's
// sections in the order chosen for them and the Rooms tab's groups, by
// their rooms' latest messages.

/** The Chats tab: the direct chats, invites to them and the Notes to self too. */
fun chats(state: RoomListState): List<RoomSummary> = state.rooms.filter { it.kind == RoomKind.DIRECT }

/**
 * The names of the top-level spaces shared with each direct chat's person,
 * in the Spaces tab's order, by room ID: a space lists the direct chats
 * with its members. Not the Notes to self's.
 */
fun sharedSpaces(state: RoomListState): Map<String, List<String>> {
    val byId = state.rooms.associateBy { it.id }
    val spaces = state.spaces.mapNotNull { space -> byId[space.spaceId]?.let { it.name to space.roomIds.toSet() } }
    return chats(state).filter { it.id != state.notesRoomId }.associate { room ->
        room.id to spaces.filter { (_, ids) -> room.id in ids }.map { (name, _) -> name }
    }
}

/**
 * An entry of the spaces page: a top-level space with its summary, or Home
 * ([space] null). [unread] while any room it lists has unread messages.
 */
data class SpaceRow(val entry: SpaceEntry, val space: RoomSummary?, val unread: Boolean) {
    val name: String get() = space?.name ?: HOME_NAME
}

/** The spaces page: the top-level spaces, then Home unless it is empty. */
fun spaceRows(state: RoomListState): List<SpaceRow> {
    val byId = state.rooms.associateBy { it.id }
    fun unread(ids: List<String>) = ids.any { (byId[it]?.unreadMessages ?: 0uL) > 0uL }
    val spaces = state.spaces.mapNotNull { space ->
        byId[space.spaceId]?.let { SpaceRow(SpaceEntry.Space(space.spaceId), it, unread(space.roomIds)) }
    }
    val home = state.roomIdsOf(SpaceEntry.Home)?.let { SpaceRow(SpaceEntry.Home, null, unread(it)) }
    return spaces + listOfNotNull(home)
}

/** The top-level spaces, in the list's order: those to invite someone to. */
fun topLevelSpaces(state: RoomListState): List<RoomSummary> {
    val byId = state.rooms.associateBy { it.id }
    return state.spaces.mapNotNull { byId[it.spaceId] }
}

/** The rooms [entry] lists, for the page right of the spaces; empty while it is not listed. */
fun roomsOf(state: RoomListState, entry: SpaceEntry): List<RoomSummary> {
    val ids = state.roomIdsOf(entry) ?: return emptyList()
    val byId = state.rooms.associateBy { it.id }
    return ids.mapNotNull { byId[it] }
}


/** [rooms] in the order [sort]; ties keep the list's order. */
fun sorted(rooms: List<RoomSummary>, sort: RoomSort): List<RoomSummary> = when (sort) {
    RoomSort.DEFAULT -> rooms
    RoomSort.A_TO_Z -> rooms.sortedWith(byName())
    RoomSort.Z_TO_A -> rooms.sortedWith(byName().reversed())
    RoomSort.RECENT -> rooms.sortedWith(byRecent)
}

/** By name as the language orders it, upper and lower case alike. */
private fun byName(): Comparator<RoomSummary> {
    val collator = Collator.getInstance().apply { strength = Collator.SECONDARY }
    return Comparator { a, b -> collator.compare(a.name, b.name) }
}

/** The latest active first; never active last. */
private val byRecent: Comparator<RoomSummary> = compareBy(nullsLast(reverseOrder<Instant>())) { it.lastActivity }

/**
 * A group of the Rooms tab: Home ([space] null, the rooms in no space) or a
 * top-level space, with its rooms; [folded] on this phone. Its heading
 * shows the [unread] count while folded, red with [mentions].
 */
data class RoomGroup(val space: RoomSummary?, val rooms: List<RoomSummary>, val folded: Boolean = false) {
    /** As the Spaces tab's entry's (`SpaceEntry.key`). */
    val key: String get() = space?.id ?: SpaceEntry.HOME
    val name: String get() = space?.name ?: HOME_NAME
    val unread: Long get() = rooms.sumOf { it.unreadMessages.toLong() }
    val mentions: Boolean get() = rooms.any { it.unreadMentions > 0uL }
}

const val HOME_NAME = "Home"

/**
 * The Rooms tab's groups: Home, then each top-level space in the Spaces
 * tab's order, each with its rooms and invites (no direct chats nor joined
 * spaces) by their latest message, newest first; a room in several spaces
 * is in each. Those whose key is in [folded] are folded. [only], if given,
 * keeps those rooms (a direct chat's rooms shared with the person). Groups
 * without rooms are left out.
 */
fun roomGroups(state: RoomListState, only: Set<String>? = null, folded: Set<String> = emptySet()): List<RoomGroup> {
    val byId = state.rooms.associateBy { it.id }
    fun group(space: RoomSummary?, ids: List<String>) = RoomGroup(
        space,
        ids.mapNotNull { byId[it] }
            .filter { opensOnRoomsTab(it) && (only == null || it.id in only) }
            .sortedWith(byRecent),
        folded = (space?.id ?: SpaceEntry.HOME) in folded,
    )
    val home = group(null, state.roomsOutsideSpaces)
    val spaces = state.spaces.mapNotNull { space -> byId[space.spaceId]?.let { group(it, space.roomIds) } }
    return (listOf(home) + spaces).filter { it.rooms.isNotEmpty() }
}

/**
 * The room open on the Rooms tab, [roomId], while it is listed there; also
 * before the list is loaded, and while it is [opening] (just joined or
 * made) and not listed yet.
 */
fun shownRoom(state: RoomListState, roomId: String?, opening: String? = null): String? {
    if (roomId == null) return null
    val room = state.rooms.firstOrNull { it.id == roomId }
    return when {
        room != null -> roomId.takeIf { opensOnRoomsTab(room) }
        roomId == opening || !state.loaded -> roomId
        else -> null
    }
}

/**
 * The space a room's top bar shows: the first top-level space, in the
 * Spaces tab's order, that lists room [roomId]; null for a room in none,
 * and for direct chats.
 */
fun roomSpace(state: RoomListState, roomId: String): RoomSummary? {
    val byId = state.rooms.associateBy { it.id }
    val room = byId[roomId] ?: return null
    if (!opensOnRoomsTab(room)) return null
    return state.spaces.filter { roomId in it.roomIds }.firstNotNullOfOrNull { byId[it.spaceId] }
}

/** Rooms and invites open on the Rooms tab; direct chats on Chats. */
fun opensOnRoomsTab(room: RoomSummary): Boolean = room.kind != RoomKind.DIRECT && (room.kind != RoomKind.SPACE || room.invited)

/** Unread messages in the direct chats: the Chats tab's badge. */
fun chatsUnread(state: RoomListState): Long = state.rooms.filter { it.kind == RoomKind.DIRECT }.sumOf { it.unreadMessages.toLong() }

/** Unread messages in the rooms: the Rooms tab's badge. */
fun roomsUnread(state: RoomListState): Long = state.rooms.filter { it.kind == RoomKind.ROOM }.sumOf { it.unreadMessages.toLong() }

/** Unread messages in the rooms and direct chats but [except] (the one on screen): its back arrow's count. */
fun unreadElsewhere(state: RoomListState, except: String): Long =
    state.rooms.filter { it.kind != RoomKind.SPACE && it.id != except }.sumOf { it.unreadMessages.toLong() }

/** Unread messages in the rooms the spaces list: the Spaces tab's badge. */
fun spacesUnread(state: RoomListState): Long {
    val inSpaces = state.spaces.flatMap { it.roomIds }.toSet()
    return state.rooms.filter { it.kind == RoomKind.ROOM && it.id in inSpaces }.sumOf { it.unreadMessages.toLong() }
}

/** The unread count to show, and whether it counts mentions; null if none. */
fun unreadBadge(room: RoomSummary): Pair<Long, Boolean>? = when {
    room.unreadMentions > 0uL -> room.unreadMentions.toLong() to true
    room.unreadMessages > 0uL -> room.unreadMessages.toLong() to false
    else -> null
}
