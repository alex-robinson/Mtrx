package org.mtrx.app.rooms

import uniffi.mtrx.RoomLink
import uniffi.mtrx.Topic

// A room's topic as the room screen and Room info show it: its text and its
// links (`[call] …` and `[link] …` lines), parsed and composed by the library
// (`parseTopic`, `composeTopic`).

/** No topic. */
val NO_TOPIC = Topic(text = "", links = emptyList())

/**
 * The topic line under a room's name: the text's first line, else the
 * links' names, else "Add a topic" where we may edit it; null for none.
 */
fun topicPreview(topic: Topic, canEdit: Boolean): String? {
    val firstLine = topic.text.lineSequence().map { it.trim() }.firstOrNull { it.isNotEmpty() }
    return when {
        firstLine != null -> firstLine
        topic.links.isNotEmpty() -> topic.links.joinToString(" · ") { it.name }
        canEdit -> ADD_A_TOPIC
        else -> null
    }
}

const val ADD_A_TOPIC = "Add a topic"

/** A link in the topic editor; [passcode] only for a call. */
data class LinkDraft(
    val call: Boolean = false,
    val title: String = "",
    val url: String = "",
    val passcode: String = "",
)

/** The topic editor's text and links, as typed. */
data class TopicDraft(val text: String = "", val links: List<LinkDraft> = emptyList()) {
    /** The topic to save: links without a URL are left out, blank fields are none. */
    fun toTopic(): Topic = Topic(
        text = text.trim(),
        links = links.filter { it.url.isNotBlank() }.map { link ->
            val title = link.title.trim().ifEmpty { null }
            RoomLink(
                call = link.call,
                title = title,
                url = link.url.trim(),
                passcode = link.passcode.trim().ifEmpty { null }?.takeIf { link.call },
                name = title ?: link.url.trim(),
            )
        },
    )

    companion object {
        fun of(topic: Topic) = TopicDraft(
            text = topic.text,
            links = topic.links.map { LinkDraft(it.call, it.title.orEmpty(), it.url, it.passcode.orEmpty()) },
        )
    }
}
