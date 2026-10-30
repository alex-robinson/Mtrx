package org.mtrx.app.timeline

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import java.time.LocalDate
import org.mtrx.app.model.Images
import org.mtrx.app.rooms.RoomGroup
import org.mtrx.app.ui.RoomGroupsList

/**
 * A direct chat's panel of the rooms shared with its person ([name]), under
 * the header over the timeline and the composer: by space as on the Rooms
 * tab ([groups]; null while loading). A room opens with [onOpenRoom], a
 * space's heading with [onOpenSpace] (their IDs).
 */
@Composable
fun SharedRoomsPanel(
    name: String,
    groups: List<RoomGroup>?,
    images: Images,
    today: LocalDate,
    onOpenRoom: (String) -> Unit,
    onOpenSpace: (String) -> Unit,
) {
    Surface(Modifier.fillMaxSize(), tonalElevation = 1.dp) {
        RoomGroupsList(
            groups.orEmpty(),
            loaded = groups != null,
            images,
            today,
            onOpenRoom = { onOpenRoom(it.id) },
            onOpenSpace = onOpenSpace,
            empty = "No rooms shared with $name",
        )
    }
}
