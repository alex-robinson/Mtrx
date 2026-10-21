package org.mtrx.app.rooms

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Badge
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.time.LocalDate
import org.mtrx.app.model.Account
import org.mtrx.app.model.Images
import org.mtrx.app.ui.Avatar
import org.mtrx.app.ui.lastActivity
import uniffi.mtrx.ConnectionState
import uniffi.mtrx.RoomKind
import uniffi.mtrx.RoomSummary

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RoomListScreen(account: Account, userId: String, images: Images, onOpenRoom: (String) -> Unit) {
    val list by account.rooms.collectAsStateWithLifecycle()
    val connection by account.connection.collectAsStateWithLifecycle()
    var filter by rememberSaveable { mutableStateOf(RoomFilter.All) }
    var menuOpen by remember { mutableStateOf(false) }
    var confirmSignOut by remember { mutableStateOf(false) }
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(account) { account.messages.collect { snackbar.showSnackbar(it) } }
    val rooms = remember(list, filter) { filterRooms(list.rooms, filter) }
    val today = remember(list) { LocalDate.now() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Mtrx") },
                actions = {
                    IconButton(onClick = { menuOpen = true }) {
                        Icon(Icons.Filled.MoreVert, contentDescription = "More")
                    }
                    DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                        DropdownMenuItem(text = { Text(userId) }, onClick = {}, enabled = false)
                        DropdownMenuItem(
                            text = { Text("Sign out") },
                            onClick = {
                                menuOpen = false
                                confirmSignOut = true
                            },
                        )
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        Column(
            Modifier
                .padding(padding)
                .fillMaxSize(),
        ) {
            ConnectionBanner(connection, onSignInAgain = { account.forget() })
            Row(
                Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                RoomFilter.entries.forEach {
                    FilterChip(selected = filter == it, onClick = { filter = it }, label = { Text(it.label) })
                }
            }
            when {
                rooms.isEmpty() && !list.loaded -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator()
                }
                rooms.isEmpty() -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Text("No rooms", color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                else -> LazyColumn(Modifier.fillMaxSize()) {
                    items(rooms, key = { it.id }) { room ->
                        RoomRow(room, images, today, onClick = { onOpenRoom(room.id) })
                    }
                }
            }
        }
    }

    if (confirmSignOut) {
        AlertDialog(
            onDismissRequest = { confirmSignOut = false },
            title = { Text("Sign out?") },
            text = { Text("Messages and keys of this account are deleted from this device.") },
            confirmButton = {
                TextButton(onClick = {
                    confirmSignOut = false
                    account.signOut()
                }) { Text("Sign out") }
            },
            dismissButton = {
                TextButton(onClick = { confirmSignOut = false }) { Text("Cancel") }
            },
        )
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
private fun RoomRow(room: RoomSummary, images: Images, today: LocalDate, onClick: () -> Unit) {
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
            Avatar(
                name = room.name,
                mxcUri = room.avatarUrl,
                seed = room.directUserId ?: room.id,
                images = images,
                size = 44.dp,
                // Spaces are squares, as on the desktop.
                shape = if (room.kind == RoomKind.SPACE) RoundedCornerShape(10.dp) else CircleShape,
            )
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
