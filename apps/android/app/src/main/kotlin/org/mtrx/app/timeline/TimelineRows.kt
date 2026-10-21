package org.mtrx.app.timeline

import java.time.LocalDate
import java.time.ZoneId
import uniffi.mtrx.EventContent
import uniffi.mtrx.MembershipChange
import uniffi.mtrx.TimelineEvent
import uniffi.mtrx.TimelineItem
import uniffi.mtrx.TimelineItemContent

/** A row of the timeline screen; [id] is stable (list identity). */
sealed interface TimelineRow {
    val id: String
}

/** The room's beginning. */
data class StartRow(override val id: String) : TimelineRow

data class DayRow(override val id: String, val date: LocalDate) : TimelineRow

/** Below are unread messages. */
data class ReadMarkerRow(override val id: String) : TimelineRow

/** A message, or a deleted or undecryptable one; [showSender] starts a group. */
data class EventRow(override val id: String, val event: TimelineEvent, val showSender: Boolean) : TimelineRow

/** Consecutive state events (joins, renames, …) in one line. */
data class StateRow(override val id: String, val text: String) : TimelineRow

/** The library's items (oldest first) as rows, oldest first. */
fun timelineRows(items: List<TimelineItem>, zone: ZoneId = ZoneId.systemDefault()): List<TimelineRow> {
    val rows = ArrayList<TimelineRow>(items.size)
    val run = mutableListOf<Pair<String, TimelineEvent>>()
    var previousSender: String? = null

    fun flushRun() {
        if (run.isEmpty()) return
        val text = stateSummary(run.map { it.second })
        if (text.isNotEmpty()) rows += StateRow(run.first().first, text)
        run.clear()
    }

    for (item in items) {
        when (val content = item.content) {
            is TimelineItemContent.Event -> {
                val event = content.event
                if (isState(event.content)) {
                    run += item.id to event
                    previousSender = null
                } else {
                    flushRun()
                    rows += EventRow(item.id, event, showSender = !event.own && event.senderId != previousSender)
                    previousSender = event.senderId
                }
            }
            is TimelineItemContent.DayDivider -> {
                flushRun()
                rows += DayRow(item.id, content.date.atZone(zone).toLocalDate())
                previousSender = null
            }
            is TimelineItemContent.ReadMarker -> {
                flushRun()
                rows += ReadMarkerRow(item.id)
            }
            is TimelineItemContent.Start -> {
                flushRun()
                rows += StartRow(item.id)
                previousSender = null
            }
        }
    }
    flushRun()
    return rows
}

/** The newest message's row, to mark as read. */
fun latestEventId(rows: List<TimelineRow>): String? = rows.lastOrNull { it is EventRow }?.id

/** The sender's display name, or the localpart of their ID. */
fun displayName(event: TimelineEvent): String = event.senderName ?: localpart(event.senderId)

/** `alice` of `@alice:example.org`. */
fun localpart(userId: String): String = userId.removePrefix("@").substringBefore(':')

private fun isState(content: EventContent): Boolean =
    content is EventContent.Membership || content is EventContent.DisplayNameChange || content is EventContent.OtherState

/**
 * One line for a run of state events, as on the desktop: "alice created and
 * configured the room · alice invited bob · bob joined".
 */
fun stateSummary(events: List<TimelineEvent>): String {
    val creators = events.filter {
        val content = it.content
        content is EventContent.OtherState && content.eventType == "m.room.create"
    }.map { it.senderId }.toSet()
    val parts = mutableListOf<String>()
    val joined = mutableListOf<String>()
    val left = mutableListOf<String>()
    for (event in events) {
        val name = displayName(event)
        val part: String? = when (val content = event.content) {
            is EventContent.OtherState -> when {
                content.eventType == "m.room.create" -> "$name created and configured the room"
                // Initial settings belong to "created and configured".
                event.senderId in creators -> null
                content.eventType == "m.room.name" -> "$name renamed the room"
                content.eventType == "m.room.topic" -> "$name changed the topic"
                content.eventType == "m.room.avatar" -> "$name changed the room picture"
                content.eventType == "m.room.pinned_events" -> "$name changed the pinned messages"
                else -> "$name changed the room settings"
            }
            is EventContent.Membership -> {
                val target = content.name ?: localpart(content.userId)
                when (content.change) {
                    MembershipChange.JOINED, MembershipChange.INVITATION_ACCEPTED -> {
                        if (content.userId !in creators && target !in joined) joined += target
                        null
                    }
                    MembershipChange.LEFT -> {
                        left += target
                        null
                    }
                    MembershipChange.INVITED -> "$name invited $target"
                    MembershipChange.INVITATION_REJECTED -> "$target declined the invite"
                    MembershipChange.INVITATION_REVOKED -> "$name withdrew the invite for $target"
                    MembershipChange.KICKED -> "$name removed $target"
                    MembershipChange.BANNED -> "$name banned $target"
                    MembershipChange.UNBANNED -> "$name unbanned $target"
                    MembershipChange.KNOCKED -> "$target asked to join"
                    MembershipChange.OTHER -> null
                }
            }
            is EventContent.DisplayNameChange -> {
                val user = localpart(content.userId)
                val newName = content.newName
                if (newName != null) "$user is now $newName" else "$user removed their display name"
            }
            else -> null
        }
        if (part != null && parts.lastOrNull() != part) parts += part
    }
    if (joined.isNotEmpty()) parts += "${joined.joinToString(", ")} joined"
    if (left.isNotEmpty()) parts += "${left.joinToString(", ")} left"
    return parts.joinToString(" · ")
}
