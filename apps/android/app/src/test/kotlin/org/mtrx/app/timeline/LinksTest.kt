package org.mtrx.app.timeline

import org.junit.Assert.assertEquals
import org.junit.Test

class LinksTest {
    private fun urls(text: String) = findLinks(text).map { it.url }

    @Test
    fun webAndMatrixAddressesWithTheirPlaces() {
        val text = "Plots: https://drive.example.org/ice?run=2 and the room https://matrix.to/#/#ice:example.org"
        val links = findLinks(text)
        assertEquals(listOf("https://drive.example.org/ice?run=2", "https://matrix.to/#/#ice:example.org"), links.map { it.url })
        assertEquals("https://drive.example.org/ice?run=2", text.substring(links[0].start, links[0].end))
        assertEquals(text.length, links[1].end)
    }

    @Test
    fun wwwOpensOverHttps() {
        val links = findLinks("see www.awi.de")
        assertEquals(listOf(TextLink(4, 14, "https://www.awi.de")), links)
    }

    @Test
    fun punctuationAfterAnAddressIsNotPartOfIt() {
        assertEquals(listOf("https://example.org/a"), urls("Look at https://example.org/a."))
        assertEquals(listOf("https://example.org/a"), urls("(https://example.org/a)"))
        assertEquals(listOf("https://en.wikipedia.org/wiki/Ice_(disambiguation)"), urls("https://en.wikipedia.org/wiki/Ice_(disambiguation), yes"))
        assertEquals(listOf("http://example.org"), urls("\"http://example.org\"!"))
    }

    @Test
    fun noAddressNoLink() {
        assertEquals(emptyList<String>(), urls("no links here, nor in example.org or https:// or www."))
        assertEquals(emptyList<String>(), urls("ftp://example.org"))
        assertEquals(emptyList<String>(), urls("nothttps://example.org"))
    }
}
