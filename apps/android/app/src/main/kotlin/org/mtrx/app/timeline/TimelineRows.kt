package org.mtrx.app.timeline

import java.time.LocalDate
import java.time.ZoneId
import uniffi.mtrx.EventContent
import uniffi.mtrx.MediaKind
import uniffi.mtrx.MembershipChange
import uniffi.mtrx.Message
import uniffi.mtrx.MessageKind
import uniffi.mtrx.QuoteState
import uniffi.mtrx.Reaction
import uniffi.mtrx.Reader
import uniffi.mtrx.ReplyQuote
import uniffi.mtrx.SendState
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

/**
 * A message, or a deleted or undecryptable one; [showSender] starts a group.
 * Actions on it take [id] (`TimelineItem.id`).
 */
data class EventRow(override val id: String, val event: TimelineEvent, val showSender: Boolean) : TimelineRow {
    /** The message; null if deleted, undecryptable or not a message. */
    val message: Message? get() = (event.content as? EventContent.Message)?.message

    val edited: Boolean get() = message?.edited == true

    /** In the order first given. */
    val reactions: List<Reaction> get() = message?.reactions.orEmpty()

    /** The message this one answers. */
    val quote: Quote? get() = message?.replyTo?.let(::quote)

    /** Who read up to here. */
    val readers: Readers get() = Readers(event.readBy)

    /** The text to copy or select: a file's caption, not its name; null without one. */
    val text: String? get() = message?.let(::messageText)

    /** What holding the message offers, in the menu's order. */
    val actions: Set<MessageAction> get() = messageActions(this)

    /** Swiping it left replies, as Reply in its menu: a sent message. */
    val canReply: Boolean get() = MessageAction.REPLY in actions
}

/** What holding a message offers. */
enum class MessageAction(val label: String) {
    REACT("React"),
    REPLY("Reply"),
    COPY("Copy"),
    SELECT_TEXT("Select text"),
    EDIT("Edit"),
    DELETE("Delete"),
    RETRY("Retry"),
    DISCARD("Discard"),
}

/**
 * Only what applies: React and Reply once the message is sent (has an event
 * ID), Copy and Select text with a text, Edit when the library allows it,
 * Delete for our own sent ones, Retry and Discard after a failed send.
 */
fun messageActions(row: EventRow): Set<MessageAction> {
    val event = row.event
    val sent = event.eventId != null
    return buildSet {
        if (row.message != null && sent) {
            add(MessageAction.REACT)
            add(MessageAction.REPLY)
        }
        if (row.text != null) {
            add(MessageAction.COPY)
            add(MessageAction.SELECT_TEXT)
        }
        if (event.editable) add(MessageAction.EDIT)
        if (event.own && sent && event.content !is EventContent.Redacted) add(MessageAction.DELETE)
        if (event.sendState is SendState.Failed) {
            add(MessageAction.RETRY)
            add(MessageAction.DISCARD)
        }
    }
}

/** A message's text; an image's or file's body is its name unless it has a caption. */
fun messageText(message: Message): String? {
    val media = message.media
    return message.body.takeIf { it.isNotBlank() && (media == null || it != media.filename) }
}

/** The emoji offered first for a reaction. */
val COMMON_REACTIONS = listOf("👍", "❤️", "😂", "😮", "😢", "🎉")

/** The quote of the message a reply answers, above the reply's text. */
data class Quote(
    /** The original's event ID: go to the row with it. */
    val eventId: String,
    /** Null until loaded. */
    val sender: String?,
    val text: String,
    val state: QuoteState,
) {
    /** A loaded quote goes to the original. */
    val tappable: Boolean get() = state == QuoteState.LOADED
}

fun quote(reply: ReplyQuote): Quote = Quote(
    eventId = reply.eventId,
    sender = reply.senderName ?: reply.senderId?.let(::localpart),
    text = when (reply.state) {
        QuoteState.LOADING -> "Loading…"
        QuoteState.FAILED -> "Could not load the message"
        QuoteState.LOADED -> listOfNotNull(reply.kind?.let(::kindLabel), reply.body).joinToString(" · ")
    },
    state = reply.state,
)

/** "Image", "Video", "Audio" or "File" for a message of that kind. */
fun kindLabel(kind: MessageKind): String? = when (kind) {
    MessageKind.IMAGE -> "Image"
    MessageKind.VIDEO -> "Video"
    MessageKind.AUDIO -> "Audio"
    MessageKind.FILE -> "File"
    else -> null
}

private fun mediaLabel(kind: MediaKind): String = when (kind) {
    MediaKind.IMAGE -> "Image"
    MediaKind.VIDEO -> "Video"
    MediaKind.AUDIO -> "Audio"
    MediaKind.FILE -> "File"
}

/** The first line of a message, as the composer shows what it answers. */
fun preview(row: EventRow): String {
    val message = row.message ?: return ""
    val media = message.media
    val text = messageText(message)?.lineSequence()?.firstOrNull()
    return when {
        media != null -> listOfNotNull(mediaLabel(media.kind), text ?: media.filename).joinToString(" · ")
        else -> text.orEmpty()
    }
}

/** Who read up to a message: the first [MAX_SHOWN] shown, then "+[more]". */
data class Readers(val all: List<Reader>) {
    val shown: List<Reader> get() = all.take(MAX_SHOWN)
    val more: Int get() = (all.size - MAX_SHOWN).coerceAtLeast(0)

    companion object {
        const val MAX_SHOWN = 3
    }
}

/** A reader's or reactor's name, or the localpart of their ID until it is known. */
fun personName(reader: Reader): String = reader.name ?: localpart(reader.userId)

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
