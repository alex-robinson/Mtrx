package org.mtrx.app.rooms

import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mtrx.app.SpacesFixture
import org.mtrx.app.SpacesFixture.DEEP
import org.mtrx.app.SpacesFixture.LAB
import org.mtrx.app.SpacesFixture.LOOSE
import org.mtrx.app.SpacesFixture.NOTES
import org.mtrx.app.SpacesFixture.OFFER
import org.mtrx.app.SpacesFixture.OTHER
import org.mtrx.app.SpacesFixture.PARTY
import org.mtrx.app.SpacesFixture.SUB
import org.mtrx.app.SpacesFixture.TEAM
import org.mtrx.app.SpacesFixture.WITH_BOB
import org.mtrx.app.SpacesFixture.WITH_CAROL
import org.mtrx.app.model.RoomSort
import org.mtrx.app.model.SpaceEntry
import org.mtrx.app.summary
import uniffi.mtrx.RoomKind
import uniffi.mtrx.SpaceRooms

class RoomListsTest {
    private val state = SpacesFixture.state()

    @Test
    fun chatsAreTheDirectChatsInListOrder() {
        val chats = chats(state)
        assertEquals(listOf(WITH_CAROL, WITH_BOB, NOTES), chats.map { it.id })
        assertTrue("Carol's is an invite", chats.first().invited)
    }

    @Test
    fun aDirectChatsSpacesAreThoseListingItInTheSpacesOrder() {
        val both = state.copy(spaces = listOf(SpaceRooms(OTHER, listOf(WITH_BOB, NOTES)), state.spaces[1]), notesRoomId = NOTES)
        // None shared with Carol; the Notes to self left out.
        assertEquals(mapOf(WITH_CAROL to emptyList<String>(), WITH_BOB to listOf("other", "team")), sharedSpaces(both))
    }

    @Test
    fun topLevelSpacesInListOrderWithTheirSummaries() {
        val spaces = topLevelSpaces(state)
        assertEquals(listOf(OTHER, TEAM), spaces.map { it.id })
        assertEquals(listOf("other", "team"), spaces.map { it.name })
        // A space not (yet) in the rooms is left out.
        assertEquals(listOf(TEAM), topLevelSpaces(state.copy(rooms = state.rooms.filter { it.id != OTHER })).map { it.id })
    }

    @Test
    fun spacesPageListsTopLevelSpacesWithUnreadDotsThenHome() {
        val rows = spaceRows(state)
        assertEquals(
            listOf(SpaceEntry.Space(OTHER), SpaceEntry.Space(TEAM), SpaceEntry.Home),
            rows.map { it.entry },
        )
        assertEquals(listOf("other", "team", "Home"), rows.map { it.name })
        // Lab and the chat with Bob are unread in Team; nothing elsewhere.
        assertEquals(listOf(false, true, false), rows.map { it.unread })
        assertNull(rows.last().space)
    }

    @Test
    fun aSpaceListsItsRoomsInListOrderWithoutNestedSpaces() {
        val team = roomsOf(state, SpaceEntry.Space(TEAM))
        assertEquals(listOf(WITH_BOB, OFFER, LAB, DEEP, NOTES), team.map { it.id })
        assertTrue("the invite to Offer", team[1].invited)
        assertTrue(team.none { it.id == SUB })
        assertEquals(listOf(NOTES), roomsOf(state, SpaceEntry.Space(OTHER)).map { it.id })
        assertEquals(emptyList<String>(), roomsOf(state, SpaceEntry.Space(SUB)).map { it.id })
    }

    @Test
    fun homeListsRoomsOutsideSpacesAndIsLeftOutWhileEmpty() {
        val home = roomsOf(state, SpaceEntry.Home)
        assertEquals(listOf(PARTY, LOOSE), home.map { it.id })
        assertTrue("the invite to Party", home.first().invited)

        val inSpaces = state.copy(roomsOutsideSpaces = emptyList())
        assertEquals(listOf(SpaceEntry.Space(OTHER), SpaceEntry.Space(TEAM)), spaceRows(inSpaces).map { it.entry })
        assertEquals(emptyList<String>(), roomsOf(inSpaces, SpaceEntry.Home).map { it.id })
    }

    @Test
    fun anUnreadRoomOutsideSpacesDotsHome() {
        val rooms = state.rooms.map { if (it.id == LOOSE) it.copy(unreadMessages = 1uL) else it }
        assertTrue(spaceRows(state.copy(rooms = rooms)).last().unread)
    }

    @Test
    fun entriesAreSavedByKey() {
        for (entry in listOf(SpaceEntry.Space(TEAM), SpaceEntry.Home)) {
            assertEquals(entry, SpaceEntry.fromKey(entry.key))
        }
    }

    // In list order (most recently active first, by the library's rules).
    private val listed = listOf(
        summary("!b:x", name = "bravo", lastActivity = Instant.ofEpochSecond(200)),
        summary("!a:x", name = "Alpha"),
        summary("!c:x", name = "charlie", lastActivity = Instant.ofEpochSecond(300)),
        summary("!e:x", name = "echo", lastActivity = Instant.ofEpochSecond(100)),
        summary("!d:x", name = "Delta"),
    )

    @Test
    fun defaultKeepsTheListsOrder() {
        assertEquals(listOf("!b:x", "!a:x", "!c:x", "!e:x", "!d:x"), sorted(listed, RoomSort.DEFAULT).map { it.id })
    }

    @Test
    fun alphabeticalOrdersIgnoreCase() {
        assertEquals(listOf("Alpha", "bravo", "charlie", "Delta", "echo"), sorted(listed, RoomSort.A_TO_Z).map { it.name })
        assertEquals(listOf("echo", "Delta", "charlie", "bravo", "Alpha"), sorted(listed, RoomSort.Z_TO_A).map { it.name })
    }

    @Test
    fun mostRecentPutsRoomsNeverActiveLastInListOrder() {
        assertEquals(listOf("!c:x", "!b:x", "!e:x", "!a:x", "!d:x"), sorted(listed, RoomSort.RECENT).map { it.id })
    }

    @Test
    fun equalNamesKeepTheListsOrder() {
        val twins = listOf(summary("!2:x", name = "Ann"), summary("!1:x", name = "ann"), summary("!0:x", RoomKind.DIRECT, name = "Ann"))
        assertEquals(listOf("!2:x", "!1:x", "!0:x"), sorted(twins, RoomSort.A_TO_Z).map { it.id })
        assertEquals(listOf("!2:x", "!1:x", "!0:x"), sorted(twins, RoomSort.Z_TO_A).map { it.id })
    }
}
