package org.mtrx.app.rooms

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
import org.mtrx.app.model.SpaceEntry

class RoomListsTest {
    private val state = SpacesFixture.state()

    @Test
    fun chatsAreTheDirectChatsInListOrder() {
        val chats = chats(state)
        assertEquals(listOf(WITH_CAROL, WITH_BOB, NOTES), chats.map { it.id })
        assertTrue("Carol's is an invite", chats.first().invited)
    }

    @Test
    fun spacesPageListsTopLevelSpacesWithUnreadDotsThenOtherRooms() {
        val rows = spaceRows(state)
        assertEquals(
            listOf(SpaceEntry.Space(OTHER), SpaceEntry.Space(TEAM), SpaceEntry.OtherRooms),
            rows.map { it.entry },
        )
        assertEquals(listOf("other", "team", "Other rooms"), rows.map { it.name })
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
    fun otherRoomsListsRoomsOutsideSpacesAndIsLeftOutWhileEmpty() {
        val other = roomsOf(state, SpaceEntry.OtherRooms)
        assertEquals(listOf(PARTY, LOOSE), other.map { it.id })
        assertTrue("the invite to Party", other.first().invited)

        val inSpaces = state.copy(roomsOutsideSpaces = emptyList())
        assertEquals(listOf(SpaceEntry.Space(OTHER), SpaceEntry.Space(TEAM)), spaceRows(inSpaces).map { it.entry })
        assertEquals(emptyList<String>(), roomsOf(inSpaces, SpaceEntry.OtherRooms).map { it.id })
    }

    @Test
    fun anUnreadRoomOutsideSpacesDotsOtherRooms() {
        val rooms = state.rooms.map { if (it.id == LOOSE) it.copy(unreadMessages = 1uL) else it }
        assertTrue(spaceRows(state.copy(rooms = rooms)).last().unread)
    }

    @Test
    fun entriesAreSavedByKey() {
        for (entry in listOf(SpaceEntry.Space(TEAM), SpaceEntry.OtherRooms)) {
            assertEquals(entry, SpaceEntry.fromKey(entry.key))
        }
    }
}
