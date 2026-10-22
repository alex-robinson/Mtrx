package org.mtrx.app.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Badge
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.time.LocalDate
import kotlinx.coroutines.launch
import org.mtrx.app.model.Account
import org.mtrx.app.model.Images
import org.mtrx.app.model.PresenceModel
import org.mtrx.app.model.PushRegistration
import org.mtrx.app.model.RoomSort
import org.mtrx.app.model.SpaceEntry
import org.mtrx.app.model.directPerson
import org.mtrx.app.model.showsEncryptionBanner
import org.mtrx.app.rooms.SpaceRow
import org.mtrx.app.rooms.SpaceSections
import org.mtrx.app.rooms.chats
import org.mtrx.app.rooms.sectionsOf
import org.mtrx.app.rooms.spaceRows
import org.mtrx.app.rooms.unreadBadge
import org.mtrx.app.settings.SessionInfo
import org.mtrx.app.settings.SettingsContent
import uniffi.mtrx.ConnectionState
import uniffi.mtrx.Presence
import uniffi.mtrx.RoomKind
import uniffi.mtrx.RoomSummary

/** The tabs of the bottom bar. */
enum class HomeTab(val label: String, val icon: ImageVector) {
    Chats("Chats", Icons.Filled.Person),
    Spaces("Spaces", MtrxIcons.Spaces),
    Settings("Settings", Icons.Filled.Settings),
}

/**
 * The signed-in home, in three tabs: Chats, the direct chats; Spaces, a
 * pager of the spaces (left) and the selected space's rooms and people
 * (right), as the desktop's two columns; and Settings. Tab, page and scroll
 * positions are kept while a room or another screen is open (the screen's
 * saved state); the selected space and the order of its rooms are the
 * account's ([Account.spaceSelection], [Account.roomSort]). Banners for the
 * connection over all tabs, and for encryption over Chats and Spaces. Chats
 * shows the people's [presence] on their avatars.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalFoundationApi::class)
@Composable
fun HomeScreen(
    account: Account,
    push: PushRegistration,
    info: SessionInfo,
    images: Images,
    presence: PresenceModel,
    onOpenRoom: (String) -> Unit,
    onEnterRecoveryKey: () -> Unit,
    onVerify: () -> Unit,
) {
    val list by account.rooms.collectAsStateWithLifecycle()
    val presences by presence.presences.collectAsStateWithLifecycle()
    val connection by account.connection.collectAsStateWithLifecycle()
    val encryption by account.encryption.collectAsStateWithLifecycle()
    val selected by account.spaceSelection.selected.collectAsStateWithLifecycle()
    val sort by account.roomSort.sort.collectAsStateWithLifecycle()
    var tab by rememberSaveable { mutableStateOf(HomeTab.Chats) }
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(account) { account.messages.collect { snackbar.showSnackbar(it) } }
    val today = remember(list) { LocalDate.now() }
    val chatRooms = remember(list) { chats(list) }
    val spaceEntries = remember(list) { spaceRows(list) }
    val sections = remember(list, selected, sort) {
        selected?.let { sectionsOf(list, it, sort) } ?: SpaceSections(emptyList(), emptyList())
    }

    val scope = rememberCoroutineScope()
    // The selected space's rooms are the second page; without one there is none.
    val pager = rememberPagerState { if (selected != null) 2 else 1 }
    val chatsList = rememberLazyListState()
    val spacesList = rememberLazyListState()
    val spaceRoomsList = rememberSaveable(selected?.key, saver = LazyListState.Saver) { LazyListState() }
    val showSpaces: () -> Unit = { scope.launch { pager.animateScrollToPage(0) } }

    // A new selection (tapped, or restored at start) shows its rooms; none
    // shows the spaces. The entry paged to is saved with the screen, so
    // coming back from a room keeps the page.
    var pagedTo by rememberSaveable { mutableStateOf<String?>(null) }
    LaunchedEffect(selected) {
        val entry = selected
        if (entry?.key == pagedTo) return@LaunchedEffect
        pagedTo = entry?.key
        when {
            entry == null -> pager.scrollToPage(0)
            tab == HomeTab.Spaces -> pager.animateScrollToPage(1)
            else -> pager.scrollToPage(1)
        }
    }
    val onSpaceRooms = tab == HomeTab.Spaces && pager.currentPage == 1 && selected != null
    BackHandler(enabled = onSpaceRooms, onBack = showSpaces)

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    val title = if (onSpaceRooms) {
                        spaceEntries.firstOrNull { it.entry == selected }?.name.orEmpty()
                    } else {
                        tab.label
                    }
                    Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis)
                },
                navigationIcon = {
                    if (onSpaceRooms) {
                        IconButton(onClick = showSpaces) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Spaces")
                        }
                    }
                },
                actions = {
                    if (onSpaceRooms) SortMenu(sort, onSort = account.roomSort::choose)
                },
            )
        },
        bottomBar = {
            NavigationBar {
                HomeTab.entries.forEach {
                    NavigationBarItem(
                        selected = tab == it,
                        onClick = {
                            // Spaces again: back to the spaces.
                            if (tab == it && it == HomeTab.Spaces) showSpaces()
                            tab = it
                        },
                        icon = { Icon(it.icon, contentDescription = null) },
                        label = { Text(it.label) },
                    )
                }
            }
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        Column(
            Modifier
                .padding(padding)
                .fillMaxSize(),
        ) {
            ConnectionBanner(connection, onSignInAgain = { account.forget() })
            // Settings says the same in its Encryption section.
            if (tab != HomeTab.Settings && showsEncryptionBanner(encryption)) {
                EncryptionBanner(onClick = { tab = HomeTab.Settings })
            }
            when (tab) {
                HomeTab.Chats -> RoomColumn(
                    chatRooms,
                    list.loaded,
                    "No chats",
                    chatsList,
                    images,
                    today,
                    onOpenRoom,
                    presenceOf = { room -> directPerson(room, info.userId)?.let { presences[it] } },
                )
                HomeTab.Spaces -> HorizontalPager(pager, Modifier.fillMaxSize()) { page ->
                    if (page == 0) {
                        SpaceColumn(spaceEntries, selected, list.loaded, spacesList, images) { entry ->
                            if (entry == selected) {
                                scope.launch { pager.animateScrollToPage(1) }
                            } else {
                                account.spaceSelection.select(entry)
                            }
                        }
                    } else {
                        SectionColumn(sections, list.loaded, spaceRoomsList, images, today, onOpenRoom)
                    }
                }
                HomeTab.Settings -> SettingsContent(account, push, presence, info, onEnterRecoveryKey, onVerify)
            }
        }
    }
}

/** The order of the spaces' rooms, chosen in a menu; the current one checked. */
@Composable
private fun SortMenu(sort: RoomSort, onSort: (RoomSort) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { open = true }) {
            Icon(MtrxIcons.Sort, contentDescription = "Sort")
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            RoomSort.entries.forEach { option ->
                DropdownMenuItem(
                    text = { Text(option.label) },
                    onClick = {
                        open = false
                        onSort(option)
                    },
                    trailingIcon = if (option == sort) {
                        { Icon(Icons.Filled.Check, contentDescription = "Selected") }
                    } else {
                        null
                    },
                )
            }
        }
    }
}

/** [rooms], or a spinner until the list is loaded, or [empty]; the dots of [presenceOf]. */
@Composable
private fun RoomColumn(
    rooms: List<RoomSummary>,
    loaded: Boolean,
    empty: String,
    state: LazyListState,
    images: Images,
    today: LocalDate,
    onOpenRoom: (String) -> Unit,
    presenceOf: (RoomSummary) -> Presence?,
) {
    when {
        rooms.isEmpty() && !loaded -> Loading()
        rooms.isEmpty() -> Empty(empty)
        else -> LazyColumn(Modifier.fillMaxSize(), state = state) {
            items(rooms, key = { it.id }) { room ->
                RoomRow(room, images, today, onClick = { onOpenRoom(room.id) }, presence = presenceOf(room))
            }
        }
    }
}

/** A space's rooms page: Rooms, then People, each left out while empty. */
@Composable
private fun SectionColumn(
    sections: SpaceSections,
    loaded: Boolean,
    state: LazyListState,
    images: Images,
    today: LocalDate,
    onOpenRoom: (String) -> Unit,
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

private fun LazyListScope.section(
    title: String,
    rooms: List<RoomSummary>,
    images: Images,
    today: LocalDate,
    onOpenRoom: (String) -> Unit,
) {
    if (rooms.isEmpty()) return
    // Room IDs start with "!": no clash with the rows' keys.
    item(key = "section:$title", contentType = "section") {
        Text(
            title,
            Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 4.dp),
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.primary,
        )
    }
    items(rooms, key = { it.id }, contentType = { "room" }) { room ->
        RoomRow(room, images, today, onClick = { onOpenRoom(room.id) })
    }
}

/** The spaces page: [rows], [selected] highlighted. */
@Composable
private fun SpaceColumn(
    rows: List<SpaceRow>,
    selected: SpaceEntry?,
    loaded: Boolean,
    state: LazyListState,
    images: Images,
    onSelect: (SpaceEntry) -> Unit,
) {
    when {
        rows.isEmpty() && !loaded -> Loading()
        rows.isEmpty() -> Empty("No rooms")
        else -> LazyColumn(Modifier.fillMaxSize(), state = state) {
            items(rows, key = { it.entry.key }) { row ->
                SpaceRowItem(row, row.entry == selected, images, onClick = { onSelect(row.entry) })
            }
        }
    }
}

@Composable
private fun SpaceRowItem(row: SpaceRow, selected: Boolean, images: Images, onClick: () -> Unit) {
    val shape = RoundedCornerShape(10.dp)
    ListItem(
        modifier = Modifier.clickable(onClick = onClick),
        colors = if (selected) {
            ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.secondaryContainer)
        } else {
            ListItemDefaults.colors()
        },
        leadingContent = {
            val space = row.space
            if (space != null) {
                // Spaces are squares, as on the desktop.
                Avatar(space.name, space.avatarUrl, space.id, images, size = 44.dp, shape = shape)
            } else {
                Box(
                    Modifier
                        .size(44.dp)
                        .clip(shape)
                        .background(MaterialTheme.colorScheme.surfaceVariant),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        Icons.AutoMirrored.Filled.List,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        },
        headlineContent = {
            Text(
                row.name,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                fontWeight = if (row.unread) FontWeight.SemiBold else FontWeight.Normal,
            )
        },
        trailingContent = if (row.unread) {
            {
                Box(
                    Modifier
                        .size(10.dp)
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.primary)
                        .semantics { contentDescription = "Unread" },
                )
            }
        } else {
            null
        },
    )
}

@Composable
private fun Loading() {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
}

@Composable
private fun Empty(text: String) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun ConnectionBanner(connection: ConnectionState, onSignInAgain: () -> Unit) {
    when (connection) {
        ConnectionState.ONLINE -> {}
        ConnectionState.OFFLINE -> Surface(color = MaterialTheme.colorScheme.secondaryContainer) {
            Text(
                "Offline. Messages are sent when the connection is back.",
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                style = MaterialTheme.typography.bodySmall,
            )
        }
        ConnectionState.SIGNED_OUT -> Surface(color = MaterialTheme.colorScheme.errorContainer) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(start = 16.dp, end = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "The server signed this device out.",
                    Modifier.weight(1f),
                    style = MaterialTheme.typography.bodySmall,
                )
                TextButton(onClick = onSignInAgain) { Text("Sign in again") }
            }
        }
    }
}

@Composable
private fun EncryptionBanner(onClick: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.tertiaryContainer) {
        Row(
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = 16.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(Icons.Filled.Lock, contentDescription = null)
            Text(
                "Verify this device or enter your recovery key to read encrypted messages.",
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

@Composable
private fun RoomRow(room: RoomSummary, images: Images, today: LocalDate, onClick: () -> Unit, presence: Presence? = null) {
    val badge = unreadBadge(room)
    val unread = room.unreadMessages > 0uL
    val activity = room.lastActivity
    val subtitle = when {
        room.invited -> "Invited"
        room.kind == RoomKind.SPACE -> "Space"
        else -> room.topic?.lineSequence()?.firstOrNull() ?: room.alias
    }
    ListItem(
        modifier = Modifier.clickable(onClick = onClick),
        leadingContent = {
            WithPresenceDot(presence, avatarSize = 44.dp, dotSize = 12.dp, ring = MaterialTheme.colorScheme.surface) {
                Avatar(
                    name = room.name,
                    mxcUri = room.avatarUrl,
                    seed = room.directUserId ?: room.id,
                    images = images,
                    size = 44.dp,
                    // Spaces are squares, as on the desktop.
                    shape = if (room.kind == RoomKind.SPACE) RoundedCornerShape(10.dp) else CircleShape,
                )
            }
        },
        headlineContent = {
            Text(
                room.name,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                fontWeight = if (unread) FontWeight.SemiBold else FontWeight.Normal,
            )
        },
        supportingContent = if (subtitle != null) {
            { Text(subtitle, maxLines = 1, overflow = TextOverflow.Ellipsis) }
        } else {
            null
        },
        trailingContent = {
            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                if (activity != null) {
                    Text(
                        lastActivity(activity, today),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (badge != null) {
                    val (count, mentions) = badge
                    Badge(
                        containerColor = if (mentions) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
                        contentColor = if (mentions) MaterialTheme.colorScheme.onError else MaterialTheme.colorScheme.onPrimary,
                    ) { Text(if (count > 99) "99+" else count.toString()) }
                }
            }
        },
    )
}
