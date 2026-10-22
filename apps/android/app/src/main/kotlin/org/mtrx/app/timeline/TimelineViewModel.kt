package org.mtrx.app.timeline

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import org.mtrx.app.model.describe
import uniffi.mtrx.MtrxException
import uniffi.mtrx.TaskHandle
import uniffi.mtrx.Timeline
import uniffi.mtrx.TimelineItem
import uniffi.mtrx.TimelineListener
import uniffi.mtrx.TypingListener

/**
 * A room's timeline screen: its rows, older events on demand, the composer
 * (a new message, a reply or an edit), who is typing, the actions on a
 * message, and going to the original of a reply. [open] gives the room's
 * timeline (`Session.timeline`). Methods are called on the main thread.
 *
 * Actions on a message take its row's id from the latest [rows]. Sends and
 * the other actions run in a scope of their own, so leaving the room (which
 * clears this model) does not cancel them.
 */
class TimelineViewModel(
    roomId: String,
    open: suspend (String) -> Timeline,
) : ViewModel() {
    private val _rows = MutableStateFlow<List<TimelineRow>>(emptyList())
    val rows: StateFlow<List<TimelineRow>> = _rows.asStateFlow()

    private val _draft = MutableStateFlow("")
    val draft: StateFlow<String> = _draft.asStateFlow()

    /** What the draft answers or edits; null for a new message. */
    private val _composing = MutableStateFlow<Composing?>(null)
    val composing: StateFlow<Composing?> = _composing.asStateFlow()

    /** Names of the others typing. */
    private val _typing = MutableStateFlow<List<String>>(emptyList())
    val typing: StateFlow<List<String>> = _typing.asStateFlow()

    /** Older events are loaded to find a reply's original. */
    private val _seeking = MutableStateFlow(false)
    val seeking: StateFlow<Boolean> = _seeking.asStateFlow()

    /** The row gone to, marked for a moment. */
    private val _highlighted = MutableStateFlow<String?>(null)
    val highlighted: StateFlow<String?> = _highlighted.asStateFlow()

    /** Rows to scroll to. */
    private val _scrollTo = MutableSharedFlow<String>(extraBufferCapacity = 1)
    val scrollTo: SharedFlow<String> = _scrollTo.asSharedFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    private var timeline: Timeline? = null
    private var subscription: TaskHandle? = null
    private var typingSubscription: TaskHandle? = null
    private var nearTop = false
    private var paginating = false
    private var atStart = false
    private var markedRead: String? = null
    private var seek: Job? = null

    // Not cancelled with viewModelScope: completed when the room is left, it
    // ends once the actions under way are done.
    private val actionsJob = SupervisorJob()
    private val actions = CoroutineScope(actionsJob + Dispatchers.Main.immediate)

    // One pagination at a time: older events, and the search for an original.
    private val pagination = Mutex()

    // Typing notices in order, the latest only when calls wait.
    private val typingNotices = Channel<Boolean>(Channel.CONFLATED)
    private var typingOn = false

    init {
        viewModelScope.launch {
            try {
                val opened = open(roomId)
                timeline = opened
                subscription = opened.subscribe(object : TimelineListener {
                    // On a library thread.
                    override fun onUpdate(items: List<TimelineItem>) {
                        _rows.value = timelineRows(items)
                    }
                })
                typingSubscription = opened.subscribeTyping(object : TypingListener {
                    // On a library thread.
                    override fun onUpdate(names: List<String>) {
                        _typing.value = names
                    }
                })
                launch { sendTypingNotices(opened) }
                if (nearTop) loadOlder()
            } catch (e: MtrxException) {
                _error.value = describe(e)
            }
        }
    }

    /** The list is scrolled (nearly) to its top: load older events while it is. */
    fun onNearTop(value: Boolean) {
        nearTop = value
        if (value) loadOlder()
    }

    private fun loadOlder() {
        val current = timeline ?: return
        if (paginating || atStart) return
        paginating = true
        viewModelScope.launch {
            val more = try {
                paginate(current, PAGE)
                true
            } catch (e: MtrxException) {
                _error.value = describe(e)
                false
            } finally {
                paginating = false
            }
            // Short pages may not move the list away from its top.
            if (more && nearTop) loadOlder()
        }
    }

    /** Load [events] older events; true at the room's start. */
    private suspend fun paginate(current: Timeline, events: UShort): Boolean = pagination.withLock {
        current.paginateBack(events).also { if (it) atStart = true }
    }

    /** The newest message is on screen. */
    fun markRead() {
        val current = timeline ?: return
        val latest = latestEventId(_rows.value) ?: return
        if (latest == markedRead) return
        markedRead = latest
        viewModelScope.launch {
            try {
                current.markRead()
            } catch (e: MtrxException) {
                markedRead = null
            }
        }
    }

    /** The user changed the text. */
    fun onDraft(value: String) {
        _draft.value = value
        typed(value.isNotEmpty())
    }

    /** Tell the others: on every change while there is text (the library throttles), once when it is gone. */
    private fun typed(typing: Boolean) {
        if (!typing && !typingOn) return
        typingOn = typing
        typingNotices.trySend(typing)
    }

    private suspend fun sendTypingNotices(current: Timeline) {
        for (typing in typingNotices) {
            try {
                current.setTyping(typing)
            } catch (e: MtrxException) {
                // Only a hint to the others.
            }
        }
    }

    /** Reply to row [itemId]: the composer says to whom; an edit under way ends. */
    fun startReply(itemId: String) {
        val row = eventRow(itemId) ?: return
        endEdit()
        _composing.value = Composing.Replying(itemId, displayName(row.event), preview(row))
    }

    /** Edit row [itemId]: the composer gets its text as we wrote it; a reply under way ends. */
    fun startEdit(itemId: String) {
        val message = eventRow(itemId)?.message ?: return
        val original = message.rawBody.ifEmpty { message.body }
        val draft = (_composing.value as? Composing.Editing)?.draft ?: _draft.value
        _composing.value = Composing.Editing(itemId, original, draft)
        _draft.value = original
    }

    /** Back to a new message; an edit gives back the text from before it. */
    fun cancelComposing() {
        endEdit()
        _composing.value = null
    }

    private fun endEdit() {
        val editing = _composing.value as? Composing.Editing ?: return
        _composing.value = null
        _draft.value = editing.draft
    }

    /** Send the draft, as a reply or an edit if one is under way; false if there is nothing to send. */
    fun send(): Boolean {
        val current = timeline ?: return false
        val text = _draft.value
        if (text.isBlank()) return false
        val mode = _composing.value
        _composing.value = null
        _draft.value = (mode as? Composing.Editing)?.draft ?: ""
        typed(false)
        if (mode is Composing.Editing && text == mode.original) return true
        actions.launch {
            try {
                when (mode) {
                    null -> current.sendText(text)
                    is Composing.Replying -> current.reply(mode.itemId, text)
                    is Composing.Editing -> current.edit(mode.itemId, text)
                }
            } catch (e: MtrxException) {
                _error.value = describeAction(e)
                restore(text, mode)
            }
        }
        return true
    }

    /** Back in the composer after a failed send, unless another was started. */
    private fun restore(text: String, mode: Composing?) {
        if (_composing.value != null) return
        when (mode) {
            is Composing.Editing -> {
                _composing.value = mode.copy(draft = _draft.value)
                _draft.value = text
            }
            else -> if (_draft.value.isEmpty()) {
                _composing.value = mode
                _draft.value = text
            }
        }
    }

    /** Add our reaction [key] to row [itemId], or remove it. */
    fun react(itemId: String, key: String) = act { it.react(itemId, key) }

    /** Delete row [itemId]'s message for everyone. */
    fun delete(itemId: String) = act { it.delete(itemId) }

    /** Send row [itemId]'s failed message again. */
    fun retry(itemId: String) = act { it.retry(itemId) }

    /** Drop row [itemId]'s failed message. */
    fun discard(itemId: String) = act { it.discard(itemId) }

    private fun act(action: suspend (Timeline) -> Unit) {
        val current = timeline ?: return
        actions.launch {
            try {
                action(current)
            } catch (e: MtrxException) {
                _error.value = describeAction(e)
            }
        }
    }

    /**
     * Go to the original of a reply: scroll to its row and mark it, after
     * loading older events until it is there, the room's start, or
     * [SEEK_PAGES] pages. A new one replaces one under way.
     */
    fun goToOriginal(eventId: String) {
        val current = timeline ?: return
        seek?.cancel()
        // Started once it is `seek`: it may end without suspending.
        val job = viewModelScope.launch(start = CoroutineStart.LAZY) {
            val me = coroutineContext.job
            _seeking.value = true
            val found = try {
                findOriginal(current, eventId)
            } catch (e: MtrxException) {
                _error.value = describe(e)
                return@launch
            } finally {
                if (seek === me) _seeking.value = false
            }
            if (found == null) {
                _error.value = NOT_FOUND
                return@launch
            }
            _scrollTo.tryEmit(found)
            _highlighted.value = found
            delay(HIGHLIGHT_MILLIS)
            if (_highlighted.value == found) _highlighted.value = null
        }
        seek = job
        job.start()
    }

    /** The row of event [eventId], loading older events for it; null if not found. */
    private suspend fun findOriginal(current: Timeline, eventId: String): String? {
        rowOf(_rows.value, eventId)?.let { return it }
        return coroutineScope {
            // In each new list: the items arrive apart from paginateBack's return.
            val watcher = async { _rows.mapNotNull { rowOf(it, eventId) }.first() }
            val pager = async {
                loadForOriginal(current, eventId)
                rowOf(_rows.value, eventId)
            }
            val found = select<String?> {
                watcher.onAwait { it }
                pager.onAwait { it }
            }
            coroutineContext.cancelChildren()
            found
        }
    }

    /** Load older events until event [eventId] or the room's start is in the rows, or [SEEK_PAGES] pages. */
    private suspend fun loadForOriginal(current: Timeline, eventId: String) {
        repeat(SEEK_PAGES) {
            if (startLoaded() || rowOf(_rows.value, eventId) != null) return
            if (paginate(current, SEEK_PAGE)) {
                // The start's items may come after the call returned.
                withTimeoutOrNull(SETTLE_MILLIS) { _rows.first { rows -> rows.any { it is StartRow } } }
                return
            }
        }
        // The last page's items, likewise.
        delay(SETTLE_MILLIS)
    }

    private fun startLoaded(): Boolean = _rows.value.any { it is StartRow }

    private fun rowOf(rows: List<TimelineRow>, eventId: String): String? =
        rows.firstOrNull { it is EventRow && it.event.eventId == eventId }?.id

    private fun eventRow(itemId: String): EventRow? = _rows.value.firstOrNull { it.id == itemId } as? EventRow

    /** A row gone meanwhile is invalid input to the library. */
    private fun describeAction(e: MtrxException): String? =
        if (e is MtrxException.InvalidInput) GONE else describe(e)

    fun clearError() {
        _error.value = null
    }

    override fun onCleared() {
        for (handle in listOfNotNull(subscription, typingSubscription)) {
            handle.cancel()
            handle.close()
        }
        typingNotices.close()
        val current = timeline
        if (current != null) {
            actions.launch {
                try {
                    if (typingOn) current.setTyping(false)
                } catch (e: MtrxException) {
                    // Only a hint to the others.
                } finally {
                    // Calls under way keep the library's timeline until they end.
                    current.close()
                }
            }
        }
        actionsJob.complete()
    }

    companion object {
        val PAGE: UShort = 30u

        /** Events per page, and pages at most, loaded to find a reply's original. */
        val SEEK_PAGE: UShort = 50u
        const val SEEK_PAGES = 20

        /** How long to wait for the items of a last page. */
        const val SETTLE_MILLIS = 2_000L
        const val HIGHLIGHT_MILLIS = 2_000L

        const val NOT_FOUND = "The original message was not found in the room's history"
        const val GONE = "The message is no longer in the timeline"
    }
}
