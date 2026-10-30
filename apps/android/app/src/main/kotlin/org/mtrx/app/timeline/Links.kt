package org.mtrx.app.timeline

/** A web address in a message's text: its characters [start] until [end], opening [url]. */
data class TextLink(val start: Int, val end: Int, val url: String)

/**
 * The web addresses in [text]: those starting with http:// or https://
 * (matrix.to links too), and with www. (opened over https). Punctuation
 * after one ends it, as do brackets it does not open.
 */
fun findLinks(text: String): List<TextLink> = CANDIDATE.findAll(text).mapNotNull { match ->
    val address = trimEnd(match.value)
    // A scheme or www. alone is no address.
    if (!ADDRESS.matches(address)) return@mapNotNull null
    val url = if (address.startsWith("www.", ignoreCase = true)) "https://$address" else address
    TextLink(match.range.first, match.range.first + address.length, url)
}.toList()

private val CANDIDATE = Regex("""(?i)\b(?:https?://|www\.)[^\s<>"]+""")
private val ADDRESS = Regex("""(?i)(?:https?://|www\.)[^\s<>"]+""")

/** [address] without the punctuation after it, nor closing brackets it does not open. */
private fun trimEnd(address: String): String {
    var end = address.length
    while (end > 0) {
        val last = address[end - 1]
        val open = OPENING[last]
        val text = address.substring(0, end)
        val unbalanced = open != null && text.count { it == last } > text.count { it == open }
        if (last in TRAILING || unbalanced) end-- else break
    }
    return address.substring(0, end)
}

private const val TRAILING = ".,;:!?'*"
private val OPENING = mapOf(')' to '(', ']' to '[', '}' to '{')
