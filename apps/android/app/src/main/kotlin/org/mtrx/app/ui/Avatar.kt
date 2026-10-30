package org.mtrx.app.ui

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import org.mtrx.app.model.Images

/**
 * The picture at [mxcUri], or the first letter of [name] on a colour of
 * [seed] (a user or room ID) without one, or until it loads.
 */
@Composable
fun Avatar(
    name: String,
    mxcUri: String?,
    seed: String,
    images: Images,
    size: Dp,
    modifier: Modifier = Modifier,
    shape: Shape = CircleShape,
) {
    val sizePx = with(LocalDensity.current) { size.roundToPx() }
    val bitmap: Bitmap? by produceState(mxcUri?.let { images.cachedAvatar(it, sizePx) }, mxcUri, sizePx) {
        value = mxcUri?.let { images.cachedAvatar(it, sizePx) ?: images.avatar(it, sizePx) }
    }
    Box(
        modifier
            .size(size)
            .clip(shape)
            .background(identityColor(seed)),
        contentAlignment = Alignment.Center,
    ) {
        val loaded = bitmap
        if (loaded != null) {
            val image = remember(loaded) { loaded.asImageBitmap() }
            Image(image, contentDescription = null, modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        } else {
            // The picture's letter: sized to it, whatever the text size.
            val letter = with(LocalDensity.current) { (size * 0.42f).toSp() }
            Text(
                initial(name),
                color = Color.White,
                style = MaterialTheme.typography.titleMedium.copy(fontSize = letter, lineHeight = letter * 1.2f),
            )
        }
    }
}

/** The first letter of a name, skipping a leading `@`, `#` or `!`. */
fun initial(name: String): String {
    val trimmed = name.trimStart('@', '#', '!')
    if (trimmed.isEmpty()) return "?"
    return String(Character.toChars(trimmed.codePointAt(0))).uppercase()
}
