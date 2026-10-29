package org.mtrx.app.timeline

import org.mtrx.app.model.directPerson
import org.mtrx.app.rooms.NO_TOPIC
import uniffi.mtrx.RoomSummary
import uniffi.mtrx.Topic
import uniffi.mtrx.parseTopic as libraryParseTopic

/**
 * What the timeline's top bar shows of the room: its name, and for a direct
 * chat with someone else ([person], their user ID) the room's avatar,
 * coloured by [seed] without one; its [topic] (text and links) and the
 * event IDs of its [pinned] messages.
 */
data class RoomHeader(
    val name: String,
    val avatarUrl: String?,
    val seed: String,
    val person: String?,
    val topic: Topic = NO_TOPIC,
    val pinned: List<String> = emptyList(),
)

/**
 * The header of room [roomId], from its summary; the ID while it is not in
 * the room list. Our own direct chat (the Notes to self) has no person.
 * [parseTopic] reads the topic's text and links (the library's).
 */
fun roomHeader(roomId: String, room: RoomSummary?, ownUserId: String, parseTopic: (String) -> Topic = ::libraryParseTopic): RoomHeader = RoomHeader(
    name = room?.name ?: roomId,
    avatarUrl = room?.avatarUrl,
    seed = room?.directUserId ?: roomId,
    person = room?.let { directPerson(it, ownUserId) },
    topic = room?.topic?.takeIf { it.isNotBlank() }?.let(parseTopic) ?: NO_TOPIC,
    pinned = room?.pinned.orEmpty(),
)
