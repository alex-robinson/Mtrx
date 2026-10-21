package org.mtrx.app.timeline

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.time.LocalDate
import kotlinx.coroutines.launch
import org.mtrx.app.model.Images
import org.mtrx.app.ui.Avatar
import org.mtrx.app.ui.dayLabel
import org.mtrx.app.ui.identityColor
import org.mtrx.app.ui.timeOfDay
import uniffi.mtrx.EventContent
import uniffi.mtrx.Media
import uniffi.mtrx.MediaKind
import uniffi.mtrx.MessageKind
import uniffi.mtrx.SendState
import uniffi.mtrx.TimelineEvent

/** Rows from the top this close load older events. */
private const val LOAD_MARGIN = 5

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TimelineScreen(title: String, viewModel: TimelineViewModel, images: Images, onBack: () -> Unit) {
    val rows by viewModel.rows.collectAsStateWithLifecycle()
    val draft by viewModel.draft.collectAsStateWithLifecycle()
    val error by viewModel.error.collectAsStateWithLifecycle()
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val snackbar = remember { SnackbarHostState() }
    // Newest first: the list is reversed, so it starts at the bottom and
    // stays there as messages arrive.
    val newestFirst = remember(rows) { rows.asReversed() }
    val today = remember(rows) { LocalDate.now() }

    LaunchedEffect(error) {
        val message = error ?: return@LaunchedEffect
        snackbar.showSnackbar(message)
        viewModel.clearError()
    }

    // Older events near the top (the reversed list's end).
    LaunchedEffect(listState) {
        snapshotFlow {
            val info = listState.layoutInfo
            (info.visibleItemsInfo.lastOrNull()?.index ?: -1) >= info.totalItemsCount - LOAD_MARGIN
        }.collect { viewModel.onNearTop(it) }
    }

    // Follow new messages while at the bottom.
    val newestId = newestFirst.firstOrNull()?.id
    LaunchedEffect(newestId) {
        if (listState.firstVisibleItemIndex <= 1) listState.animateScrollToItem(0)
    }

    // Read while the newest message shows and the screen is in front.
    val lifecycleState by LocalLifecycleOwner.current.lifecycle.currentStateFlow.collectAsState()
    val atBottom by remember { derivedStateOf { listState.firstVisibleItemIndex == 0 } }
    LaunchedEffect(atBottom, newestId, lifecycleState) {
        if (atBottom && lifecycleState.isAtLeast(Lifecycle.State.RESUMED)) viewModel.markRead()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
        bottomBar = {
            Composer(
                draft = draft,
                onDraft = viewModel::onDraft,
                onSend = {
                    if (viewModel.send()) scope.launch { listState.scrollToItem(0) }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        LazyColumn(
            Modifier
                .padding(padding)
                .fillMaxSize(),
            state = listState,
            reverseLayout = true,
            contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
        ) {
            items(newestFirst, key = { it.id }, contentType = { it::class }) { row ->
                when (row) {
                    is EventRow -> EventBubble(row, images)
                    is StateRow -> Notice(row.text)
                    is DayRow -> DayDivider(dayLabel(row.date, today))
                    is ReadMarkerRow -> ReadMarker()
                    is StartRow -> Notice("This is the beginning of the room")
                }
            }
        }
    }
}

@Composable
private fun Composer(draft: String, onDraft: (String) -> Unit, onSend: () -> Unit) {
    Surface(tonalElevation = 2.dp) {
        Row(
            Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .imePadding()
                .padding(horizontal = 8.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedTextField(
                value = draft,
                onValueChange = onDraft,
                modifier = Modifier.weight(1f),
                placeholder = { Text("Message") },
                maxLines = 6,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
            )
            IconButton(onClick = onSend, enabled = draft.isNotBlank()) {
                Icon(Icons.AutoMirrored.Filled.Send, contentDescription = "Send")
            }
        }
    }
}

@Composable
private fun EventBubble(row: EventRow, images: Images) {
    val event = row.event
    val own = event.own
    val name = displayName(event)
    Row(
        Modifier
            .fillMaxWidth()
            .padding(
                top = if (row.showSender) 8.dp else 2.dp,
                start = if (own) 48.dp else 0.dp,
                end = if (own) 0.dp else 48.dp,
            ),
        horizontalArrangement = if (own) Arrangement.End else Arrangement.Start,
    ) {
        if (!own) {
            if (row.showSender) {
                Avatar(name, event.senderAvatarUrl, seed = event.senderId, images = images, size = 32.dp)
            } else {
                Spacer(Modifier.width(32.dp))
            }
            Spacer(Modifier.width(8.dp))
        }
        Column(
            Modifier.weight(1f, fill = false),
            horizontalAlignment = if (own) Alignment.End else Alignment.Start,
        ) {
            if (row.showSender) {
                Text(
                    name,
                    style = MaterialTheme.typography.labelMedium,
                    color = identityColor(event.senderId),
                    modifier = Modifier.padding(start = 4.dp, bottom = 2.dp),
                )
            }
            Surface(
                shape = RoundedCornerShape(16.dp),
                color = if (own) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceVariant,
            ) {
                Column(Modifier.padding(horizontal = 12.dp, vertical = 8.dp)) {
                    MessageBody(event, name, images)
                    Text(
                        timeOfDay(event.timestamp),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.align(Alignment.End),
                    )
                }
            }
            SendStateLine(event.sendState)
        }
    }
}

@Composable
private fun MessageBody(event: TimelineEvent, name: String, images: Images) {
    val secondary = MaterialTheme.colorScheme.onSurfaceVariant
    when (val content = event.content) {
        is EventContent.Message -> {
            val message = content.message
            val media = message.media
            val edited = if (message.edited) " (edited)" else ""
            when {
                media != null && (media.kind == MediaKind.IMAGE || media.kind == MediaKind.VIDEO) -> {
                    MediaPreview(media, images)
                    // An image's body is its file name unless it has a caption.
                    if (message.body.isNotBlank() && message.body != media.filename) {
                        Text(message.body + edited, Modifier.padding(top = 6.dp))
                    }
                }
                media != null -> Text("${media.filename}${fileSize(media)}", fontStyle = FontStyle.Italic)
                message.kind == MessageKind.EMOTE -> Text("* $name ${message.body}$edited", fontStyle = FontStyle.Italic)
                message.kind == MessageKind.NOTICE -> Text(message.body + edited, color = secondary)
                else -> Text(message.body + edited)
            }
        }
        EventContent.Redacted -> Text("Message deleted", color = secondary, fontStyle = FontStyle.Italic)
        EventContent.UnableToDecrypt -> Text(
            "Unable to decrypt this message (waiting for keys)",
            color = secondary,
            fontStyle = FontStyle.Italic,
        )
        is EventContent.Other -> Text("sent a ${content.description}", color = secondary)
        // State events are StateRows.
        is EventContent.Membership, is EventContent.DisplayNameChange, is EventContent.OtherState -> {}
    }
}

/** An image's (or video's) preview, laid out at its size before it loads. */
@Composable
private fun MediaPreview(media: Media, images: Images) {
    val width = media.width?.toLong()
    val height = media.height?.toLong()
    val ratio = if (width != null && height != null && width > 0 && height > 0) {
        (width.toFloat() / height.toFloat()).coerceIn(0.5f, 2.5f)
    } else {
        4f / 3f
    }
    val bitmap: Bitmap? by produceState(images.cachedPreview(media), media.source) {
        value = images.cachedPreview(media) ?: images.preview(media)
    }
    Box(
        Modifier
            .width(240.dp)
            .aspectRatio(ratio)
            .clip(RoundedCornerShape(12.dp))
            .background(MaterialTheme.colorScheme.surface),
    ) {
        val loaded = bitmap
        if (loaded != null) {
            val image = remember(loaded) { loaded.asImageBitmap() }
            Image(image, contentDescription = media.filename, modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        }
    }
}

private fun fileSize(media: Media): String {
    val bytes = media.size?.toLong() ?: return ""
    return when {
        bytes >= 1_000_000 -> " (%.1f MB)".format(bytes / 1_000_000.0)
        bytes >= 1_000 -> " (${bytes / 1_000} kB)"
        else -> " ($bytes B)"
    }
}

@Composable
private fun SendStateLine(state: SendState) {
    val (text, error) = when (state) {
        SendState.Sent -> return
        SendState.Sending -> "sending…" to false
        SendState.Waiting -> "waiting for connection…" to false
        is SendState.Failed -> "not sent: ${state.error}" to true
    }
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = if (error) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(horizontal = 4.dp, vertical = 2.dp),
    )
}

@Composable
private fun Notice(text: String) {
    Text(
        text,
        Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        textAlign = TextAlign.Center,
    )
}

@Composable
private fun DayDivider(label: String) {
    Row(Modifier.padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
        HorizontalDivider(Modifier.weight(1f))
        Text(
            label,
            Modifier.padding(horizontal = 12.dp),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        HorizontalDivider(Modifier.weight(1f))
    }
}

@Composable
private fun ReadMarker() {
    Row(Modifier.padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        HorizontalDivider(Modifier.weight(1f), color = MaterialTheme.colorScheme.primary)
        Text(
            "New",
            Modifier.padding(horizontal = 8.dp),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.primary,
        )
    }
}
