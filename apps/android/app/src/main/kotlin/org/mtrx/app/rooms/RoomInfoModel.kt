package org.mtrx.app.rooms

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.mtrx.app.model.describe
import uniffi.mtrx.Access
import uniffi.mtrx.History
import uniffi.mtrx.Member
import uniffi.mtrx.MtrxException
import uniffi.mtrx.NotifyMode
import uniffi.mtrx.Permissions
import uniffi.mtrx.RoomSettings
import uniffi.mtrx.Session
import uniffi.mtrx.Topic

/**
 * Room info of [roomId] (a room or a space): its settings and members, and
 * the changes to them. Each change reads the settings (or members) again
 * after it, as the library returns once sync brought it back. [composeTopic]
 * makes the topic to save from its text and links (the library's). Methods
 * are called on the main thread.
 */
class RoomInfoModel(
    val roomId: String,
    private val session: Session,
    private val composeTopic: (Topic) -> String,
) : ViewModel(), RoomInfoActions {
    private val _settings = MutableStateFlow<RoomSettings?>(null)
    val settings: StateFlow<RoomSettings?> = _settings.asStateFlow()

    private val _members = MutableStateFlow<List<Member>?>(null)
    val members: StateFlow<List<Member>?> = _members.asStateFlow()

    /** A change under way, as "Saving…". */
    private val _busy = MutableStateFlow<String?>(null)
    val busy: StateFlow<String?> = _busy.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    init {
        loadSettings()
        loadMembers()
    }

    fun loadSettings() {
        viewModelScope.launch {
            try {
                _settings.value = session.roomSettings(roomId)
            } catch (e: MtrxException) {
                _error.value = describe(e)
            }
        }
    }

    fun loadMembers() {
        viewModelScope.launch {
            try {
                _members.value = session.members(roomId)
            } catch (e: MtrxException) {
                _error.value = describe(e)
            }
        }
    }

    override fun rename(name: String) {
        if (name.isBlank()) return
        change("Renaming…") { session.setRoomName(roomId, name.trim()) }
    }

    override fun setTopic(topic: Topic) = change("Saving…") { session.setRoomTopic(roomId, composeTopic(topic)) }

    override fun setNotify(mode: NotifyMode) = change("Saving…") { session.setNotifyMode(roomId, mode) }

    override fun setAccess(access: Access) = change("Saving…") { session.setRoomAccess(roomId, access) }

    override fun setListed(listed: Boolean) = change("Saving…") { session.setListed(roomId, listed) }

    override fun setSuggested(spaceId: String, suggested: Boolean) =
        change("Saving…") { session.setSuggested(spaceId, roomId, suggested) }

    override fun setHistory(history: History) = change("Saving…") { session.setHistory(roomId, history) }

    override fun setPermissions(permissions: Permissions) = change("Saving…") { session.setPermissions(roomId, permissions) }

    override fun encrypt() = change("Turning encryption on…") { session.encryptRoom(roomId) }

    /** The address `#name:ourserver`; blank removes it. */
    override fun setAddress(name: String) =
        change("Saving…") { session.setAddress(roomId, name.trim().removePrefix("#").ifEmpty { null }) }

    override fun addToSpace(spaceId: String) = change("Saving…") { session.addToSpace(spaceId, roomId) }

    override fun removeFromSpace(spaceId: String) = change("Saving…") { session.removeFromSpace(spaceId, roomId) }

    override fun unban(userId: String) = change("Saving…") { session.unban(roomId, userId) }

    override fun setLevel(userId: String, level: Long) =
        change("Changing the level…", members = true) { session.setLevel(roomId, userId, level) }

    /** Remove [userId], or also ban them; they see [reason]. */
    override fun remove(userId: String, reason: String, ban: Boolean) =
        change(if (ban) "Banning…" else "Removing…", members = true) {
            session.removeMember(roomId, userId, reason.trim().ifEmpty { null }, ban)
        }

    /** Invite [userId] (`@name:server`). */
    override fun invite(userId: String) = change("Sending the invites…", members = true) { session.invite(roomId, userId.trim()) }

    private fun change(doing: String, members: Boolean = false, call: suspend () -> Unit) {
        viewModelScope.launch {
            _busy.value = doing
            try {
                call()
                _settings.value = session.roomSettings(roomId)
                if (members) _members.value = session.members(roomId)
            } catch (e: MtrxException) {
                _error.value = describe(e)
            } finally {
                if (_busy.value == doing) _busy.value = null
            }
        }
    }

    override fun clearError() {
        _error.value = null
    }
}
