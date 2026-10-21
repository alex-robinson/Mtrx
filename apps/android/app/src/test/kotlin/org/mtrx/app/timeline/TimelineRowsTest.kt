package org.mtrx.app.timeline

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mtrx.app.stateEvent
import org.mtrx.app.textEvent
import uniffi.mtrx.EventContent
import uniffi.mtrx.MembershipChange
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
}
