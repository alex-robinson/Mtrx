package org.mtrx.app.rooms

import uniffi.mtrx.RoomKind
import uniffi.mtrx.RoomSummary

/** Which rooms the list shows. */
enum class RoomFilter(val label: String) {
    All("All"),
    People("People"),
    Rooms("Rooms"),
    Spaces("Spaces"),
}

/**
 * The rooms of [filter], in the library's order (most recently active
 * first). Joined spaces hold rooms rather than messages: only under Spaces
 * (invitations to them show everywhere).
 */
fun filterRooms(rooms: List<RoomSummary>, filter: RoomFilter): List<RoomSummary> = rooms.filter {
    when (filter) {
        RoomFilter.All -> it.kind != RoomKind.SPACE || it.invited
        RoomFilter.People -> it.kind == RoomKind.DIRECT
        RoomFilter.Rooms -> it.kind == RoomKind.ROOM
        RoomFilter.Spaces -> it.kind == RoomKind.SPACE
    }
}

/** The unread count to show, and whether it counts mentions; null if none. */
fun unreadBadge(room: RoomSummary): Pair<Long, Boolean>? = when {
    room.unreadMentions > 0uL -> room.unreadMentions.toLong() to true
    room.unreadMessages > 0uL -> room.unreadMessages.toLong() to false
    else -> null
}
