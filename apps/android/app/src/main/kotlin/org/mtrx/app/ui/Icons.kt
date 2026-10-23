package org.mtrx.app.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.unit.dp

/** Material icons that the core set lacks, from their 24 dp paths. */
object MtrxIcons {
    /** Four squares: spaces are squares, as on the desktop. */
    val Spaces: ImageVector = icon("Spaces", "M3,3h8v8H3V3zM13,3h8v8h-8V3zM3,13h8v8H3v-8zM13,13h8v8h-8v-8z")

    val Reply: ImageVector = icon(
        "Reply",
        "M10,9V5l-7,7 7,7v-4.1c5,0 8.5,1.6 11,5.1 -1,-5 -4,-10 -11,-11z",
        autoMirror = true,
    )

    val Sort: ImageVector = icon("Sort", "M3,18h6v-2H3v2zM3,6v2h18V6H3zM3,13h12v-2H3v2z")

    val Copy: ImageVector = icon(
        "Copy",
        "M16,1H4c-1.1,0 -2,0.9 -2,2v14h2V3h12V1zM19,5H8c-1.1,0 -2,0.9 -2,2v14c0,1.1 0.9,2 2,2h11c1.1,0 2,-0.9 2,-2V7c0,-1.1 -0.9,-2 -2,-2zM19,21H8V7h11v14z",
    )
}

private fun icon(name: String, path: String, autoMirror: Boolean = false): ImageVector = ImageVector.Builder(
    name = name,
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
    autoMirror = autoMirror,
).addPath(pathData = addPathNodes(path), fill = SolidColor(Color.Black)).build()
