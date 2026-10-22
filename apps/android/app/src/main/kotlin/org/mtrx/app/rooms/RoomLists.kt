package org.mtrx.app.rooms

import org.mtrx.app.model.SpaceEntry
import org.mtrx.app.model.roomIdsOf
import uniffi.mtrx.RoomKind
import uniffi.mtrx.RoomListState
import uniffi.mtrx.RoomSummary

// The lists of the Chats and Rooms tabs, from each room list. All keep the
// library's order (most recently active first); nothing is re-sorted.

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

/** The rooms [entry] lists, for the page right of the spaces; empty while it is not listed. */
fun roomsOf(state: RoomListState, entry: SpaceEntry): List<RoomSummary> {
    val ids = state.roomIdsOf(entry) ?: return emptyList()
    val byId = state.rooms.associateBy { it.id }
    return ids.mapNotNull { byId[it] }
}

/** The unread count to show, and whether it counts mentions; null if none. */
fun unreadBadge(room: RoomSummary): Pair<Long, Boolean>? = when {
    room.unreadMentions > 0uL -> room.unreadMentions.toLong() to true
    room.unreadMessages > 0uL -> room.unreadMessages.toLong() to false
    else -> null
}
