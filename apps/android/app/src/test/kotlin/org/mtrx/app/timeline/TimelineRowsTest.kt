package org.mtrx.app.timeline

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mtrx.app.eventItem
import org.mtrx.app.messageEvent
import org.mtrx.app.stateEvent
import org.mtrx.app.textEvent
import uniffi.mtrx.EventContent
import uniffi.mtrx.Media
import uniffi.mtrx.MediaKind
import uniffi.mtrx.MembershipChange
import uniffi.mtrx.Message
import uniffi.mtrx.MessageKind
import uniffi.mtrx.QuoteState
import uniffi.mtrx.Reaction
import uniffi.mtrx.Reader
import uniffi.mtrx.ReplyQuote
import uniffi.mtrx.SendState
import uniffi.mtrx.TimelineItem
import uniffi.mtrx.TimelineItemContent

class TimelineRowsTest {
    private val alice = "@alice:example.org"
    private val bob = "@bob:example.org"

    @Test
    fun groupsMessagesBySender() {
        val rows = timelineRows(
            listOf(
                TimelineItem("start", TimelineItemContent.Start),
                TimelineItem("day-0", TimelineItemContent.DayDivider(Instant.EPOCH)),
                textEvent("1", alice, "hi"),
                textEvent("2", alice, "there"),
                textEvent("3", bob, "hello"),
                TimelineItem("read-marker", TimelineItemContent.ReadMarker),
                textEvent("4", bob, "again"),
                textEvent("5", "@me:example.org", "mine", own = true),
                textEvent("6", bob, "back"),
            ),
            ZoneOffset.UTC,
        )
        assertEquals(listOf("start", "day-0", "1", "2", "3", "read-marker", "4", "5", "6"), rows.map { it.id })
        assertEquals(LocalDate.of(1970, 1, 1), (rows[1] as DayRow).date)
        val senders = rows.filterIsInstance<EventRow>().map { it.id to it.showSender }
        // The read marker does not start a group; our own messages show no sender.
        assertEquals(
            listOf("1" to true, "2" to false, "3" to true, "4" to false, "5" to false, "6" to true),
            senders,
        )
        assertEquals("6", latestEventId(rows))
    }

    @Test
    fun stateEventsShareALine() {
        val rows = timelineRows(
            listOf(
                stateEvent("c", alice, EventContent.OtherState("m.room.create")),
                stateEvent("n", alice, EventContent.OtherState("m.room.name")),
                stateEvent("i", alice, EventContent.Membership(bob, "Bob", MembershipChange.INVITED)),
                stateEvent("j", bob, EventContent.Membership(bob, "Bob", MembershipChange.JOINED)),
                textEvent("m", bob, "hi"),
                stateEvent("t", bob, EventContent.OtherState("m.room.topic")),
            ),
        )
        assertEquals(listOf("c", "m", "t"), rows.map { it.id })
        assertEquals(
            "alice created and configured the room · alice invited Bob · Bob joined",
            (rows[0] as StateRow).text,
        )
        assertEquals("bob changed the topic", (rows[2] as StateRow).text)
        assertEquals(true, (rows[1] as EventRow).showSender)
    }

    @Test
    fun localparts() {
        assertEquals("alice", localpart("@alice:example.org"))
        assertEquals("alice", localpart("alice"))
    }

    private val me = "@me:example.org"

    private fun reader(userId: String, name: String? = null) = Reader(userId = userId, name = name, avatarUrl = null)

    private fun text(body: String, edited: Boolean = false, reactions: List<Reaction> = emptyList(), replyTo: ReplyQuote? = null) =
        Message(kind = MessageKind.TEXT, body = body, edited = edited, media = null, reactions = reactions, replyTo = replyTo, rawBody = body)

    private fun row(item: TimelineItem): EventRow = timelineRows(listOf(item)).single() as EventRow

    private fun quoted(state: QuoteState, kind: MessageKind? = null, body: String? = null) = ReplyQuote(
        eventId = "\$original",
        state = state,
        senderId = if (state == QuoteState.LOADED) alice else null,
        senderName = null,
        kind = kind,
        body = body,
    )

    @Test
    fun rowsCarryReactionsQuotesEditsAndReaders() {
        val reactions = listOf(
            Reaction(key = "👍", count = 2uL, byUs = true, senders = listOf(reader(me, "Me"), reader(bob))),
            Reaction(key = "🎉", count = 1uL, byUs = false, senders = listOf(reader(bob, "Bob"))),
        )
        val readers = listOf("a", "b", "c", "d", "e").map { reader("@$it:example.org") }
        val row = row(
            messageEvent(
                "2",
                bob,
                text("yes", edited = true, reactions = reactions, replyTo = quoted(QuoteState.LOADED, MessageKind.TEXT, "shall we?")),
                readBy = readers,
            ),
        )
        assertTrue(row.edited)
        assertEquals(listOf("👍" to true, "🎉" to false), row.reactions.map { it.key to it.byUs })
        assertEquals(Quote("\$original", "alice", "shall we?", QuoteState.LOADED), row.quote)
        assertTrue(row.quote!!.tappable)
        // At most three avatars, then "+2".
        assertEquals(listOf("@a:example.org", "@b:example.org", "@c:example.org"), row.readers.shown.map { it.userId })
        assertEquals(2, row.readers.more)
        assertEquals(0, Readers(readers.take(3)).more)
        // Names may come later: the ID's localpart until then.
        assertEquals("bob", personName(reader(bob)))
        assertEquals("Bob", personName(reader(bob, "Bob")))

        val plain = row(textEvent("3", bob, "plain"))
        assertFalse(plain.edited)
        assertNull(plain.quote)
        assertTrue(plain.reactions.isEmpty())
        assertTrue(plain.readers.all.isEmpty())
    }

    @Test
    fun quotesSayWhatTheOriginalIs() {
        assertEquals(Quote("\$original", null, "Loading…", QuoteState.LOADING), quote(quoted(QuoteState.LOADING)))
        val failed = quote(quoted(QuoteState.FAILED))
        assertEquals("Could not load the message", failed.text)
        assertFalse(failed.tappable)
        assertEquals("Image · ice.png", quote(quoted(QuoteState.LOADED, MessageKind.IMAGE, "ice.png")).text)
        assertEquals("File · data.nc", quote(quoted(QuoteState.LOADED, MessageKind.FILE, "data.nc")).text)
        // A deleted original: no kind, the library's text.
        assertEquals("Message deleted", quote(quoted(QuoteState.LOADED, null, "Message deleted")).text)
        assertEquals("alice", quote(quoted(QuoteState.LOADED)).sender)
    }

    @Test
    fun theMenuOffersWhatApplies() {
        val all = MessageAction.entries
        fun actions(item: TimelineItem) = row(item).actions.toList()

        // Others' message: no edit or delete.
        assertEquals(
            listOf(MessageAction.REACT, MessageAction.REPLY, MessageAction.COPY, MessageAction.SELECT_TEXT),
            actions(textEvent("1", bob, "hi")),
        )
        // Ours, sent, in the menu's order.
        assertEquals(
            all - MessageAction.RETRY - MessageAction.DISCARD,
            actions(messageEvent("2", me, text("mine"), own = true, editable = true)),
        )
        // Not sent yet: no reacting, replying or deleting.
        assertEquals(
            listOf(MessageAction.COPY, MessageAction.SELECT_TEXT, MessageAction.EDIT),
            actions(messageEvent("3", me, text("on its way"), own = true, eventId = null, sendState = SendState.Sending, editable = true)),
        )
        // Refused: retry or discard.
        assertEquals(
            listOf(MessageAction.COPY, MessageAction.SELECT_TEXT, MessageAction.RETRY, MessageAction.DISCARD),
            actions(messageEvent("4", me, text("refused"), own = true, eventId = null, sendState = SendState.Failed("too large"))),
        )
        // Deleted: nothing, ours or not.
        assertEquals(emptyList<MessageAction>(), actions(eventItem("5", me, EventContent.Redacted, own = true)))
        assertEquals(emptyList<MessageAction>(), actions(eventItem("6", bob, EventContent.Redacted)))
        // Undecryptable, ours: only deleting.
        assertEquals(listOf(MessageAction.DELETE), actions(eventItem("7", me, EventContent.UnableToDecrypt, own = true)))
        // An image without a caption: nothing to copy.
        val image = Message(
            kind = MessageKind.IMAGE,
            body = "ice.png",
            edited = false,
            media = Media(MediaKind.IMAGE, "ice.png", null, "image/png", 800u, 600u, "{}"),
        )
        val imageRow = row(messageEvent("8", bob, image))
        assertEquals(listOf(MessageAction.REACT, MessageAction.REPLY), imageRow.actions.toList())
        assertNull(imageRow.text)
        assertEquals("Image · ice.png", preview(imageRow))
    }
}
