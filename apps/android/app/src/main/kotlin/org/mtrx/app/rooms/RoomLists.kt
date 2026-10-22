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

/** The rooms [entry] lists, for the page right of the spaces; empty while it is not listed. */
fun roomsOf(state: RoomListState, entry: SpaceEntry): List<RoomSummary> {
    val ids = state.roomIdsOf(entry) ?: return emptyList()
    val byId = state.rooms.associateBy { it.id }
    return ids.mapNotNull { byId[it] }
}

/** A space's rooms page: its rooms, and its direct chats (People). */
data class SpaceSections(val rooms: List<RoomSummary>, val people: List<RoomSummary>)

/** The rooms [entry] lists, the direct chats apart, each part in the order [sort]. */
fun sectionsOf(state: RoomListState, entry: SpaceEntry, sort: RoomSort): SpaceSections {
    val (people, rooms) = roomsOf(state, entry).partition { it.kind == RoomKind.DIRECT }
    return SpaceSections(rooms = sorted(rooms, sort), people = sorted(people, sort))
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

/** The unread count to show, and whether it counts mentions; null if none. */
fun unreadBadge(room: RoomSummary): Pair<Long, Boolean>? = when {
    room.unreadMentions > 0uL -> room.unreadMentions.toLong() to true
    room.unreadMessages > 0uL -> room.unreadMessages.toLong() to false
    else -> null
}
