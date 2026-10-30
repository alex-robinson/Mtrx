package org.mtrx.app.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

// The logo's colour (crates/mtrx-app/assets/logo.svg).
private val LogoIndigo = Color(0xFF2F3E9E)
private val Indigo = Color(0xFF5B6CF0)
private val Teal = Color(0xFF2BB0C9)

private val Light = lightColorScheme(
    primary = LogoIndigo,
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE0E3FF),
    onPrimaryContainer = Color(0xFF101A6B),
    secondary = Teal,
    onSecondary = Color.White,
)

private val Dark = darkColorScheme(
    primary = Color(0xFFBCC3FF),
    onPrimary = Color(0xFF1B2380),
    primaryContainer = Color(0xFF3540B5),
    onPrimaryContainer = Color(0xFFE0E3FF),
    secondary = Color(0xFF7FD6E8),
    onSecondary = Color(0xFF00363F),
)

/** A message bubble's colours: its background, text, and time and muted text. */
data class BubbleColors(val container: Color, val content: Color, val secondary: Color)

// Our own messages: the logo's indigo, light (#57); a dark blue-grey in the dark.
private val LightOwnBubble = BubbleColors(Color(0xFFC7CFF3), Color.Black, Color(0xFF4A5180))
private val DarkOwnBubble = BubbleColors(Color(0xFF2E3352), Color.White, Color(0xFFB8BFE6))

/** The bubbles of our own messages, as the theme sets them. */
val LocalOwnBubble = staticCompositionLocalOf { LightOwnBubble }

@Composable
fun MtrxTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    MaterialTheme(colorScheme = if (dark) Dark else Light) {
        CompositionLocalProvider(LocalOwnBubble provides if (dark) DarkOwnBubble else LightOwnBubble, content = content)
    }
}

/**
 * A line of each of [styles], one above the other, at the text size chosen
 * in the system's settings: for heights that must grow with it.
 */
@Composable
fun linesHeight(vararg styles: TextStyle): Dp = with(LocalDensity.current) {
    styles.fold(0.dp) { sum, style -> sum + style.lineHeight.toDp() }
}

/** The largest text size, times the default, of the tabs and of a count on an icon. */
const val BAR_TEXT_SCALE = 1.3f

/**
 * [content] with the text size at most [max] times the default: for what
 * cannot grow with it, as the tabs' labels and the counts on icons.
 */
@Composable
fun TextScaleAtMost(max: Float, content: @Composable () -> Unit) {
    val density = LocalDensity.current
    CompositionLocalProvider(LocalDensity provides Density(density.density, minOf(density.fontScale, max)), content = content)
}

/** A colour per user or room, for avatars without a picture and sender names. */
fun identityColor(seed: String): Color {
    val palette = listOf(Indigo, Teal, Color(0xFFE07A5F), Color(0xFF81B29A), Color(0xFFB5838D), Color(0xFFF2A541), Color(0xFF6D597A), Color(0xFF3D85C6))
    return palette[Math.floorMod(seed.hashCode(), palette.size)]
}
