package org.mtrx.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material3.HorizontalDivider
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import org.mtrx.app.rooms.ShownSection
import org.mtrx.app.rooms.SectionedList
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
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
import org.mtrx.app.rooms.RoomGroup
import org.mtrx.app.rooms.SpaceRow
import org.mtrx.app.rooms.unreadBadge
import uniffi.mtrx.Presence
import uniffi.mtrx.RoomKind
import uniffi.mtrx.RoomSummary

// The lists of the home's tabs, in compact rows (#47): without a model, for
// the screens and their screenshot tests.

/**
 * The Chats tab: the Notes to self at the top where pinned there ([notes]),
 * then the direct chats, with the people's presence; a pencil on those with
 * a draft ([drafts], room IDs).
 */
@Composable
fun ChatList(
    notes: RoomSummary?,
    rooms: List<RoomSummary>,
    loaded: Boolean,
    images: Images,
    today: LocalDate,
    onOpenRoom: (RoomSummary) -> Unit,
    presenceOf: (RoomSummary) -> Presence?,
    drafts: Set<String> = emptySet(),
    state: LazyListState = rememberLazyListState(),
) {
    when {
        notes == null && rooms.isEmpty() && !loaded -> Loading()
        notes == null && rooms.isEmpty() -> Empty("No chats")
        else -> LazyColumn(Modifier.fillMaxSize(), state = state) {
            notesRow(notes, images, today, drafts, onOpenRoom)
            items(rooms, key = { it.id }) { room ->
                RoomRow(room, images, today, onClick = { onOpenRoom(room) }, presence = presenceOf(room), draft = room.id in drafts)
            }
        }
    }
}

/** The Notes to self, pinned above the rest, with a line under it. */
private fun LazyListScope.notesRow(
    notes: RoomSummary?,
    images: Images,
    today: LocalDate,
    drafts: Set<String>,
    onOpenRoom: (RoomSummary) -> Unit,
) {
    if (notes == null) return
    item(key = "notes", contentType = "notes") {
        Column {
            RoomRow(notes, images, today, onClick = { onOpenRoom(notes) }, draft = notes.id in drafts)
            HorizontalDivider(Modifier.padding(horizontal = 16.dp))
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

/**
 * A space's rooms page: the Notes to self at the top where pinned there,
 * then the sections shown, in order; a section's heading folds it
 * ([onFold] with its key), and a folded one shows its unread count. A
 * pencil on the rooms with a draft ([drafts]).
 */
@Composable
fun SpaceRoomsList(
    list: SectionedList,
    loaded: Boolean,
    images: Images,
    today: LocalDate,
    onOpenRoom: (RoomSummary) -> Unit,
    onFold: (String) -> Unit,
    drafts: Set<String> = emptySet(),
    state: LazyListState = rememberLazyListState(),
) {
    when {
        list.isEmpty && !loaded -> Loading()
        list.isEmpty -> Empty("No rooms")
        else -> LazyColumn(Modifier.fillMaxSize(), state = state) {
            notesRow(list.notes, images, today, drafts, onOpenRoom)
            for (section in list.sections) section(section, images, today, drafts, onOpenRoom, onFold)
        }
    }
}

/**
 * The Rooms tab's [groups] (and a direct chat's rooms shared with the
 * person): each under a heading with its unread count, a space's opening it
 * on the Spaces tab ([onOpenSpace], its ID); a pencil on the rooms with a
 * draft ([drafts]). [empty] says there are none.
 */
@Composable
fun RoomGroupsList(
    groups: List<RoomGroup>,
    loaded: Boolean,
    images: Images,
    today: LocalDate,
    onOpenRoom: (RoomSummary) -> Unit,
    onOpenSpace: (String) -> Unit,
    drafts: Set<String> = emptySet(),
    empty: String = "No rooms",
    state: LazyListState = rememberLazyListState(),
) {
    when {
        groups.isEmpty() && !loaded -> Loading()
        groups.isEmpty() -> Empty(empty)
        else -> LazyColumn(Modifier.fillMaxSize(), state = state) {
            for (group in groups) {
                item(key = "group:${group.key}", contentType = "group") {
                    GroupHeading(group, images, onClick = group.space?.let { space -> { onOpenSpace(space.id) } })
                }
                items(group.rooms, key = { "${group.key}/${it.id}" }, contentType = { "room" }) { room ->
                    RoomRow(room, images, today, onClick = { onOpenRoom(room) }, draft = room.id in drafts)
                }
            }
        }
    }
}

/**
 * A group's heading: the space's small picture (a house for Home), its
 * name and unread count; tapped ([onClick], for a space), a caret.
 */
@Composable
private fun GroupHeading(group: RoomGroup, images: Images, onClick: (() -> Unit)?) {
    Row(
        Modifier
            .fillMaxWidth()
            .let { if (onClick != null) it.clickable(onClickLabel = "Open the space", onClick = onClick) else it }
            .padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // The name and caret at the left, the count at the right edge (as the rooms').
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
            // As tall as the name, at any text size.
            val iconSize = linesHeight(MaterialTheme.typography.titleSmall)
            val space = group.space
            if (space != null) {
                Avatar(space.name, space.avatarUrl, space.id, images, size = iconSize, shape = RoundedCornerShape(4.dp))
            } else {
                Icon(Icons.Filled.Home, contentDescription = null, modifier = Modifier.size(iconSize), tint = MaterialTheme.colorScheme.primary)
            }
            Spacer(Modifier.width(8.dp))
            Text(
                group.name,
                Modifier.weight(1f, fill = false),
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.primary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (onClick != null) {
                Icon(
                    Icons.AutoMirrored.Filled.KeyboardArrowRight,
                    contentDescription = null,
                    modifier = Modifier.size(iconSize),
                    tint = MaterialTheme.colorScheme.primary,
                )
            }
        }
        if (group.unread > 0) UnreadBadge(group.unread, group.mentions)
    }
}

private fun LazyListScope.section(
    section: ShownSection,
    images: Images,
    today: LocalDate,
    drafts: Set<String>,
    onOpenRoom: (RoomSummary) -> Unit,
    onFold: (String) -> Unit,
) {
    item(key = "section:${section.key}", contentType = "section") {
        SectionHeading(section, onClick = { onFold(section.key) })
    }
    if (section.folded) return
    items(section.rooms, key = { "${section.key}/${it.id}" }, contentType = { "room" }) { room ->
        RoomRow(room, images, today, onClick = { onOpenRoom(room) }, draft = room.id in drafts)
    }
}

/** A section's heading: a caret (down while unfolded), its title, and while folded its unread count. */
@Composable
private fun SectionHeading(section: ShownSection, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClickLabel = if (section.folded) "Unfold" else "Fold", onClick = onClick)
            .padding(start = 8.dp, end = 16.dp, top = 8.dp, bottom = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (section.folded) Icons.AutoMirrored.Filled.KeyboardArrowRight else Icons.Filled.KeyboardArrowDown,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = MaterialTheme.colorScheme.primary,
        )
        Spacer(Modifier.width(4.dp))
        Text(
            section.title,
            Modifier.weight(1f),
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.primary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (section.folded && section.unread > 0) UnreadBadge(section.unread, section.mentions)
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
 * line ([subtitle], else what the room is), its last activity, and below
 * it a small grey pencil if it has a [draft] and the unread count.
 */
@Composable
fun RoomRow(
    room: RoomSummary,
    images: Images,
    today: LocalDate,
    onClick: () -> Unit,
    presence: Presence? = null,
    subtitle: String? = null,
    draft: Boolean = false,
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
                if (draft || badge != null) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        if (draft) {
                            Icon(
                                Icons.Outlined.Edit,
                                contentDescription = "Draft",
                                modifier = Modifier.size(14.dp),
                                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        if (badge != null) UnreadBadge(badge.first, badge.second)
                    }
                }
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
            .fillMaxSize()
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
    }
}
