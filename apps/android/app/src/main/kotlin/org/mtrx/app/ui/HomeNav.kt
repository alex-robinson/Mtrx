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
 * Where the home is: the tab, the direct chat open on Chats and the room
 * open on Rooms (null for their lists). [opening] is a room just opened
 * that the room list may not have yet (just joined or made). Kept with the
 * screen's saved state; changed on the main thread.
 */
class HomeNav(tab: HomeTab = HomeTab.Chats, chat: String? = null, room: String? = null, opening: String? = null) {
    var tab by mutableStateOf(tab)
    var chat by mutableStateOf(chat)
    var room by mutableStateOf(room)
    var opening by mutableStateOf(opening)
        private set

    /** Open [roomId] ([room], its summary, if listed yet): a direct chat on Chats; a room or invite on Rooms. */
    fun open(roomId: String, room: RoomSummary?) {
        if (room != null && !opensOnRoomsTab(room)) {
            tab = HomeTab.Chats
            chat = roomId
            return
        }
        opening = roomId.takeIf { room == null }
        tab = HomeTab.Rooms
        this.room = roomId
    }

    /** A tab tapped: tapped again on Chats or Rooms, back to its list. */
    fun tapTab(tapped: HomeTab) {
        if (tapped == tab && tapped == HomeTab.Rooms) room = null
        if (tapped == tab && tapped == HomeTab.Chats) chat = null
        tab = tapped
    }

    companion object {
        val Saver: Saver<HomeNav, Any> = listSaver(
            save = { listOf(it.tab.name, it.chat, it.room, it.opening) },
            restore = {
                HomeNav(
                    tab = HomeTab.valueOf(it[0] as String),
                    chat = it[1] as String?,
                    room = it[2] as String?,
                    opening = it[3] as String?,
                )
            },
        )
    }
}
