package org.mtrx.app.rooms

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.mtrx.app.model.describe
import uniffi.mtrx.MtrxException
import uniffi.mtrx.NewRoom
import uniffi.mtrx.Session

/** The new room or space form, as typed. */
data class NewRoomForm(
    val name: String = "",
    val topic: String = "",
    /** Rooms only; on by default. */
    val encrypted: Boolean = true,
    val moderated: Boolean = false,
    /** User IDs to invite, in the order added. */
    val invite: List<String> = emptyList(),
    /** Those of [invite] to own the room as we do. */
    val coOwners: Set<String> = emptySet(),
    /** The user ID being typed. */
    val person: String = "",
) {
    val canCreate: Boolean get() = name.isNotBlank()

    /** The room to make; [parent], the space to make it in. */
    fun toNewRoom(space: Boolean, parent: String?, offersOwners: Boolean): NewRoom = NewRoom(
        name = name.trim(),
        topic = topic.trim().ifEmpty { null },
        space = space,
        encrypted = encrypted && !space,
        moderated = moderated,
        invite = invite,
        coOwners = if (offersOwners) invite.filter { it in coOwners } else emptyList(),
        parent = parent,
    )
}

/** A user ID as `@name:server`; null if [text] isn't one. */
fun userIdOf(text: String): String? {
    val id = text.trim()
    return id.takeIf { it.startsWith('@') && it.indexOf(':') > 1 && it.none { c -> c.isWhitespace() } }
}

/**
 * New room or space ([space]) in [parent], if any: name, topic, encrypted
 * (rooms), only moderators edit, people to invite and, where the server
 * offers it, which of them are co-owners. Methods are called on the main
 * thread.
 */
class NewRoomModel(
    private val session: Session,
    val space: Boolean,
    val parent: String?,
) : ViewModel() {
    private val _form = MutableStateFlow(NewRoomForm())
    val form: StateFlow<NewRoomForm> = _form.asStateFlow()

    /** New rooms may have co-owners (room version 12 offered). */
    private val _offersOwners = MutableStateFlow(false)
    val offersOwners: StateFlow<Boolean> = _offersOwners.asStateFlow()

    private val _creating = MutableStateFlow(false)
    val creating: StateFlow<Boolean> = _creating.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    init {
        viewModelScope.launch { _offersOwners.value = session.offersOwners() }
    }

    fun edit(change: (NewRoomForm) -> NewRoomForm) = _form.update(change)

    /** Add the typed user ID to those invited; false if it isn't one. */
    fun addPerson(): Boolean {
        val id = userIdOf(_form.value.person) ?: return false
        _form.update { if (id in it.invite) it.copy(person = "") else it.copy(invite = it.invite + id, person = "") }
        return true
    }

    fun removePerson(userId: String) =
        _form.update { it.copy(invite = it.invite - userId, coOwners = it.coOwners - userId) }

    fun setCoOwner(userId: String, on: Boolean) =
        _form.update { it.copy(coOwners = if (on) it.coOwners + userId else it.coOwners - userId) }

    /** Make it; [onCreated] with its room ID. */
    fun create(onCreated: (String) -> Unit) {
        if (_creating.value) return
        // A user ID typed but not added yet is invited too.
        addPerson()
        val form = _form.value
        if (!form.canCreate) return
        _creating.value = true
        viewModelScope.launch {
            try {
                onCreated(session.createRoom(form.toNewRoom(space, parent, _offersOwners.value)))
            } catch (e: MtrxException) {
                val what = if (space) "space" else "room"
                _error.value = "Could not create the $what “${form.name.trim()}”: ${describe(e)}"
            } finally {
                _creating.value = false
            }
        }
    }

    fun clearError() {
        _error.value = null
    }
}
