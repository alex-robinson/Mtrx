package org.mtrx.app.model

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** Where the recent rooms are kept across launches. */
interface RecentStore {
    /** The saved room IDs, newest first; null if none, or they cannot be read. */
    suspend fun load(): List<String>?

    suspend fun save(ids: List<String>)
}

/**
 * The rooms opened lately on the Rooms tab, across all spaces, newest first
 * and at most [LIMIT]; the first is the last room, which the Rooms tab
 * reopens. Saved in [store] across launches. Used on [scope]'s (main) thread.
 */
class RecentRooms(private val store: RecentStore, private val scope: CoroutineScope) {
    private val _ids = MutableStateFlow<List<String>>(emptyList())
    val ids: StateFlow<List<String>> = _ids.asStateFlow()

    /** Changed before the saved list was read: that one is stale. */
    private var changed = false

    // Saves run in order and each saves the latest list.
    private val saving = Mutex()

    init {
        scope.launch {
            val saved = store.load()
            if (!changed && saved != null) _ids.value = saved.distinct().take(LIMIT)
        }
    }

    /** [roomId] was opened: it is the last room now. */
    fun opened(roomId: String) {
        update(listOf(roomId) + (_ids.value - roomId))
    }

    /** [roomId] was left: it is no longer recent. */
    fun remove(roomId: String) {
        if (roomId in _ids.value) update(_ids.value - roomId)
    }

    /** At sign-out: nothing recent, and nothing saved. */
    suspend fun clear() {
        changed = true
        _ids.value = emptyList()
        saving.withLock { store.save(emptyList()) }
    }

    private fun update(ids: List<String>) {
        changed = true
        _ids.value = ids.take(LIMIT)
        scope.launch { saving.withLock { store.save(_ids.value) } }
    }

    companion object {
        const val LIMIT = 20
    }
}

/**
 * The composer's text per room (#42): typed in one room, still there when
 * coming back to it after another. Kept in memory while signed in.
 */
class Drafts {
    private val texts = mutableMapOf<String, String>()

    operator fun get(roomId: String): String = synchronized(texts) { texts[roomId].orEmpty() }

    operator fun set(roomId: String, text: String) {
        synchronized(texts) {
            if (text.isEmpty()) texts.remove(roomId) else texts[roomId] = text
        }
    }

    fun clear() {
        synchronized(texts) { texts.clear() }
    }
}
