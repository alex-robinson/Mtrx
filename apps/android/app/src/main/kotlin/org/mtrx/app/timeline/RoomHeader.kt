package org.mtrx.app.timeline

import org.mtrx.app.model.directPerson
import uniffi.mtrx.RoomSummary

/**
 * What the timeline's top bar shows of the room: its name, and for a direct
 * chat with someone else ([person], their user ID) the room's avatar,
 * coloured by [seed] without one.
 */
data class RoomHeader(val name: String, val avatarUrl: String?, val seed: String, val person: String?)

/**
 * The header of room [roomId], from its summary; the ID while it is not in
 * the room list. Our own direct chat (the Notes to self) has no person.
 */
fun roomHeader(roomId: String, room: RoomSummary?, ownUserId: String): RoomHeader = RoomHeader(
    name = room?.name ?: roomId,
    avatarUrl = room?.avatarUrl,
    seed = room?.directUserId ?: roomId,
    person = room?.let { directPerson(it, ownUserId) },
)
