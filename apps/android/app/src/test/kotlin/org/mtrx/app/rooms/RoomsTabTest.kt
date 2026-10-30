package org.mtrx.app.rooms

import java.time.Instant
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
import org.mtrx.app.FakeSession
import org.mtrx.app.SpacesFixture
import org.mtrx.app.SpacesFixture.DEEP
import org.mtrx.app.SpacesFixture.LAB
import org.mtrx.app.SpacesFixture.LOOSE
import org.mtrx.app.SpacesFixture.OFFER
import org.mtrx.app.SpacesFixture.OTHER
import org.mtrx.app.SpacesFixture.PARTY
import org.mtrx.app.SpacesFixture.TEAM
import org.mtrx.app.SpacesFixture.WITH_BOB
import org.mtrx.app.model.SpaceEntry
import org.mtrx.app.testAccount
import org.mtrx.app.timeline.roomHeader
import org.mtrx.app.ui.HomeNav
import org.mtrx.app.ui.HomeTab
import uniffi.mtrx.Draft
import uniffi.mtrx.RoomListState
import uniffi.mtrx.SpaceRooms

@OptIn(ExperimentalCoroutinesApi::class)
class RoomsTabTest {
    private fun TestScope.scope() = CoroutineScope(UnconfinedTestDispatcher(testScheduler))

    private val state = SpacesFixture.state()

    /** [state] with Deep's latest message after Lab's (Offer has none). */
    private val active = state.copy(
        rooms = state.rooms.map {
            when (it.id) {
                LAB -> it.copy(lastActivity = Instant.ofEpochSecond(100))
                DEEP -> it.copy(lastActivity = Instant.ofEpochSecond(200))
                else -> it
            }
        },
    )

    @Test
    fun homeComesFirstThenTheSpacesEachNewestFirst() {
        val groups = roomGroups(active)
        // Other lists only the Notes to self: left out.
        assertEquals(listOf(null, TEAM), groups.map { it.space?.id })
        assertEquals(listOf(HOME_NAME, "team"), groups.map { it.name })
        // Invites too; never active last, in list order; no direct chats nor spaces.
        assertEquals(listOf(PARTY, LOOSE), groups[0].rooms.map { it.id })
        assertEquals(listOf(DEEP, LAB, OFFER), groups[1].rooms.map { it.id })
        // Lab's unread message.
        assertEquals(listOf(0L, 1L), groups.map { it.unread })
    }

    @Test
    fun aRoomInSeveralSpacesIsInEachInTheSpacesOrder() {
        val both = active.copy(spaces = listOf(SpaceRooms(OTHER, listOf(LAB)), active.spaces[1]))
        val groups = roomGroups(both)
        assertEquals(listOf(null, OTHER, TEAM), groups.map { it.space?.id })
        assertEquals(listOf(LAB), groups[1].rooms.map { it.id })
        assertTrue(LAB in groups[2].rooms.map { it.id })
    }

    @Test
    fun onlyTheRoomsSharedWithSomeone() {
        val groups = roomGroups(active, only = setOf(LAB, WITH_BOB, TEAM, LOOSE))
        assertEquals(listOf(listOf(LOOSE), listOf(LAB)), groups.map { group -> group.rooms.map { it.id } })
        assertTrue(roomGroups(active, only = setOf(WITH_BOB)).isEmpty())
    }

    @Test
    fun theOpenRoomShowsWhileListed() {
        assertEquals(LAB, shownRoom(state, LAB))
        assertNull(shownRoom(state, null))
        assertNull(shownRoom(state, "!gone:x"))
        // Not on the Rooms tab.
        assertNull(shownRoom(state, WITH_BOB))
        assertNull(shownRoom(state, TEAM))
        // Before the list is loaded.
        assertEquals(LAB, shownRoom(RoomListState(rooms = emptyList(), loaded = false), LAB))
        // Just joined, not listed yet.
        assertEquals("!new:x", shownRoom(state, "!new:x", opening = "!new:x"))
    }

    // #59: on iOS an open room showed another's messages, then another's
    // header, as the list re-sorted. Here the open room is its ID, and its
    // screen (header, timeline model) is keyed by it.
    @Test
    fun theOpenRoomStaysAsTheListReSorts() {
        val nav = HomeNav()
        nav.open(LAB, active.rooms.first { it.id == LAB })
        fun latest(list: RoomListState, id: String, at: Long) = list.copy(
            rooms = listOf(list.rooms.first { it.id == id }.copy(lastActivity = Instant.ofEpochSecond(at))) +
                list.rooms.filter { it.id != id },
        )
        // A message arrives in Deep, then ours is sent in Lab.
        val arrived = latest(active, DEEP, 300)
        val sent = latest(arrived, LAB, 400)
        assertEquals(listOf(DEEP, LAB, OFFER), roomGroups(arrived)[1].rooms.map { it.id })
        assertEquals(listOf(LAB, DEEP, OFFER), roomGroups(sent)[1].rooms.map { it.id })
        for (list in listOf(arrived, sent)) {
            val shown = shownRoom(list, nav.room, nav.opening)
            assertEquals(LAB, shown)
            val header = roomHeader(LAB, list.rooms.firstOrNull { it.id == shown }, FakeSession.USER_ID, space = roomSpace(list, LAB))
            assertEquals("lab", header.name)
            assertEquals(TEAM, header.space?.id)
        }
        assertEquals(HomeTab.Rooms, nav.tab)
    }

    @Test
    fun foldedGroupsKeepTheirRoomsAndCount() {
        val groups = roomGroups(active, folded = setOf(TEAM, "!gone:x"))
        assertEquals(listOf(false, true), groups.map { it.folded })
        assertEquals(listOf(DEEP, LAB, OFFER), groups[1].rooms.map { it.id })
        assertEquals(1L, groups[1].unread)
        // Home by the Spaces tab's key for it.
        assertEquals(listOf(true, false), roomGroups(active, folded = setOf(SpaceEntry.HOME)).map { it.folded })
        assertEquals(SpaceEntry.Home.key, groups[0].key)
    }

    @Test
    fun unreadCountsOfTheTabsAndOfTheBackArrow() {
        val unread = state.copy(rooms = state.rooms.map { if (it.id == LOOSE) it.copy(unreadMessages = 4uL) else it })
        // Lab has 1, Loose 4 (outside spaces); Bob's chat 2 (a direct chat).
        assertEquals(5L, roomsUnread(unread))
        assertEquals(1L, spacesUnread(unread))
        assertEquals(2L, chatsUnread(unread))
        // A room's back arrow: everywhere else.
        assertEquals(6L, unreadElsewhere(unread, except = LAB))
        assertEquals(5L, unreadElsewhere(unread, except = WITH_BOB))
    }

    @Test
    fun directChatsOpenOnChatsAndRoomsOnRooms() {
        val nav = HomeNav()
        nav.open(WITH_BOB, state.rooms.first { it.id == WITH_BOB })
        assertEquals(HomeTab.Chats, nav.tab)
        assertEquals(WITH_BOB, nav.chat)
        assertNull(nav.room)

        nav.open(LAB, state.rooms.first { it.id == LAB })
        assertEquals(HomeTab.Rooms, nav.tab)
        assertEquals(LAB, nav.room)
        // An invite opens there too (its invite screen).
        nav.open(PARTY, state.rooms.first { it.id == PARTY })
        assertEquals(PARTY, nav.room)
        assertNull(nav.opening)
        // A room not listed yet, just joined.
        nav.open("!new:x", null)
        assertEquals("!new:x", nav.opening)
    }

    @Test
    fun theRoomsTabOpensOnItsListAndTappingItAgainGoesBackThere() {
        val nav = HomeNav(tab = HomeTab.Spaces)
        nav.tapTab(HomeTab.Rooms)
        assertNull(nav.room)
        nav.open(LAB, state.rooms.first { it.id == LAB })
        nav.tapTab(HomeTab.Chats)
        nav.tapTab(HomeTab.Rooms)
        assertEquals(LAB, nav.room)
        nav.tapTab(HomeTab.Rooms)
        assertNull(nav.room)
    }

    @Test
    fun aRoomsTopBarShowsItsFirstSpaceInTheSpacesOrder() {
        assertEquals(TEAM, roomSpace(state, LAB)?.id)
        // Listed by Other too, which comes first.
        val both = state.copy(spaces = listOf(SpaceRooms(OTHER, listOf(LAB)), state.spaces[1]))
        assertEquals(OTHER, roomSpace(both, LAB)?.id)
        // None outside spaces, nor for direct chats (Bob's is listed by Team).
        assertNull(roomSpace(state, LOOSE))
        assertNull(roomSpace(state, WITH_BOB))
        assertNull(roomSpace(state, "!gone:x"))
    }

    @Test
    fun spacesAreNotOpenedOnRoomsButInvitesToThemAre() {
        assertFalse(opensOnRoomsTab(state.rooms.first { it.id == TEAM }))
        assertTrue(opensOnRoomsTab(state.rooms.first { it.id == LAB }))
    }

    @Test
    fun aLeftRoomLosesItsDraft() = runTest {
        val session = FakeSession()
        val account = testAccount(FakeMtrx(restored = session), scope())
        account.start()
        session.drafts[LAB] = Draft("half a thought", null)
        account.leave(LAB, "lab")
        assertEquals(listOf("leave $LAB"), session.roomCalls)
        assertFalse(LAB in session.drafts)
    }

    @Test
    fun signingOutForgetsTheDrafts() = runTest {
        val session = FakeSession()
        val account = testAccount(FakeMtrx(restored = session), scope())
        account.start()
        session.roomListListener!!.onUpdate(state.copy(drafts = listOf(LAB)))
        assertEquals(listOf(LAB), account.rooms.value.drafts)
        account.signOut()
        // The drafts were in the session's store, gone with it.
        assertTrue(session.loggedOut)
        assertEquals(emptyList<String>(), account.rooms.value.drafts)
    }
}
