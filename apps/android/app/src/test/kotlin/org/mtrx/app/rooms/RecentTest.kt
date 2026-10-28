package org.mtrx.app.rooms

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mtrx.app.FakeMtrx
import org.mtrx.app.FakeRecentStore
import org.mtrx.app.FakeSession
import org.mtrx.app.SpacesFixture
import org.mtrx.app.SpacesFixture.DEEP
import org.mtrx.app.SpacesFixture.LAB
import org.mtrx.app.SpacesFixture.LOOSE
import org.mtrx.app.SpacesFixture.PARTY
import org.mtrx.app.SpacesFixture.TEAM
import org.mtrx.app.SpacesFixture.WITH_BOB
import org.mtrx.app.model.RecentRooms
import org.mtrx.app.testAccount
import org.mtrx.app.ui.HomeNav
import org.mtrx.app.ui.HomeTab
import uniffi.mtrx.RoomListState

@OptIn(ExperimentalCoroutinesApi::class)
class RecentTest {
    private fun TestScope.scope() = CoroutineScope(UnconfinedTestDispatcher(testScheduler))

    private val state = SpacesFixture.state()

    @Test
    fun anOpenedRoomComesFirstOnceAndIsSaved() = runTest {
        val store = FakeRecentStore()
        val recent = RecentRooms(store, scope())
        recent.opened(LAB)
        recent.opened(DEEP)
        recent.opened(LAB)
        assertEquals(listOf(LAB, DEEP), recent.ids.value)
        assertEquals(listOf(LAB, DEEP), store.saved)
        // The next launch reads them.
        assertEquals(listOf(LAB, DEEP), RecentRooms(store, scope()).ids.value)
    }

    @Test
    fun atMostTwentyAreKept() = runTest {
        val recent = RecentRooms(FakeRecentStore(), scope())
        repeat(25) { recent.opened("!r$it:x") }
        assertEquals(RecentRooms.LIMIT, recent.ids.value.size)
        assertEquals("!r24:x", recent.ids.value.first())
    }

    @Test
    fun aLeftRoomIsNoLongerRecent() = runTest {
        val session = FakeSession()
        val store = FakeRecentStore(listOf(LAB, DEEP))
        val account = testAccount(FakeMtrx(restored = session), scope(), recent = store)
        account.start()
        account.drafts[LAB] = "half a thought"
        account.leave(LAB, "lab")
        assertEquals(listOf("leave $LAB"), session.roomCalls)
        assertEquals(listOf(DEEP), account.recent.ids.value)
        assertEquals("", account.drafts[LAB])
    }

    @Test
    fun signingOutForgetsTheRecentRoomsAndDrafts() = runTest {
        val store = FakeRecentStore(listOf(LAB))
        val account = testAccount(FakeMtrx(restored = FakeSession()), scope(), recent = store)
        account.start()
        account.drafts[LAB] = "draft"
        account.signOut()
        assertEquals(emptyList<String>(), account.recent.ids.value)
        assertEquals(emptyList<String>(), store.saved)
        assertEquals("", account.drafts[LAB])
    }

    @Test
    fun recentRowsNameTheirSpaceAndLeaveOutDirectChatsAndGoneRooms() {
        val rows = recentRows(state, listOf(LOOSE, WITH_BOB, "!gone:x", LAB))
        assertEquals(listOf(LOOSE, LAB), rows.map { it.room.id })
        assertEquals(listOf(OTHER_ROOMS_NAME, "team"), rows.map { it.space })
    }

    @Test
    fun theLastRoomIsTheNewestStillListed() {
        assertEquals(LAB, lastRoom(state, listOf("!gone:x", LAB)))
        assertNull(lastRoom(state, emptyList()))
        // Before the list is loaded, the saved one.
        assertEquals(LAB, lastRoom(RoomListState(rooms = emptyList(), loaded = false), listOf(LAB)))
        // Just joined, not listed yet.
        assertEquals("!new:x", lastRoom(state, listOf("!new:x", LAB), opening = "!new:x"))
    }

    @Test
    fun unreadCountsSplitBetweenRecentRoomsAndTheRest() {
        // Lab has 1 unread; Bob's chat 2 (a direct chat: neither).
        assertEquals(1L, recentUnread(state, listOf(LAB, DEEP)))
        assertEquals(0L, recentUnread(state, listOf(LAB, DEEP), except = LAB))
        assertEquals(0L, spacesUnread(state, listOf(LAB)))
        assertEquals(1L, spacesUnread(state, listOf(DEEP)))
    }

    @Test
    fun directChatsOpenOnChatsAndRoomsOnRoomsWhereTheyBecomeRecent() = runTest {
        val recent = RecentRooms(FakeRecentStore(), scope())
        val nav = HomeNav()
        nav.open(WITH_BOB, state.rooms.first { it.id == WITH_BOB }, recent)
        assertEquals(HomeTab.Chats, nav.tab)
        assertEquals(WITH_BOB, nav.chat)
        assertTrue(recent.ids.value.isEmpty())

        nav.inRoom = false
        nav.open(LAB, state.rooms.first { it.id == LAB }, recent)
        assertEquals(HomeTab.Rooms, nav.tab)
        assertTrue(nav.inRoom)
        assertEquals(listOf(LAB), recent.ids.value)
        // An invite opens there too (its invite screen).
        nav.open(PARTY, state.rooms.first { it.id == PARTY }, recent)
        assertEquals(listOf(PARTY, LAB), recent.ids.value)
        assertNull(nav.opening)
        // A room not listed yet, just joined.
        nav.open("!new:x", null, recent)
        assertEquals("!new:x", nav.opening)
    }

    @Test
    fun tappingRoomsAgainInARoomShowsRecent() {
        val nav = HomeNav(tab = HomeTab.Spaces)
        nav.tapTab(HomeTab.Rooms)
        assertTrue(nav.inRoom)
        nav.tapTab(HomeTab.Rooms)
        assertFalse(nav.inRoom)
    }

    @Test
    fun spacesAreNotOpenedOnRoomsButInvitesToThemAre() {
        assertFalse(opensOnRoomsTab(state.rooms.first { it.id == TEAM }))
        assertTrue(opensOnRoomsTab(state.rooms.first { it.id == LAB }))
    }
}
