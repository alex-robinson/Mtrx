package org.mtrx.app.model

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.security.MessageDigest
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import uniffi.mtrx.SecretBackend
import uniffi.mtrx.SecretException

/**
 * The library's secrets (access tokens, the store's passphrase), one file
 * each in [dir], encrypted by [sealer]. Files are named by a hash of the
 * secret's key; the key is also bound to the ciphertext, so a file cannot
 * stand in for another. Called on the library's threads.
 */
class FileSecretBackend(private val dir: File, private val sealer: Sealer) : SecretBackend {

    @Synchronized
    override fun load(key: String): String? = guard {
        val file = fileFor(key)
        if (!file.exists()) {
            null
        } else {
            String(sealer.open(file.readBytes(), key.toByteArray()), Charsets.UTF_8)
        }
    }

    @Synchronized
    override fun save(key: String, value: String) {
        guard {
            if (!dir.isDirectory && !dir.mkdirs()) throw IOException("cannot create $dir")
            val file = fileFor(key)
            // Through a temporary file, so a crash never leaves half a secret.
            val partial = File(dir, "${file.name}.part")
            partial.writeBytes(sealer.seal(value.toByteArray(Charsets.UTF_8), key.toByteArray()))
            if (!partial.renameTo(file)) throw IOException("cannot replace ${file.name}")
        }
    }

    @Synchronized
    override fun delete(key: String) {
        guard {
            val file = fileFor(key)
            if (file.exists() && !file.delete()) throw IOException("cannot delete ${file.name}")
        }
    }

    private fun fileFor(key: String): File {
        val hash = MessageDigest.getInstance("SHA-256").digest(key.toByteArray())
        return File(dir, hash.joinToString("") { "%02x".format(it) })
    }

    private inline fun <T> guard(block: () -> T): T = try {
        block()
    } catch (e: GeneralSecurityException) {
        throw SecretException.Failed(e.toString())
    } catch (e: IOException) {
        throw SecretException.Failed(e.toString())
    }
}

/** Encrypts small values; `associated` is authenticated with them. */
interface Sealer {
    fun seal(plain: ByteArray, associated: ByteArray): ByteArray
    fun open(sealed: ByteArray, associated: ByteArray): ByteArray
}

/**
 * AES-GCM with a random IV per value: [IV length][IV][ciphertext and tag].
 * [loadKey] is called on first use.
 */
class AesGcmSealer(loadKey: () -> SecretKey) : Sealer {
    private val key: SecretKey by lazy(loadKey)

    override fun seal(plain: ByteArray, associated: ByteArray): ByteArray {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key)
        cipher.updateAAD(associated)
        val iv = cipher.iv
        return byteArrayOf(iv.size.toByte()) + iv + cipher.doFinal(plain)
    }

    override fun open(sealed: ByteArray, associated: ByteArray): ByteArray {
        if (sealed.isEmpty()) throw GeneralSecurityException("empty secret")
        val ivSize = sealed[0].toInt()
        if (ivSize <= 0 || sealed.size < 1 + ivSize) throw GeneralSecurityException("truncated secret")
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_BITS, sealed, 1, ivSize))
        cipher.updateAAD(associated)
        return cipher.doFinal(sealed, 1 + ivSize, sealed.size - 1 - ivSize)
    }

    companion object {
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val TAG_BITS = 128

        /** With a 256-bit key that never leaves the Android Keystore. */
        fun keystore(alias: String = "mtrx-secrets"): AesGcmSealer = AesGcmSealer {
            val store = KeyStore.getInstance("AndroidKeyStore")
            store.load(null)
            (store.getKey(alias, null) as? SecretKey) ?: run {
                val spec = KeyGenParameterSpec.Builder(
                    alias,
                    KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                )
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setKeySize(256)
                    .build()
                val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
                generator.init(spec)
                generator.generateKey()
            }
        }
    }
}
