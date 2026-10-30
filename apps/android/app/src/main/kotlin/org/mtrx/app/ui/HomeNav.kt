package org.mtrx.app.ui

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Settings
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.vector.ImageVector
import org.mtrx.app.model.RecentRooms
import org.mtrx.app.rooms.opensOnRoomsTab
import uniffi.mtrx.RoomSummary

/** The tabs of the bottom bar. */
enum class HomeTab(val label: String, val icon: ImageVector) {
    Chats("Chats", Icons.Filled.Person),
    Rooms("Rooms", MtrxIcons.Hash),
    Spaces("Spaces", MtrxIcons.Spaces),
    Settings("Settings", Icons.Filled.Settings),
}

/**
 * Where the home is: the tab, the direct chat open on Chats, and whether
 * Rooms shows the last room (else Recent). [opening] is a room just opened
 * that the room list may not have yet (just joined or made). Kept with the
 * screen's saved state; changed on the main thread.
 */
class HomeNav(tab: HomeTab = HomeTab.Chats, chat: String? = null, inRoom: Boolean = true, opening: String? = null) {
    var tab by mutableStateOf(tab)
    var chat by mutableStateOf(chat)
    var inRoom by mutableStateOf(inRoom)
    var opening by mutableStateOf(opening)
        private set

    /**
     * Open [roomId] ([room], its summary, if listed yet): a direct chat on
     * Chats; a room or invite on Rooms, where it becomes the last room and
     * joins [recent].
     */
    fun open(roomId: String, room: RoomSummary?, recent: RecentRooms) {
        if (room != null && !opensOnRoomsTab(room)) {
            tab = HomeTab.Chats
            chat = roomId
            return
        }
        recent.opened(roomId)
        opening = roomId.takeIf { room == null }
        tab = HomeTab.Rooms
        inRoom = true
    }

    /** A tab tapped: tapped again on Rooms while in a room, Recent. */
    fun tapTab(tapped: HomeTab) {
        if (tapped == tab && tapped == HomeTab.Rooms) inRoom = false
        if (tapped == tab && tapped == HomeTab.Chats) chat = null
        tab = tapped
    }

    companion object {
        val Saver: Saver<HomeNav, Any> = listSaver(
            save = { listOf(it.tab.name, it.chat, it.inRoom, it.opening) },
            restore = {
                HomeNav(
                    tab = HomeTab.valueOf(it[0] as String),
                    chat = it[1] as String?,
                    inRoom = it[2] as Boolean,
                    opening = it[3] as String?,
                )
            },
        )
    }
}
