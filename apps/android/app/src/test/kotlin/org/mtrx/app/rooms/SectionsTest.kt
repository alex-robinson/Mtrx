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
import org.mtrx.app.FakeSectionStore
import org.mtrx.app.FakeSession
import org.mtrx.app.model.RoomSort
import org.mtrx.app.model.SpaceEntry
import org.mtrx.app.summary
import org.mtrx.app.testAccount
import uniffi.mtrx.RoomKind
import uniffi.mtrx.RoomListState
import uniffi.mtrx.RoomSection
import uniffi.mtrx.SectionKind
import uniffi.mtrx.SpaceRooms

@OptIn(ExperimentalCoroutinesApi::class)
class SectionsTest {
    private fun TestScope.scope() = CoroutineScope(UnconfinedTestDispatcher(testScheduler))

    private val space = SpaceEntry.Space("!s:x")

    private fun state(notesAtTop: Boolean = true, extra: List<RoomSection> = emptyList()) = RoomListState(
        rooms = listOf(
            summary("!notes:x", RoomKind.DIRECT, directUserId = FakeSession.USER_ID),
            summary("!bob:x", RoomKind.DIRECT, unread = 2, directUserId = "@bob:x"),
            summary("!fav:x"),
            summary("!field:x", unread = 3, mentions = 1),
            summary("!radar:x", unread = 1),
            summary("!room:x"),
            summary("!s:x", RoomKind.SPACE),
        ),
        loaded = true,
        spaces = listOf(
            SpaceRooms(
                spaceId = "!s:x",
                roomIds = listOf("!notes:x", "!bob:x", "!fav:x", "!field:x", "!radar:x", "!room:x"),
                sections = listOf(
                    RoomSection("invites", SectionKind.INVITES, "", emptyList()),
                    RoomSection("direct_messages", SectionKind.DIRECT_MESSAGES, "", listOf("!notes:x", "!bob:x")),
                    RoomSection("favourites", SectionKind.FAVOURITES, "", listOf("!fav:x")),
                    RoomSection("category:7", SectionKind.CATEGORY, "Field work", listOf("!field:x", "!radar:x")),
                    RoomSection("rooms", SectionKind.ROOMS, "", listOf("!room:x")),
                    RoomSection("low_priority", SectionKind.LOW_PRIORITY, "", emptyList()),
                ) + extra,
            ),
        ),
        notesAtTop = notesAtTop,
        notesRoomId = "!notes:x",
    )

    @Test
    fun sectionsInTheAccountsOrderNamedLeavingOutEmptyOnes() {
        val list = spaceList(state(), space, null, RoomSort.DEFAULT)
        assertEquals(listOf("Direct messages", "Favourites", "Field work", "Rooms"), list.sections.map { it.title })
        // The notes on top, out of Direct messages.
        assertEquals("!notes:x", list.notes?.id)
        assertEquals(listOf("!bob:x"), list.sections[0].rooms.map { it.id })
    }

    @Test
    fun withoutNotesAtTopTheNotesAreAnotherDirectChat() {
        val list = spaceList(state(notesAtTop = false), space, null, RoomSort.DEFAULT)
        assertNull(list.notes)
        assertEquals(listOf("!notes:x", "!bob:x"), list.sections[0].rooms.map { it.id })
        val (notes, chats) = chatList(state(notesAtTop = false))
        assertNull(notes)
        assertEquals(listOf("!notes:x", "!bob:x"), chats.map { it.id })
    }

    @Test
    fun chatsHaveTheNotesOnTopAndNotBelow() {
        val (notes, chats) = chatList(state())
        assertEquals("!notes:x", notes?.id)
        assertEquals(listOf("!bob:x"), chats.map { it.id })
    }

    @Test
    fun foldedSectionsKeepTheirHeadingWithTheUnreadCount() {
        val list = spaceList(state(), space, SectionPrefs(folded = setOf("category:7")), RoomSort.DEFAULT)
        val field = list.sections.single { it.key == "category:7" }
        assertTrue(field.folded)
        assertEquals(4L, field.unread)
        assertTrue(field.mentions)
    }

    @Test
    fun hiddenSectionsAndThePhonesOrder() {
        val prefs = SectionPrefs(order = listOf("rooms", "category:7", "favourites"), hidden = setOf("favourites"))
        val list = spaceList(state(), space, prefs, RoomSort.DEFAULT)
        // Sections the order lacks take their place in the account's.
        assertEquals(listOf("direct_messages", "rooms", "category:7"), list.sections.map { it.key })
        val choices = sectionChoices(state(), space, prefs)
        // Low priority, new to the phone, after Rooms as in the account's order.
        assertEquals(listOf("invites", "direct_messages", "rooms", "low_priority", "category:7", "favourites"), choices.map { it.key })
        assertEquals(listOf(true, true, true, true, true, false), choices.map { it.shown })
    }

    @Test
    fun aNewSectionTakesItsPlaceInTheAccountsOrder() {
        assertEquals(listOf("c", "a", "b"), orderedKeys(listOf("a", "b", "c"), listOf("c", "a")))
        assertEquals(listOf("new", "c", "a"), orderedKeys(listOf("new", "a", "c"), listOf("c", "a")))
        assertEquals(listOf("b", "a"), orderedKeys(listOf("a", "b"), listOf("b", "gone", "a")))
        // A new category on the desktop shows, unfolded, in the phone's list.
        val saved = SectionPrefs(order = listOf("rooms", "category:7"), folded = setOf("rooms"))
        val withNew = state(extra = listOf(RoomSection("category:9", SectionKind.CATEGORY, "Teaching", listOf("!room:x"))))
        val shown = spaceList(withNew, space, saved, RoomSort.DEFAULT).sections
        assertEquals(listOf("direct_messages", "favourites", "rooms", "category:9", "category:7"), shown.map { it.key })
        assertFalse(shown.single { it.key == "category:9" }.folded)
        assertTrue(shown.single { it.key == "rooms" }.folded)
    }

    @Test
    fun foldingShowingAndOrderAreSavedPerSpace() = runTest {
        val store = FakeSectionStore()
        val layouts = SectionLayouts(store, scope())
        val keys = listOf("invites", "direct_messages", "rooms")
        layouts.toggleFold("!s:x", "rooms")
        layouts.setShown("!s:x", "invites", false)
        layouts.move("!s:x", keys, 2, 0)
        assertEquals(SectionPrefs(order = listOf("rooms", "invites", "direct_messages"), hidden = setOf("invites"), folded = setOf("rooms")), store.saved!!["!s:x"])
        layouts.toggleFold("!s:x", "rooms")
        layouts.hideAll("!s:x", keys)
        assertEquals(keys.toSet(), layouts.prefs.value["!s:x"]!!.hidden)
        layouts.showAll("!s:x")
        assertTrue(layouts.prefs.value["!s:x"]!!.hidden.isEmpty())
        assertTrue(layouts.prefs.value["!s:x"]!!.folded.isEmpty())
        // Other spaces keep theirs; the next launch reads them.
        assertNull(layouts.prefs.value["!other:x"])
        assertEquals(store.saved, SectionLayouts(store, scope()).prefs.value)
    }

    @Test
    fun theSpacesOwnRoomsWithoutSectionsAndOtherRooms() {
        val plain = RoomListState(
            rooms = listOf(summary("!inv:x", invited = true), summary("!a:x"), summary("!d:x", RoomKind.DIRECT, directUserId = "@d:x")),
            loaded = true,
            roomsOutsideSpaces = listOf("!inv:x", "!a:x"),
        )
        val other = spaceList(plain, SpaceEntry.OtherRooms, null, RoomSort.DEFAULT)
        assertNull(other.notes)
        assertEquals(listOf("Invites", "Rooms"), other.sections.map { it.title })
    }

    @Test
    fun theSettingsSwitchAsksTheAccount() = runTest {
        val session = FakeSession()
        val account = testAccount(FakeMtrx(restored = session), scope())
        account.start()
        account.setNotesAtTop(false)
        assertEquals(listOf("notes at top false"), session.roomCalls)
    }
}
