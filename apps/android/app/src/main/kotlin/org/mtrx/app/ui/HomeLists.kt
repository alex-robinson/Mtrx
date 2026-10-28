package org.mtrx.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import java.time.LocalDate
import org.mtrx.app.model.Images
import org.mtrx.app.model.SpaceEntry
import org.mtrx.app.rooms.RecentRow
import org.mtrx.app.rooms.SpaceRow
import org.mtrx.app.rooms.SpaceSections
import org.mtrx.app.rooms.unreadBadge
import uniffi.mtrx.Presence
import uniffi.mtrx.RoomKind
import uniffi.mtrx.RoomSummary

// The lists of the home's tabs, in compact rows (#47): without a model, for
// the screens and their screenshot tests.

/** The Chats tab: the direct chats, with the people's presence. */
@Composable
fun ChatList(
    rooms: List<RoomSummary>,
    loaded: Boolean,
    images: Images,
    today: LocalDate,
    onOpenRoom: (RoomSummary) -> Unit,
    presenceOf: (RoomSummary) -> Presence?,
    state: LazyListState = rememberLazyListState(),
) {
    when {
        rooms.isEmpty() && !loaded -> Loading()
        rooms.isEmpty() -> Empty("No chats")
        else -> LazyColumn(Modifier.fillMaxSize(), state = state) {
            items(rooms, key = { it.id }) { room ->
                RoomRow(room, images, today, onClick = { onOpenRoom(room) }, presence = presenceOf(room))
            }
        }
    }
}

/** The spaces page: [rows], [selected] highlighted, a dot where unread. */
@Composable
fun SpaceList(
    rows: List<SpaceRow>,
    selected: SpaceEntry?,
    loaded: Boolean,
    images: Images,
    onSelect: (SpaceEntry) -> Unit,
    state: LazyListState = rememberLazyListState(),
) {
    when {
        rows.isEmpty() && !loaded -> Loading()
        rows.isEmpty() -> Empty("No spaces")
        else -> LazyColumn(Modifier.fillMaxSize(), state = state) {
            items(rows, key = { it.entry.key }) { row ->
                SpaceRowItem(row, row.entry == selected, images, onClick = { onSelect(row.entry) })
            }
        }
    }
}

/** A space's rooms page: Rooms, then People, each left out while empty. */
@Composable
fun SpaceRoomsList(
    sections: SpaceSections,
    loaded: Boolean,
    images: Images,
    today: LocalDate,
    onOpenRoom: (RoomSummary) -> Unit,
    state: LazyListState = rememberLazyListState(),
) {
    val empty = sections.rooms.isEmpty() && sections.people.isEmpty()
    when {
        empty && !loaded -> Loading()
        empty -> Empty("No rooms")
        else -> LazyColumn(Modifier.fillMaxSize(), state = state) {
            section("Rooms", sections.rooms, images, today, onOpenRoom)
            section("People", sections.people, images, today, onOpenRoom)
        }
    }
}

/**
 * Recent: the rooms opened lately, newest first, each with its space in
 * small grey and its unread count; without any, a way to the spaces.
 */
@Composable
fun RecentList(
    rows: List<RecentRow>,
    images: Images,
    today: LocalDate,
    onOpenRoom: (RoomSummary) -> Unit,
    onBrowseSpaces: () -> Unit,
    state: LazyListState = rememberLazyListState(),
) {
    if (rows.isEmpty()) {
        Column(
            Modifier.fillMaxSize(),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Text("Rooms you open show here", color = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.height(12.dp))
            Button(onClick = onBrowseSpaces) { Text("Browse spaces") }
        }
        return
    }
    LazyColumn(Modifier.fillMaxSize(), state = state) {
        items(rows, key = { it.room.id }) { row ->
            RoomRow(row.room, images, today, onClick = { onOpenRoom(row.room) }, subtitle = row.space ?: "")
        }
    }
}

private fun LazyListScope.section(
    title: String,
    rooms: List<RoomSummary>,
    images: Images,
    today: LocalDate,
    onOpenRoom: (RoomSummary) -> Unit,
) {
    if (rooms.isEmpty()) return
    // Room IDs start with "!": no clash with the rows' keys.
    item(key = "section:$title", contentType = "section") {
        Text(
            title,
            Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 2.dp),
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.primary,
        )
    }
    items(rooms, key = { it.id }, contentType = { "room" }) { room ->
        RoomRow(room, images, today, onClick = { onOpenRoom(room) })
    }
}

@Composable
private fun SpaceRowItem(row: SpaceRow, selected: Boolean, images: Images, onClick: () -> Unit) {
    val shape = RoundedCornerShape(8.dp)
    CompactRow(
        title = row.name,
        subtitle = null,
        onClick = onClick,
        bold = row.unread,
        modifier = if (selected) Modifier.background(MaterialTheme.colorScheme.secondaryContainer) else Modifier,
        leading = {
            val space = row.space
            if (space != null) {
                // Spaces are squares, as on the desktop.
                Avatar(space.name, space.avatarUrl, space.id, images, size = 36.dp, shape = shape)
            } else {
                Box(
                    Modifier
                        .size(36.dp)
                        .clip(shape)
                        .background(MaterialTheme.colorScheme.surfaceVariant),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.AutoMirrored.Filled.List, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        },
        trailing = {
            if (row.unread) {
                Box(
                    Modifier
                        .size(10.dp)
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.primary)
                        .semantics { contentDescription = "Unread" },
                )
            }
        },
    )
}

/**
 * A room in a compact row: avatar (with [presence]'s dot), name, a grey
 * line ([subtitle], else what the room is), its last activity and unread
 * count.
 */
@Composable
fun RoomRow(
    room: RoomSummary,
    images: Images,
    today: LocalDate,
    onClick: () -> Unit,
    presence: Presence? = null,
    subtitle: String? = null,
) {
    val badge = unreadBadge(room)
    val activity = room.lastActivity
    val line = subtitle?.ifEmpty { null } ?: when {
        room.invited -> "Invited"
        room.kind == RoomKind.SPACE -> "Space"
        else -> room.topic?.lineSequence()?.firstOrNull { it.isNotBlank() } ?: room.alias
    }
    CompactRow(
        title = room.name,
        subtitle = if (subtitle != null && room.invited) listOfNotNull("Invited", line).distinct().joinToString(" · ") else line,
        onClick = onClick,
        bold = room.unreadMessages > 0uL,
        leading = {
            WithPresenceDot(presence, avatarSize = 36.dp, dotSize = 10.dp, ring = MaterialTheme.colorScheme.surface) {
                Avatar(
                    name = room.name,
                    mxcUri = room.avatarUrl,
                    seed = room.directUserId ?: room.id,
                    images = images,
                    size = 36.dp,
                    // Spaces are squares, as on the desktop.
                    shape = if (room.kind == RoomKind.SPACE) RoundedCornerShape(8.dp) else CircleShape,
                )
            }
        },
        trailing = {
            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(2.dp)) {
                if (activity != null) {
                    Text(
                        lastActivity(activity, today),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (badge != null) UnreadBadge(badge.first, badge.second)
            }
        },
    )
}

@Composable
fun Loading() {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
}

@Composable
fun Empty(text: String) {
    Box(
        Modifier
            .fillMaxWidth()
            .fillMaxSize(),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
