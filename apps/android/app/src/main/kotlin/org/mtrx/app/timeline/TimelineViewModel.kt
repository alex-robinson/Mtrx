package org.mtrx.app.timeline

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.mtrx.app.model.describe
import uniffi.mtrx.MtrxException
import uniffi.mtrx.TaskHandle
import uniffi.mtrx.Timeline
import uniffi.mtrx.TimelineItem
import uniffi.mtrx.TimelineListener

/**
 * A room's timeline screen: its rows, older events on demand, the composer's
 * draft and sending. [open] gives the room's timeline (`Session.timeline`).
 * Methods are called on the main thread.
 */
class TimelineViewModel(
    roomId: String,
    open: suspend (String) -> Timeline,
) : ViewModel() {
    private val _rows = MutableStateFlow<List<TimelineRow>>(emptyList())
    val rows: StateFlow<List<TimelineRow>> = _rows.asStateFlow()

    private val _draft = MutableStateFlow("")
    val draft: StateFlow<String> = _draft.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    private var timeline: Timeline? = null
    private var subscription: TaskHandle? = null
    private var nearTop = false
    private var paginating = false
    private var atStart = false
    private var markedRead: String? = null

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
                atStart = current.paginateBack(PAGE)
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

    fun onDraft(value: String) {
        _draft.value = value
    }

    /** Send the draft; false if there is nothing to send. */
    fun send(): Boolean {
        val current = timeline ?: return false
        val text = _draft.value
        if (text.isBlank()) return false
        _draft.value = ""
        viewModelScope.launch {
            try {
                current.sendText(text)
            } catch (e: MtrxException) {
                _error.value = describe(e)
                // Keep the text, unless a new one was started.
                if (_draft.value.isEmpty()) _draft.value = text
            }
        }
        return true
    }

    fun clearError() {
        _error.value = null
    }

    override fun onCleared() {
        subscription?.let {
            it.cancel()
            it.close()
        }
        timeline?.close()
    }

    private companion object {
        val PAGE: UShort = 30u
    }
}
