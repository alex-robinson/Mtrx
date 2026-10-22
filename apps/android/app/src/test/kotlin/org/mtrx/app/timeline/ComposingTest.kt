package org.mtrx.app.timeline

import org.junit.Assert.assertEquals
import org.junit.Test

class ComposingTest {
    @Test
    fun typingLines() {
        assertEquals("", typingText(emptyList()))
        assertEquals("Anna is typing…", typingText(listOf("Anna")))
        assertEquals("Anna and Ben are typing…", typingText(listOf("Anna", "Ben")))
        assertEquals("Anna, Ben and Carl are typing…", typingText(listOf("Anna", "Ben", "Carl")))
        assertEquals("Anna, Ben and 2 others are typing…", typingText(listOf("Anna", "Ben", "Carl", "Dora")))
    }
}
