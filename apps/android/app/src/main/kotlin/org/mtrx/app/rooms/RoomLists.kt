package org.mtrx.app.rooms

import java.text.Collator
import java.time.Instant
import org.mtrx.app.model.RoomSort
import org.mtrx.app.model.SpaceEntry
import org.mtrx.app.model.roomIdsOf
import uniffi.mtrx.RoomKind
import uniffi.mtrx.RoomListState
import uniffi.mtrx.RoomSummary

// The lists of the Chats and Spaces tabs, from each room list. All keep the
// library's order (most recently active first), except a space's sections
// in the order chosen for them.

/** The Chats tab: the direct chats, invites to them and the Notes to self too. */
fun chats(state: RoomListState): List<RoomSummary> = state.rooms.filter { it.kind == RoomKind.DIRECT }

/**
 * An entry of the spaces page: a top-level space with its summary, or
 * "Other rooms" ([space] null). [unread] while any room it lists has unread
 * messages.
 */
data class SpaceRow(val entry: SpaceEntry, val space: RoomSummary?, val unread: Boolean) {
    val name: String get() = space?.name ?: OTHER_ROOMS_NAME
}

const val OTHER_ROOMS_NAME = "Other rooms"

/** The spaces page: the top-level spaces, then "Other rooms" unless it is empty. */
fun spaceRows(state: RoomListState): List<SpaceRow> {
    val byId = state.rooms.associateBy { it.id }
    fun unread(ids: List<String>) = ids.any { (byId[it]?.unreadMessages ?: 0uL) > 0uL }
    val spaces = state.spaces.mapNotNull { space ->
        byId[space.spaceId]?.let { SpaceRow(SpaceEntry.Space(space.spaceId), it, unread(space.roomIds)) }
    }
    val other = state.roomIdsOf(SpaceEntry.OtherRooms)?.let { SpaceRow(SpaceEntry.OtherRooms, null, unread(it)) }
    return spaces + listOfNotNull(other)
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

/** A row of Recent: the room, and the name of its space ("Other rooms" outside spaces). */
data class RecentRow(val room: RoomSummary, val space: String?)

/**
 * The recent rooms ([ids], newest first) that are still in the list: rooms
 * and invites, not direct chats nor joined spaces.
 */
fun recentRooms(state: RoomListState, ids: List<String>): List<RoomSummary> {
    val byId = state.rooms.associateBy { it.id }
    return ids.mapNotNull { byId[it] }.filter { opensOnRoomsTab(it) }
}

/** Recent's rows, each with the name of the top-level space that lists it. */
fun recentRows(state: RoomListState, ids: List<String>): List<RecentRow> {
    val names = state.rooms.associate { it.id to it.name }
    return recentRooms(state, ids).map { room ->
        val space = state.spaces.firstOrNull { room.id in it.roomIds }?.let { names[it.spaceId] }
            ?: room.parents.firstNotNullOfOrNull { names[it] }
            ?: OTHER_ROOMS_NAME.takeIf { room.id in state.roomsOutsideSpaces }
        RecentRow(room, space)
    }
}

/**
 * The room the Rooms tab reopens: the newest recent one still in the list;
 * the newest saved one while the list is not loaded yet, or while it is
 * [opening] (just joined or made) and not listed yet.
 */
fun lastRoom(state: RoomListState, ids: List<String>, opening: String? = null): String? {
    val newest = ids.firstOrNull()
    if (newest != null && newest == opening && state.rooms.none { it.id == newest }) return newest
    return recentRooms(state, ids).firstOrNull()?.id ?: if (state.loaded) null else newest
}

/** Rooms and invites open on the Rooms tab (and join Recent); direct chats on Chats. */
fun opensOnRoomsTab(room: RoomSummary): Boolean = room.kind != RoomKind.DIRECT && (room.kind != RoomKind.SPACE || room.invited)

/** Unread messages in the recent rooms, except [except] (the room on screen). */
fun recentUnread(state: RoomListState, ids: List<String>, except: String? = null): Long =
    recentRooms(state, ids).filter { it.id != except }.sumOf { it.unreadMessages.toLong() }

/** Unread messages in the spaces' rooms (and the other rooms) that are not recent. */
fun spacesUnread(state: RoomListState, ids: List<String>): Long {
    val recent = ids.toSet()
    return state.rooms
        .filter { it.kind == RoomKind.ROOM && !it.invited && it.id !in recent }
        .sumOf { it.unreadMessages.toLong() }
}

/** The unread count to show, and whether it counts mentions; null if none. */
fun unreadBadge(room: RoomSummary): Pair<Long, Boolean>? = when {
    room.unreadMentions > 0uL -> room.unreadMentions.toLong() to true
    room.unreadMessages > 0uL -> room.unreadMessages.toLong() to false
    else -> null
}
