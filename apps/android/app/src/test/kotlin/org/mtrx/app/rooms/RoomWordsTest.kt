package org.mtrx.app.rooms

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mtrx.app.FakeSession
import org.mtrx.app.directoryRoom
import org.mtrx.app.member
import org.mtrx.app.roomSettings
import uniffi.mtrx.Access
import uniffi.mtrx.JoinAccess
import uniffi.mtrx.Membership
import uniffi.mtrx.Role
import uniffi.mtrx.RoomLink
import uniffi.mtrx.Topic

// Topics, levels, settings and listed rooms, as the screens word them.
class RoomWordsTest {
    private val call = RoomLink(call = true, title = "Weekly", url = "https://meet.example.org/ice", passcode = "42", name = "Weekly")
    private val drive = RoomLink(call = false, title = null, url = "https://example.org/drive", passcode = null, name = "example.org")

    @Test
    fun theTopicLineIsTheFirstLineElseTheLinksElseAnOffer() {
        assertEquals("Ice group", topicPreview(Topic("\n Ice group \nmore", listOf(call)), canEdit = false))
        assertEquals("Weekly · example.org", topicPreview(Topic("", listOf(call, drive)), canEdit = false))
        assertEquals(ADD_A_TOPIC, topicPreview(NO_TOPIC, canEdit = true))
        assertNull(topicPreview(NO_TOPIC, canEdit = false))
    }

    @Test
    fun theEditorLeavesOutLinksWithoutAUrlAndPasscodesOfLinks() {
        val draft = TopicDraft.of(Topic("Ice", listOf(call, drive))).let {
            it.copy(
                text = " Ice group ",
                links = it.links + LinkDraft(call = false, title = "Empty") + it.links[1].copy(passcode = "nope", title = " "),
            )
        }
        val topic = draft.toTopic()
        assertEquals("Ice group", topic.text)
        assertEquals(listOf("https://meet.example.org/ice", "https://example.org/drive", "https://example.org/drive"), topic.links.map { it.url })
        assertEquals(listOf(true, false, false), topic.links.map { it.call })
        assertEquals("42", topic.links[0].passcode)
        assertNull(topic.links[2].passcode)
        assertNull(topic.links[2].title)
    }

    @Test
    fun rolesAreOwnerAdminModLevelOrNothing() {
        assertEquals("Owner", roleLabel(member("@a:x", level = null)))
        assertEquals("Admin", roleLabel(member("@a:x", level = 100)))
        assertEquals("Mod", roleLabel(member("@a:x", level = 50)))
        assertEquals("Level 20", roleLabel(member("@a:x", level = 20)))
        assertNull(roleLabel(member("@a:x", level = 0)))
        assertEquals("an admin", aRole(100, 0))
        assertEquals("a moderator", aRole(50, 0))
        assertEquals("a member", aRole(0, 0))
        assertEquals("level 20", aRole(20, 0))
    }

    @Test
    fun theMenuOffersLevelsUpToOursWithTheirsTicked() {
        // We are a moderator: admin is above us.
        val choices = levelChoices(member("@b:x", level = 20, upTo = 50))
        assertEquals(listOf("Level 20", "Moderator 50", "Member 0"), choices.map { it.label })
        assertEquals(listOf(true, false, false), choices.map { it.current })
        assertEquals(listOf(false, true, true), choices.map { it.enabled })
        assertTrue(levelChoices(member("@b:x", level = 0, upTo = null)).isEmpty())
    }

    @Test
    fun askingFirstWhereItCannotBeUndone() {
        val bob = member("@b:x", level = 0, upTo = 100)
        assertNull(levelConfirmation(bob, "Bob", 50, own = false, alone = false))
        val admin = levelConfirmation(bob, "Bob", 100, own = false, alone = false)!!
        assertEquals("Make Bob an admin?", admin.title)
        assertTrue(admin.message.contains("an admin can't demote another admin"))
        // A moderator making someone a moderator: their own level.
        val same = levelConfirmation(member("@b:x", level = 0, upTo = 50), "Bob", 50, own = false, alone = false)!!
        assertTrue(same.message.startsWith("They'll have the same level as you."))
        // Stepping down, the last one at our level.
        val down = levelConfirmation(member(FakeSession.USER_ID, level = 100, upTo = 99), "Me", 50, own = true, alone = true)!!
        assertEquals("Step down to moderator?", down.title)
        assertTrue(down.message.endsWith("nobody will be able to do all you can now."))
    }

    @Test
    fun aloneWhenNobodyElseHasOurLevel() {
        val me = member(FakeSession.USER_ID, level = 100)
        assertTrue(aloneAtOurLevel(listOf(me, member("@b:x", level = 50)), FakeSession.USER_ID))
        assertFalse(aloneAtOurLevel(listOf(me, member("@b:x", level = 100)), FakeSession.USER_ID))
        assertTrue(aloneAtOurLevel(listOf(me, member("@b:x", level = 100, invited = true)), FakeSession.USER_ID))
    }

    @Test
    fun settingsWords() {
        val settings = roomSettings(access = Access.PUBLIC, encrypted = true)
        assertEquals("Room · Public · 4 members · encrypted · room version 12", factsLine(false, settings, 4))
        assertEquals("Space · Private · 1 member · room version 12", factsLine(true, roomSettings(), 1))
        assertEquals(listOf(Access.INVITE, Access.KNOCK, Access.PUBLIC), accessOptions(inSpace = false, Access.INVITE).map { it.value })
        assertEquals(4, accessOptions(inSpace = true, Access.INVITE).size)
        assertTrue(showsListed(settings))
        assertFalse(showsListed(roomSettings(listed = null)))
        assertEquals("ice", addressName("#ice:example.org"))
        assertEquals("example.org", serverOf(FakeSession.USER_ID))
    }

    @Test
    fun listedRoomsOfferWhatTheyAllow() {
        assertEquals(RowAction.JOIN, rowAction(directoryRoom("!a:x")))
        assertEquals(RowAction.ASK, rowAction(directoryRoom("!a:x", access = JoinAccess.KNOCK)))
        assertEquals(RowAction.INVITE_ONLY, rowAction(directoryRoom("!a:x", access = JoinAccess.INVITE)))
        assertEquals(RowAction.JOIN, rowAction(directoryRoom("!a:x", access = JoinAccess.INVITE, membership = Membership.INVITED)))
        assertEquals(RowAction.ASKED, rowAction(directoryRoom("!a:x", access = JoinAccess.KNOCK, membership = Membership.ASKED)))
        assertEquals(RowAction.OPEN, rowAction(directoryRoom("!a:x", membership = Membership.JOINED)))
        assertEquals(RowAction.OPEN, rowAction(directoryRoom("!a:x"), joined = true))
    }

    @Test
    fun listedRowsSayTheirSizeTopicAndRole() {
        assertEquals("3 members · Sea ice", rowDetails(directoryRoom("!a:x", topic = "\nSea ice\nmore")))
        assertEquals("Space · 4 rooms", rowDetails(directoryRoom("!a:x", space = true)))
        assertEquals("Admin", roleTag(directoryRoom("!a:x", role = Role.ADMIN)))
        assertNull(roleTag(directoryRoom("!a:x", role = Role.MEMBER)))
        val rooms = listOf(directoryRoom("!ice:x"), directoryRoom("!sea:x", topic = "About ICE"), directoryRoom("!lab:x"))
        assertEquals(listOf("!ice:x", "!sea:x"), filtered(rooms, " ice").map { it.id })
    }

    @Test
    fun userIdsAreAtNameColonServer() {
        assertEquals("@b:x", userIdOf(" @b:x "))
        assertNull(userIdOf("bob"))
        assertNull(userIdOf("@b x:y"))
    }
}
