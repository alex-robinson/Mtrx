package org.mtrx.app.model

import java.io.File
import javax.crypto.KeyGenerator
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.mtrx.SecretException

class FileSecretBackendTest {
    @get:Rule
    val folder = TemporaryFolder()

    // The Keystore's AES-GCM, with a key in memory.
    private val sealer = AesGcmSealer {
        val generator = KeyGenerator.getInstance("AES")
        generator.init(256)
        generator.generateKey()
    }

    private fun backend(dir: File = File(folder.root, "secrets")) = FileSecretBackend(dir, sealer)

    @Test
    fun savesLoadsAndDeletes() {
        val secrets = backend()
        assertNull(secrets.load("profiles/default"))
        secrets.save("profiles/default", "{\"token\":\"é\"}")
        assertEquals("{\"token\":\"é\"}", secrets.load("profiles/default"))
        secrets.save("profiles/default", "new")
        assertEquals("new", secrets.load("profiles/default"))
        secrets.delete("profiles/default")
        assertNull(secrets.load("profiles/default"))
        // Deleting a missing secret succeeds.
        secrets.delete("profiles/default")
    }

    @Test
    fun valuesAreEncryptedAndBoundToTheirKey() {
        val dir = File(folder.root, "secrets")
        val secrets = backend(dir)
        secrets.save("a", "secret value")
        val fileA = dir.listFiles()!!.single()
        secrets.save("b", "other")
        val fileB = dir.listFiles()!!.single { it != fileA }
        assertFalse(String(fileA.readBytes(), Charsets.ISO_8859_1).contains("secret value"))

        // A's file in B's place does not decrypt as B.
        fileB.writeBytes(fileA.readBytes())
        assertThrows(SecretException.Failed::class.java) { secrets.load("b") }
        assertEquals("secret value", secrets.load("a"))
    }

    @Test
    fun corruptFilesFail() {
        val dir = File(folder.root, "secrets")
        val secrets = backend(dir)
        secrets.save("a", "value")
        dir.listFiles()!!.single().writeBytes(byteArrayOf(12, 1, 2, 3))
        assertThrows(SecretException.Failed::class.java) { secrets.load("a") }
    }
}
