package org.mtrx.app.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
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
import org.mtrx.app.rooms.SpaceSections
import org.mtrx.app.rooms.chats
import org.mtrx.app.rooms.lastRoom
import org.mtrx.app.rooms.recentRows
import org.mtrx.app.rooms.recentUnread
import org.mtrx.app.rooms.sectionsOf
import org.mtrx.app.rooms.spaceRows
import org.mtrx.app.rooms.spacesUnread
import org.mtrx.app.settings.SessionInfo
import org.mtrx.app.settings.SettingsContent
import uniffi.mtrx.ConnectionState
import uniffi.mtrx.RoomSummary

/** What the home's other screens open: from the Spaces tab's + menu and a space's name. */
class HomeRoutes(
    val openOverview: (String) -> Unit,
    val newSpace: () -> Unit,
    val newRoom: (parent: String) -> Unit,
    val join: () -> Unit,
)

/**
 * The signed-in home, in four tabs ([nav]): Chats, the direct chats (one
 * opens in the tab); Spaces, a pager of the spaces and the selected space's
 * rooms and people, whose name opens its overview, with + for new spaces,
 * rooms and joining; Rooms, the last room ([roomPane]) or Recent; and
 * Settings. Spaces' badge counts unread messages in rooms that are not
 * recent, Rooms' those in recent ones. Banners for the connection over all
 * tabs, and for encryption over the lists.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun HomeScreen(
    account: Account,
    push: PushRegistration,
    info: SessionInfo,
    images: Images,
    presence: PresenceModel,
    nav: HomeNav,
    routes: HomeRoutes,
    onEnterRecoveryKey: () -> Unit,
    onVerify: () -> Unit,
    roomPane: @Composable (roomId: String, backCount: Long, onBack: () -> Unit) -> Unit,
) {
    val list by account.rooms.collectAsStateWithLifecycle()
    val presences by presence.presences.collectAsStateWithLifecycle()
    val connection by account.connection.collectAsStateWithLifecycle()
    val encryption by account.encryption.collectAsStateWithLifecycle()
    val selected by account.spaceSelection.selected.collectAsStateWithLifecycle()
    val sort by account.roomSort.sort.collectAsStateWithLifecycle()
    val recentIds by account.recent.ids.collectAsStateWithLifecycle()
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(account) { account.messages.collect { snackbar.showSnackbar(it) } }
    val today = remember(list) { LocalDate.now() }
    val chatRooms = remember(list) { chats(list) }
    val spaceEntries = remember(list) { spaceRows(list) }
    val sections = remember(list, selected, sort) {
        selected?.let { sectionsOf(list, it, sort) } ?: SpaceSections(emptyList(), emptyList())
    }
    val recent = remember(list, recentIds) { recentRows(list, recentIds) }
    val last = lastRoom(list, recentIds, nav.opening)
    val badges = mapOf(
        HomeTab.Spaces to spacesUnread(list, recentIds),
        HomeTab.Rooms to recentUnread(list, recentIds),
    )
    val open: (RoomSummary) -> Unit = { room -> nav.open(room.id, room, account.recent) }

    val scope = rememberCoroutineScope()
    // The selected space's rooms are the second page; without one there is none.
    val pager = rememberPagerState { if (selected != null) 2 else 1 }
    val chatsList = rememberLazyListState()
    val spacesList = rememberLazyListState()
    val recentList = rememberLazyListState()
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
            nav.tab == HomeTab.Spaces -> pager.animateScrollToPage(1)
            else -> pager.scrollToPage(1)
        }
    }
    val onSpaceRooms = nav.tab == HomeTab.Spaces && pager.currentPage == 1 && selected != null
    BackHandler(enabled = onSpaceRooms, onBack = showSpaces)

    val chat = nav.chat.takeIf { nav.tab == HomeTab.Chats }
    val room = last.takeIf { nav.tab == HomeTab.Rooms && nav.inRoom }
    BackHandler(enabled = chat != null) { nav.chat = null }
    BackHandler(enabled = room != null) { nav.inRoom = false }

    val selectedSpace = spaceEntries.firstOrNull { it.entry == selected }
    val selectedSpaceId = (selected as? SpaceEntry.Space)?.id
    val title: @Composable () -> Unit = {
        when {
            onSpaceRooms -> SpaceTitle(
                selectedSpace?.name.orEmpty(),
                onClick = selectedSpaceId?.let { id -> { routes.openOverview(id) } },
            )
            nav.tab == HomeTab.Rooms -> Text("Recent")
            else -> Text(nav.tab.label)
        }
    }
    HomeFrame(
        tab = nav.tab,
        badges = badges,
        onTab = nav::tapTab,
        snackbar = snackbar,
        // A room shows its own header.
        showTopBar = chat == null && room == null,
        title = title,
        navigationIcon = {
            if (onSpaceRooms) {
                IconButton(onClick = showSpaces) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Spaces")
                }
            }
        },
        actions = {
            if (nav.tab == HomeTab.Spaces) {
                if (onSpaceRooms) SortMenu(sort, onSort = account.roomSort::choose)
                val space = (selected as? SpaceEntry.Space)?.id?.takeIf { onSpaceRooms }
                AddMenu(
                    spaceName = selectedSpace?.name.takeIf { space != null },
                    onNewSpace = routes.newSpace,
                    onJoin = routes.join,
                    onNewRoom = space?.let { { routes.newRoom(it) } },
                )
            }
        },
    ) {
        ConnectionBanner(connection, onSignInAgain = { account.forget() })
        val listShown = nav.tab != HomeTab.Settings && chat == null && room == null
        // Settings says the same in its Encryption section.
        if (listShown && showsEncryptionBanner(encryption)) {
            EncryptionBanner(onClick = { nav.tab = HomeTab.Settings })
        }
        when (nav.tab) {
            HomeTab.Chats -> if (chat != null) {
                roomPane(chat, 0L) { nav.chat = null }
            } else {
                ChatList(
                    chatRooms,
                    list.loaded,
                    images,
                    today,
                    onOpenRoom = open,
                    presenceOf = { directPerson(it, info.userId)?.let { person -> presences[person] } },
                    state = chatsList,
                )
            }
            HomeTab.Spaces -> HorizontalPager(pager, Modifier.fillMaxSize()) { page ->
                if (page == 0) {
                    SpaceList(spaceEntries, selected, list.loaded, images, state = spacesList, onSelect = { entry ->
                        if (entry == selected) {
                            scope.launch { pager.animateScrollToPage(1) }
                        } else {
                            account.spaceSelection.select(entry)
                        }
                    })
                } else {
                    SpaceRoomsList(sections, list.loaded, images, today, onOpenRoom = open, state = spaceRoomsList)
                }
            }
            HomeTab.Rooms -> if (room != null) {
                roomPane(room, recentUnread(list, recentIds, except = room)) { nav.inRoom = false }
            } else {
                RecentList(
                    recent,
                    images,
                    today,
                    onOpenRoom = open,
                    onBrowseSpaces = {
                        nav.tab = HomeTab.Spaces
                        showSpaces()
                    },
                    state = recentList,
                )
            }
            HomeTab.Settings -> SettingsContent(account, push, presence, info, onEnterRecoveryKey, onVerify)
        }
    }
}

/**
 * The home's frame: the top bar (left out while a room shows its own),
 * the content, and the tabs with their unread [badges].
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun HomeFrame(
    tab: HomeTab,
    badges: Map<HomeTab, Long>,
    onTab: (HomeTab) -> Unit,
    snackbar: SnackbarHostState = remember { SnackbarHostState() },
    showTopBar: Boolean = true,
    title: @Composable () -> Unit = { Text(tab.label) },
    navigationIcon: @Composable () -> Unit = {},
    actions: @Composable () -> Unit = {},
    content: @Composable () -> Unit,
) {
    Scaffold(
        topBar = {
            if (showTopBar) {
                TopAppBar(title = title, navigationIcon = navigationIcon, actions = { actions() })
            }
        },
        bottomBar = {
            NavigationBar {
                HomeTab.entries.forEach {
                    val count = badges[it] ?: 0L
                    NavigationBarItem(
                        selected = tab == it,
                        onClick = { onTab(it) },
                        icon = {
                            BadgedBox(badge = { if (count > 0) UnreadBadge(count) }) {
                                Icon(it.icon, contentDescription = null)
                            }
                        },
                        label = { Text(it.label) },
                    )
                }
            }
        },
        snackbarHost = { SnackbarHost(snackbar) },
        // The top bar and the tabs take the system bars; a room's header the status bar.
        contentWindowInsets = WindowInsets(0, 0, 0, 0),
    ) { padding ->
        Column(
            Modifier
                .padding(padding)
                .consumeWindowInsets(padding)
                .fillMaxSize(),
        ) {
            content()
        }
    }
}

/** The space's name over its rooms, tapped for its overview (none for Other rooms). */
@Composable
fun SpaceTitle(name: String, onClick: (() -> Unit)?) {
    Row(
        Modifier
            .clip(RoundedCornerShape(8.dp))
            .let { if (onClick != null) it.clickable(onClick = onClick) else it }
            .padding(end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(name, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
        if (onClick != null) Icon(Icons.Filled.KeyboardArrowDown, contentDescription = "Space overview")
    }
}

/** +: New space…, Join a space or room…, and inside a space New room in it…. */
@Composable
private fun AddMenu(spaceName: String?, onNewSpace: () -> Unit, onJoin: () -> Unit, onNewRoom: (() -> Unit)?) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { open = true }) {
            Icon(Icons.Filled.Add, contentDescription = "New or join")
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            if (onNewRoom != null && spaceName != null) {
                DropdownMenuItem(text = { Text("New room in $spaceName…") }, onClick = {
                    open = false
                    onNewRoom()
                })
            }
            DropdownMenuItem(text = { Text("New space…") }, onClick = {
                open = false
                onNewSpace()
            })
            DropdownMenuItem(text = { Text("Join a space or room…") }, onClick = {
                open = false
                onJoin()
            })
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
