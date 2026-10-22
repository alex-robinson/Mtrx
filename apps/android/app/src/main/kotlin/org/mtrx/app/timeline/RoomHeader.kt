package org.mtrx.app.timeline

import uniffi.mtrx.RoomKind
import uniffi.mtrx.RoomSummary

/**
 * What the timeline's top bar shows of the room: its name, and for a direct
 * chat the person's avatar (the room's), coloured by [seed] without one.
 */
data class RoomHeader(val name: String, val avatarUrl: String?, val seed: String, val direct: Boolean)

/** The header of room [roomId], from its summary; the ID while it is not in the room list. */
fun roomHeader(roomId: String, room: RoomSummary?): RoomHeader = RoomHeader(
    name = room?.name ?: roomId,
    avatarUrl = room?.avatarUrl,
    seed = room?.directUserId ?: roomId,
    direct = room?.kind == RoomKind.DIRECT,
)
