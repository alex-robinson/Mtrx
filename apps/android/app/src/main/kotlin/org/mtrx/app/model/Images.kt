package org.mtrx.app.model

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.LruCache
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.mtrx.Media
import uniffi.mtrx.MtrxException
import uniffi.mtrx.SessionInterface

/**
 * Avatars and message images of a session, decoded and kept in memory (the
 * library caches the downloads). Null when an image cannot be loaded.
 */
class Images(private val session: SessionInterface) {
    // Created on first use, so that tests can construct an Images.
    private val cache by lazy {
        object : LruCache<String, Bitmap>(CACHE_BYTES) {
            override fun sizeOf(key: String, value: Bitmap): Int = value.byteCount
        }
    }

    fun cachedAvatar(mxcUri: String, sizePx: Int): Bitmap? = cache.get(avatarKey(mxcUri, sizePx))

    suspend fun avatar(mxcUri: String, sizePx: Int): Bitmap? =
        load(avatarKey(mxcUri, sizePx), sizePx) { session.avatar(mxcUri, sizePx.toUInt()) }

    fun cachedPreview(media: Media): Bitmap? = cache.get(previewKey(media))

    suspend fun preview(media: Media): Bitmap? =
        load(previewKey(media), PREVIEW_PX) { session.mediaPreview(media) }

    private suspend fun load(key: String, maxPx: Int, fetch: suspend () -> ByteArray): Bitmap? {
        cache.get(key)?.let { return it }
        val bytes = try {
            fetch()
        } catch (e: CancellationException) {
            throw e
        } catch (e: MtrxException) {
            return null
        } catch (e: IllegalStateException) {
            // The session was closed (signed out) while the screen still showed.
            return null
        }
        val bitmap = withContext(Dispatchers.Default) { decode(bytes, maxPx) } ?: return null
        cache.put(key, bitmap)
        return bitmap
    }

    private fun avatarKey(mxcUri: String, sizePx: Int) = "avatar:$sizePx:$mxcUri"

    private fun previewKey(media: Media) = "media:${media.source}"

    private companion object {
        /** Larger previews are decoded at a lower resolution. */
        const val PREVIEW_PX = 1280
        val CACHE_BYTES = (Runtime.getRuntime().maxMemory() / 8).coerceAtMost(Int.MAX_VALUE.toLong()).toInt()

        fun decode(bytes: ByteArray, maxPx: Int): Bitmap? {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
            if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
            var sample = 1
            while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= maxPx) sample *= 2
            val options = BitmapFactory.Options().apply { inSampleSize = sample }
            return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
        }
    }
}
