package org.mtrx.app.model

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.mtrx.app.FakeMtrx
import org.mtrx.app.FakeSession
import org.mtrx.app.FakeSortStore
import org.mtrx.app.FakeSpaceStore
import org.mtrx.app.SpacesFixture
import org.mtrx.app.SpacesFixture.TEAM
import uniffi.mtrx.RoomListState

@OptIn(ExperimentalCoroutinesApi::class)
class SpaceSelectionTest {
    private val noList = RoomListState(rooms = emptyList(), loaded = false)

    private fun TestScope.scope() = CoroutineScope(UnconfinedTestDispatcher(testScheduler))

    @Test
    fun withoutASavedEntryNothingIsSelected() = runTest {
        val selection = SpaceSelection(FakeSpaceStore(), MutableStateFlow(SpacesFixture.state()), scope())
        assertNull(selection.selected.value)
    }

    @Test
    fun theSelectedSpaceIsSavedAndRestoredAtTheNextLaunch() = runTest {
        val store = FakeSpaceStore()
        val first = SpaceSelection(store, MutableStateFlow(SpacesFixture.state()), scope())
        first.select(SpaceEntry.Space(TEAM))
        assertEquals(SpaceEntry.Space(TEAM), first.selected.value)
        assertEquals(SpaceEntry.Space(TEAM), store.saved)

        // The next launch starts without a list, then gets the cached one.
        val rooms = MutableStateFlow(noList)
        val next = SpaceSelection(store, rooms, scope())
        assertNull(next.selected.value)
        rooms.value = SpacesFixture.state(loaded = false)
        assertEquals(SpaceEntry.Space(TEAM), next.selected.value)
        rooms.value = SpacesFixture.state()
        assertEquals(SpaceEntry.Space(TEAM), next.selected.value)
    }

    @Test
    fun aSavedSpaceThatIsGoneIsForgottenOnceTheListIsLoaded() = runTest {
        val left = SpaceEntry.Space("!left:x")
        val store = FakeSpaceStore(left)
        val rooms = MutableStateFlow(SpacesFixture.state(loaded = false))
        val selection = SpaceSelection(store, rooms, scope())
        assertNull(selection.selected.value)
        assertEquals("kept while the list may be a cache", left, store.saved)

        rooms.value = SpacesFixture.state()
        assertNull(selection.selected.value)
        assertNull(store.saved)
    }

    @Test
    fun aSelectedEntryThatDisappearsIsDeselectedAndForgotten() = runTest {
        val store = FakeSpaceStore()
        val rooms = MutableStateFlow(SpacesFixture.state())
        val selection = SpaceSelection(store, rooms, scope())
        selection.select(SpaceEntry.OtherRooms)
        assertEquals(SpaceEntry.OtherRooms, selection.selected.value)
        assertEquals(SpaceEntry.OtherRooms, store.saved)

        rooms.value = SpacesFixture.state().copy(roomsOutsideSpaces = emptyList())
        assertNull(selection.selected.value)
        assertNull(store.saved)
        // Not selected again when it comes back.
        rooms.value = SpacesFixture.state()
        assertNull(selection.selected.value)
    }

    @Test
    fun signingOutForgetsTheSelectedSpace() = runTest {
        val store = FakeSpaceStore(SpaceEntry.Space(TEAM))
        val session = FakeSession()
        val account = Account(FakeMtrx(restored = session), scope(), store, FakeSortStore())
        account.start()
        session.roomListListener!!.onUpdate(SpacesFixture.state())
        assertEquals("restored", SpaceEntry.Space(TEAM), account.spaceSelection.selected.value)

        account.signOut()
        assertNull(account.spaceSelection.selected.value)
        assertNull(store.saved)
    }
}
