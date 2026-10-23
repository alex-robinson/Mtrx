package org.mtrx.app.timeline

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mtrx.app.FakeSession
import org.mtrx.app.FakeTimeline
import org.mtrx.app.messageEvent
import org.mtrx.app.textEvent
import uniffi.mtrx.Message
import uniffi.mtrx.MessageKind
import uniffi.mtrx.MtrxException
import uniffi.mtrx.QuoteState
import uniffi.mtrx.ReplyQuote
import uniffi.mtrx.TimelineItem
import uniffi.mtrx.TimelineItemContent
import uniffi.mtrx.UserProfile

@OptIn(ExperimentalCoroutinesApi::class)
class TimelineViewModelTest {
    private val timeline = FakeTimeline()
    private val session = FakeSession().apply { roomTimeline = timeline }

    @Before
    fun mainDispatcher() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
    }

    @After
    fun resetMainDispatcher() {
        Dispatchers.resetMain()
    }

    private var reads = 0

    private fun viewModel() = TimelineViewModel("!room:example.org", session, onRead = { reads++ })

    @Test
    fun personInfoOpensOnceAndCloses() = runTest {
        session.profile = UserProfile(userId = "@bob:x", name = "Bob", avatarUrl = null)
        val viewModel = viewModel()
        viewModel.showPersonInfo("@bob:x")
        val model = viewModel.personInfo.value!!
        assertEquals("@bob:x", model.userId)
        assertEquals("Bob", model.profile.value?.name)
        // Tapped again while open: the same sheet.
        viewModel.showPersonInfo("@bob:x")
        assertTrue(viewModel.personInfo.value === model)
        viewModel.closePersonInfo()
        assertNull(viewModel.personInfo.value)
    }

    @Test
    fun updatesBecomeRows() = runTest {
        val viewModel = viewModel()
        timeline.listener!!.onUpdate(listOf(textEvent("1", "@alice:example.org", "hi")))
        assertEquals(listOf("1"), viewModel.rows.value.map { it.id })
    }

    @Test
    fun sendsTheDraftAndKeepsItOnFailure() = runTest {
        val viewModel = viewModel()
        assertFalse(viewModel.send())
        viewModel.onDraft("hello")
        assertTrue(viewModel.send())
        assertEquals(listOf("hello"), timeline.sent)
        assertEquals("", viewModel.draft.value)

        timeline.sendError = MtrxException.Other("offline")
        viewModel.onDraft("again")
        viewModel.send()
        assertEquals("again", viewModel.draft.value)
        assertEquals("offline", viewModel.error.value)
    }

    @Test
    fun loadsOlderEventsUntilTheStart() = runTest {
        val viewModel = viewModel()
        viewModel.onNearTop(true)
        // The fake reaches the start with the first page.
        viewModel.onNearTop(true)
        assertEquals(1, timeline.pages)
    }

    @Test
    fun marksTheNewestMessageReadOnce() = runTest {
        val viewModel = viewModel()
        timeline.listener!!.onUpdate(listOf(textEvent("1", "@alice:example.org", "hi")))
        viewModel.markRead()
        viewModel.markRead()
        assertEquals(1, timeline.markedRead)
        // The room's notifications go once it is read.
        assertEquals(1, reads)
    }

    @Test
    fun aFailedMarkReadKeepsTheNotifications() = runTest {
        val viewModel = viewModel()
        timeline.markReadError = MtrxException.Other("offline")
        timeline.listener!!.onUpdate(listOf(textEvent("1", "@alice:example.org", "hi")))
        viewModel.markRead()
        assertEquals(0, reads)
    }

    private val alice = "@alice:example.org"
    private val me = "@me:example.org"

    /** Our message "**bold**", shown as "bold". */
    private val ours = messageEvent(
        "mine",
        me,
        Message(kind = MessageKind.TEXT, body = "bold", edited = false, media = null, rawBody = "**bold**"),
        own = true,
        editable = true,
    )

    private fun show(vararg items: TimelineItem) = timeline.listener!!.onUpdate(items.toList())

    @Test
    fun repliesFromTheComposer() = runTest {
        val viewModel = viewModel()
        show(textEvent("1", alice, "shall we?\nat noon"))
        viewModel.startReply("1")
        assertEquals(Composing.Replying("1", "alice", "shall we?"), viewModel.composing.value)
        viewModel.onDraft("yes")
        assertTrue(viewModel.send())
        assertEquals(listOf("1" to "yes"), timeline.replies)
        assertEquals(emptyList<String>(), timeline.sent)
        assertNull(viewModel.composing.value)
        assertEquals("", viewModel.draft.value)

        // A refused reply comes back to the composer.
        timeline.actionError = MtrxException.Other("refused")
        viewModel.startReply("1")
        viewModel.onDraft("again")
        viewModel.send()
        assertEquals("refused", viewModel.error.value)
        assertEquals(Composing.Replying("1", "alice", "shall we?"), viewModel.composing.value)
        assertEquals("again", viewModel.draft.value)

        viewModel.cancelComposing()
        assertNull(viewModel.composing.value)
        assertEquals("again", viewModel.draft.value)
    }

    @Test
    fun aMessageNotSentYetCannotBeAnswered() = runTest {
        val viewModel = viewModel()
        show(messageEvent("unsent", "@me:example.org", Message(kind = MessageKind.TEXT, body = "soon", edited = false, media = null), own = true, eventId = null))
        viewModel.startReply("unsent")
        assertNull(viewModel.composing.value)
    }

    @Test
    fun editsStartFromTheTextWeWroteAndGiveTheDraftBack() = runTest {
        val viewModel = viewModel()
        show(textEvent("1", alice, "hi"), ours)
        viewModel.onDraft("unsent")
        viewModel.startEdit("mine")
        assertEquals(Composing.Editing("mine", "**bold**", "unsent"), viewModel.composing.value)
        assertEquals("**bold**", viewModel.draft.value)

        // Cancelling gives back the draft.
        viewModel.cancelComposing()
        assertNull(viewModel.composing.value)
        assertEquals("unsent", viewModel.draft.value)

        // One of replying and editing at a time.
        viewModel.startEdit("mine")
        viewModel.startReply("1")
        assertEquals("unsent", viewModel.draft.value)
        assertTrue(viewModel.composing.value is Composing.Replying)
        viewModel.startEdit("mine")
        assertEquals("**bold**", viewModel.draft.value)
        assertEquals(Composing.Editing("mine", "**bold**", "unsent"), viewModel.composing.value)

        viewModel.onDraft("**bolder**")
        assertTrue(viewModel.send())
        assertEquals(listOf("mine" to "**bolder**"), timeline.edits)
        assertNull(viewModel.composing.value)
        assertEquals("unsent", viewModel.draft.value)

        // Unchanged: nothing to send.
        viewModel.startEdit("mine")
        assertTrue(viewModel.send())
        assertEquals(1, timeline.edits.size)
    }

    @Test
    fun actionsTakeTheRowId() = runTest {
        val viewModel = viewModel()
        show(ours)
        viewModel.react("mine", "👍")
        viewModel.delete("mine")
        viewModel.retry("mine")
        viewModel.discard("mine")
        assertEquals(listOf("mine" to "👍"), timeline.reactions)
        assertEquals(listOf("mine"), timeline.deleted)
        assertEquals(listOf("mine"), timeline.retried)
        assertEquals(listOf("mine"), timeline.discarded)

        // A row gone meanwhile.
        timeline.actionError = MtrxException.InvalidInput("no message `mine` in the timeline")
        viewModel.react("mine", "👍")
        assertEquals(TimelineViewModel.GONE, viewModel.error.value)
    }

    @Test
    fun typingGoesBothWays() = runTest {
        val viewModel = viewModel()
        assertEquals(emptyList<String>(), viewModel.typing.value)
        timeline.typingListener!!.onUpdate(listOf("Anna", "Ben"))
        assertEquals(listOf("Anna", "Ben"), viewModel.typing.value)

        viewModel.onDraft("h")
        viewModel.onDraft("hi")
        viewModel.onDraft("")
        viewModel.onDraft("")
        assertEquals(listOf(true, true, false), timeline.typing)
        viewModel.onDraft("hello")
        viewModel.send()
        assertEquals(listOf(true, true, false, true, false), timeline.typing)
    }

    @Test
    fun leavingTheRoomStopsTypingButNotSending() = runTest {
        val store = ViewModelStore()
        val factory = viewModelFactory { initializer { TimelineViewModel("!room:example.org", session, onRead = {}) } }
        val viewModel = ViewModelProvider(store, factory)[TimelineViewModel::class.java]
        val gate = CompletableDeferred<Unit>()
        timeline.gate = gate
        viewModel.onDraft("hello")
        viewModel.onDraft("hello!")
        // Not sent: typing goes on.
        viewModel.onDraft("bye")
        viewModel.send()

        viewModel.onDraft("unsent")
        store.clear()
        assertTrue(timeline.typingSubscription.cancelled)
        assertEquals(false, timeline.typing.last())

        gate.complete(Unit)
        assertEquals(listOf("bye"), timeline.sent)
        assertTrue(timeline.closed)
    }

    private val original = textEvent("0", alice, "the original")

    private val reply = messageEvent(
        "1",
        alice,
        Message(
            kind = MessageKind.TEXT,
            body = "the reply",
            edited = false,
            media = null,
            replyTo = ReplyQuote("\$0", QuoteState.LOADED, alice, null, MessageKind.TEXT, "the original"),
        ),
    )

    private val start = TimelineItem("start", TimelineItemContent.Start)

    @Test
    fun goesToAnOriginalLoadedAlready() = runTest {
        val viewModel = viewModel()
        show(original, reply)
        viewModel.goToOriginal("\$0")
        assertEquals(0, timeline.pages)
        assertEquals("0", viewModel.highlighted.value)
        assertFalse(viewModel.seeking.value)
        // Marked for two seconds.
        advanceTimeBy(TimelineViewModel.HIGHLIGHT_MILLIS + 1)
        assertNull(viewModel.highlighted.value)
    }

    @Test
    fun loadsOlderEventsUntilTheOriginalIsThere() = runTest {
        val viewModel = viewModel()
        show(reply)
        timeline.pagesUntilStart = 100
        timeline.onPage = { page -> if (page == 3) show(original, reply) }
        viewModel.goToOriginal("\$0")
        assertEquals(3, timeline.pages)
        assertEquals("0", viewModel.highlighted.value)
        assertFalse(viewModel.seeking.value)
        assertNull(viewModel.error.value)
    }

    @Test
    fun findsAnOriginalArrivingAfterThePage() = runTest {
        val viewModel = viewModel()
        show(reply)
        // The start is reached, its items come later.
        viewModel.goToOriginal("\$0")
        assertEquals(1, timeline.pages)
        assertTrue(viewModel.seeking.value)
        show(start, original, reply)
        assertEquals("0", viewModel.highlighted.value)
        assertFalse(viewModel.seeking.value)
    }

    @Test
    fun stopsAtTheRoomsStart() = runTest {
        val viewModel = viewModel()
        show(reply)
        timeline.pagesUntilStart = 2
        timeline.onPage = { page -> if (page == 2) show(start, reply) }
        viewModel.goToOriginal("\$0")
        assertEquals(2, timeline.pages)
        assertEquals(TimelineViewModel.NOT_FOUND, viewModel.error.value)
        assertNull(viewModel.highlighted.value)
        assertFalse(viewModel.seeking.value)

        // At the start already: no more pages.
        viewModel.clearError()
        viewModel.goToOriginal("\$0")
        assertEquals(2, timeline.pages)
        assertEquals(TimelineViewModel.NOT_FOUND, viewModel.error.value)
    }

    @Test
    fun stopsAfterTwentyPages() = runTest {
        val viewModel = viewModel()
        show(reply)
        timeline.pagesUntilStart = 100
        viewModel.goToOriginal("\$0")
        assertEquals(TimelineViewModel.SEEK_PAGES, timeline.pages)
        assertTrue(viewModel.seeking.value)
        advanceUntilIdle()
        assertEquals(TimelineViewModel.SEEK_PAGES, timeline.pages)
        assertEquals(TimelineViewModel.NOT_FOUND, viewModel.error.value)
        assertFalse(viewModel.seeking.value)
    }
}
