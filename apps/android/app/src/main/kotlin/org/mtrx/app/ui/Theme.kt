package org.mtrx.app.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

// The logo's colours (crates/mtrx-app/assets/logo.svg).
private val Indigo = Color(0xFF5B6CF0)
private val DeepIndigo = Color(0xFF4A58D6)
private val Teal = Color(0xFF2BB0C9)

private val Light = lightColorScheme(
    primary = DeepIndigo,
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

@Composable
fun MtrxTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (isSystemInDarkTheme()) Dark else Light, content = content)
}

/** A colour per user or room, for avatars without a picture and sender names. */
fun identityColor(seed: String): Color {
    val palette = listOf(Indigo, Teal, Color(0xFFE07A5F), Color(0xFF81B29A), Color(0xFFB5838D), Color(0xFFF2A541), Color(0xFF6D597A), Color(0xFF3D85C6))
    return palette[Math.floorMod(seed.hashCode(), palette.size)]
}
