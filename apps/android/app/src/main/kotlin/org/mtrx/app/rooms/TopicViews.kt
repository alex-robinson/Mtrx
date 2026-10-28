package org.mtrx.app.rooms

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Call
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.mtrx.app.ui.MtrxIcons
import uniffi.mtrx.RoomLink
import uniffi.mtrx.Topic

/** The topic's text in full, selectable; nothing without one. */
@Composable
fun TopicText(topic: Topic, modifier: Modifier = Modifier) {
    if (topic.text.isBlank()) return
    SelectionContainer(modifier) {
        Text(topic.text, style = MaterialTheme.typography.bodyMedium)
    }
}

/** The topic's links as buttons: a call icon for calls, a link icon for links; [onOpen] with the URL. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TopicLinks(links: List<RoomLink>, onOpen: (String) -> Unit, modifier: Modifier = Modifier) {
    if (links.isEmpty()) return
    FlowRow(modifier, horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        for (link in links) {
            FilledTonalButton(onClick = { onOpen(link.url) }) {
                Icon(
                    if (link.call) Icons.Filled.Call else MtrxIcons.Link,
                    contentDescription = if (link.call) "Call" else "Link",
                    modifier = Modifier.size(18.dp),
                )
                Spacer(Modifier.width(6.dp))
                Text(link.name, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/**
 * The topic editor: the text, and the links (each a call or a link, with
 * a title, URL and a call's passcode); [onSave] with the topic to save.
 */
@Composable
fun TopicEditor(initial: Topic, onSave: (Topic) -> Unit, onCancel: () -> Unit, modifier: Modifier = Modifier) {
    var draft by remember(initial) { mutableStateOf(TopicDraft.of(initial)) }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            value = draft.text,
            onValueChange = { draft = draft.copy(text = it) },
            modifier = Modifier.fillMaxWidth(),
            label = { Text("Topic") },
            placeholder = { Text("What the room is about") },
            minLines = 2,
            maxLines = 6,
        )
        draft.links.forEachIndexed { index, link ->
            fun change(changed: LinkDraft) {
                draft = draft.copy(links = draft.links.toMutableList().also { it[index] = changed })
            }
            LinkEditor(
                link,
                onChange = ::change,
                onRemove = { draft = draft.copy(links = draft.links.toMutableList().also { it.removeAt(index) }) },
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = { draft = draft.copy(links = draft.links + LinkDraft(call = true)) }) {
                Icon(Icons.Filled.Call, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(6.dp))
                Text("Add call")
            }
            TextButton(onClick = { draft = draft.copy(links = draft.links + LinkDraft(call = false)) }) {
                Icon(MtrxIcons.Link, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(6.dp))
                Text("Add link")
            }
            Spacer(Modifier.weight(1f))
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            TextButton(onClick = onCancel) { Text("Cancel") }
            Spacer(Modifier.width(8.dp))
            FilledTonalButton(onClick = { onSave(draft.toTopic()) }) { Text("Save") }
        }
    }
}

@Composable
private fun LinkEditor(link: LinkDraft, onChange: (LinkDraft) -> Unit, onRemove: () -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(top = 4.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            SingleChoiceSegmentedButtonRow(Modifier.weight(1f)) {
                SegmentedButton(
                    selected = !link.call,
                    onClick = { onChange(link.copy(call = false)) },
                    shape = SegmentedButtonDefaults.itemShape(0, 2),
                ) { Text("Link") }
                SegmentedButton(
                    selected = link.call,
                    onClick = { onChange(link.copy(call = true)) },
                    shape = SegmentedButtonDefaults.itemShape(1, 2),
                ) { Text("Call") }
            }
            IconButton(onClick = onRemove) { Icon(Icons.Filled.Close, contentDescription = "Remove") }
        }
        OutlinedTextField(
            value = link.title,
            onValueChange = { onChange(link.copy(title = it)) },
            modifier = Modifier.fillMaxWidth(),
            label = { Text("Title") },
            singleLine = true,
        )
        OutlinedTextField(
            value = link.url,
            onValueChange = { onChange(link.copy(url = it)) },
            modifier = Modifier.fillMaxWidth(),
            label = { Text("URL") },
            placeholder = { Text("https://…") },
            singleLine = true,
        )
        if (link.call) {
            OutlinedTextField(
                value = link.passcode,
                onValueChange = { onChange(link.copy(passcode = it)) },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Passcode") },
                singleLine = true,
            )
        }
    }
}
