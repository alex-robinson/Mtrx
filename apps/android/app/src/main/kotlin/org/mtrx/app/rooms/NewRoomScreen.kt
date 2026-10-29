package org.mtrx.app.rooms

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.mtrx.app.ui.SectionHeading
import org.mtrx.app.ui.SwitchRow

/** What the new room screen does. */
class NewRoomActions(
    val edit: ((NewRoomForm) -> NewRoomForm) -> Unit,
    val addPerson: () -> Boolean,
    val removePerson: (String) -> Unit,
    val setCoOwner: (String, Boolean) -> Unit,
    val create: () -> Unit,
    val clearError: () -> Unit,
)

/**
 * New room or space ([space]), in [parentName]'s space if given: name,
 * topic, Encrypted (rooms), Only moderators can edit, people to invite and
 * for each a Co-owner switch where the server [offersOwners]; Create at the
 * top right.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewRoomContent(
    space: Boolean,
    parentName: String?,
    form: NewRoomForm,
    offersOwners: Boolean,
    creating: Boolean,
    error: String?,
    actions: NewRoomActions,
    onBack: () -> Unit,
) {
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(error) {
        val message = error ?: return@LaunchedEffect
        snackbar.showSnackbar(message)
        actions.clearError()
    }
    val title = when {
        space -> "New space"
        parentName != null -> "New room in $parentName"
        else -> "New room"
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    IconButton(onClick = onBack) { Icon(Icons.Filled.Close, contentDescription = "Cancel") }
                },
                actions = {
                    if (creating) {
                        CircularProgressIndicator(Modifier.padding(end = 16.dp).size(20.dp), strokeWidth = 2.dp)
                    } else {
                        TextButton(onClick = actions.create, enabled = form.canCreate) { Text("Create") }
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        Column(
            Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            OutlinedTextField(
                value = form.name,
                onValueChange = { name -> actions.edit { it.copy(name = name) } },
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp),
                singleLine = true,
                label = { Text(if (space) "Name" else "Room name") },
            )
            OutlinedTextField(
                value = form.topic,
                onValueChange = { topic -> actions.edit { it.copy(topic = topic) } },
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp),
                label = { Text("Topic (optional)") },
                placeholder = { Text("What the room is about") },
                maxLines = 4,
            )
            if (!space) {
                SwitchRow(
                    "Encrypted",
                    checked = form.encrypted,
                    explanation = "Only members can read messages. Cannot be turned off later.",
                    onChange = { on -> actions.edit { it.copy(encrypted = on) } },
                )
            }
            SwitchRow(
                "Only moderators can edit",
                checked = form.moderated,
                explanation = "Otherwise everyone can invite, rename and change the topic.",
                onChange = { on -> actions.edit { it.copy(moderated = on) } },
            )
            SectionHeading("Invite people (optional)")
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                OutlinedTextField(
                    value = form.person,
                    onValueChange = { person -> actions.edit { it.copy(person = person) } },
                    modifier = Modifier.weight(1f),
                    singleLine = true,
                    placeholder = { Text("@name:server") },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { actions.addPerson() }),
                )
                IconButton(onClick = { actions.addPerson() }, enabled = userIdOf(form.person) != null) {
                    Icon(Icons.Filled.Add, contentDescription = "Add")
                }
            }
            for (person in form.invite) {
                Row(
                    Modifier
                        .fillMaxWidth()
                        .padding(start = 16.dp, end = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(person, Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (offersOwners) {
                        Text("Co-owner", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Switch(
                            checked = person in form.coOwners,
                            onCheckedChange = { actions.setCoOwner(person, it) },
                            modifier = Modifier.padding(start = 8.dp),
                        )
                    }
                    IconButton(onClick = { actions.removePerson(person) }) {
                        Icon(Icons.Filled.Close, contentDescription = "Remove")
                    }
                }
            }
            if (offersOwners && form.invite.isNotEmpty()) {
                Text(
                    "Co-owners own the room as you do: nobody can demote them, and they can't step down.",
                    Modifier.padding(horizontal = 16.dp),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            HorizontalDivider(Modifier.padding(top = 8.dp))
        }
    }
}
