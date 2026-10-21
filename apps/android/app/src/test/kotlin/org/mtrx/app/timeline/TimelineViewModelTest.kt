package org.mtrx.app.timeline

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mtrx.app.FakeTimeline
import org.mtrx.app.textEvent
import uniffi.mtrx.MtrxException

@OptIn(ExperimentalCoroutinesApi::class)
class TimelineViewModelTest {
    private val timeline = FakeTimeline()

    @Before
    fun mainDispatcher() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
    }

    @After
    fun resetMainDispatcher() {
        Dispatchers.resetMain()
    }

    private fun viewModel() = TimelineViewModel("!room:example.org") { timeline }

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
    }
}
