package org.mtrx.app.settings

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import org.mtrx.app.model.Distributor
import org.mtrx.app.model.PushRegistration
import org.mtrx.app.model.PushState
import org.mtrx.app.push.notificationsAllowed

/** The "Push notifications" switch: on, and whether it can be changed. */
data class PushSwitch(val checked: Boolean, val usable: Boolean)

/** A line under the switch. */
sealed interface NotificationsLine {
    /** "Via ntfy". */
    data class Via(val label: String) : NotificationsLine

    data object Registering : NotificationsLine

    /** Several distributors and none chosen: a choice. */
    data class Choose(val distributors: List<Distributor>) : NotificationsLine

    /** None installed: a button to ntfy's store page. */
    data object Install : NotificationsLine

    /** Off in Android's settings: a button to them. */
    data object SystemOff : NotificationsLine

    data class Failed(val reason: String) : NotificationsLine
}

/** The switch for [state]: off and fixed while no distributor is installed. */
fun pushSwitch(enabled: Boolean, state: PushState): PushSwitch =
    PushSwitch(checked = enabled && state !is PushState.NoDistributor, usable = state !is PushState.NoDistributor)

/** The lines under the switch, for [state] and whether Android lets notifications show. */
fun notificationsLines(enabled: Boolean, state: PushState, systemAllowed: Boolean): List<NotificationsLine> {
    val line = when (state) {
        PushState.Off -> null
        PushState.NoDistributor -> NotificationsLine.Install
        is PushState.Choose -> NotificationsLine.Choose(state.distributors)
        is PushState.Registering -> NotificationsLine.Registering
        is PushState.Registered -> NotificationsLine.Via(state.distributor.label)
        is PushState.Failed -> NotificationsLine.Failed(state.reason)
    }
    val system = if (enabled && !systemAllowed) NotificationsLine.SystemOff else null
    return listOfNotNull(line, system)
}

fun lineText(line: NotificationsLine): String = when (line) {
    is NotificationsLine.Via -> "Via ${line.label}"
    NotificationsLine.Registering -> "Registering…"
    is NotificationsLine.Choose -> "Several apps can deliver notifications"
    NotificationsLine.Install -> "Install a UnifiedPush app, such as ntfy, to get notifications while Mtrx is closed."
    NotificationsLine.SystemOff -> "Notifications are off in Android's settings"
    is NotificationsLine.Failed -> "Registration failed: ${line.reason}"
}

/** The Settings tab's "Notifications" section. */
@Composable
fun NotificationsSection(push: PushRegistration) {
    val context = LocalContext.current
    val state by push.state.collectAsStateWithLifecycle()
    val enabled by push.enabled.collectAsStateWithLifecycle()
    var allowed by remember { mutableStateOf(notificationsAllowed(context)) }
    // Back from Android's settings or a store: look again.
    LifecycleResumeEffect(push) {
        allowed = notificationsAllowed(context)
        push.refresh()
        onPauseOrDispose {}
    }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        allowed = notificationsAllowed(context)
    }
    val switch = pushSwitch(enabled, state)

    ListItem(
        headlineContent = { Text("Push notifications") },
        trailingContent = {
            Switch(
                checked = switch.checked,
                enabled = switch.usable,
                onCheckedChange = { on ->
                    push.setEnabled(on)
                    if (on && needsPermission(context)) permission.launch(Manifest.permission.POST_NOTIFICATIONS)
                },
            )
        },
    )
    for (line in notificationsLines(enabled, state, allowed)) {
        Column(Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) {
            Text(
                lineText(line),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            when (line) {
                is NotificationsLine.Choose -> line.distributors.forEach { distributor ->
                    ListItem(
                        headlineContent = { Text(distributor.label) },
                        modifier = Modifier.clickable { push.choose(distributor.packageName) },
                    )
                }
                NotificationsLine.Install -> OutlinedButton(
                    onClick = { openNtfyStorePage(context) },
                    modifier = Modifier.padding(top = 8.dp),
                ) { Text("Get ntfy") }
                NotificationsLine.SystemOff -> OutlinedButton(
                    onClick = { openNotificationSettings(context) },
                    modifier = Modifier.padding(top = 8.dp),
                ) { Text("Open settings") }
                else -> Unit
            }
        }
    }
}

/** After a sign-in, ask once for the permission to notify (Android 13+). */
@Composable
fun AskNotificationPermissionOnce(push: PushRegistration) {
    val context = LocalContext.current
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}
    LaunchedEffect(push) {
        if (needsPermission(context) && push.enabled.value && push.askPermissionOnce()) {
            permission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }
}

private fun needsPermission(context: Context): Boolean =
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
        ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED

private fun openNotificationSettings(context: Context) {
    val intent = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
        .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
    context.startActivity(intent)
}

/** ntfy in the app store (Play or F-Droid), else its F-Droid page in the browser. */
private fun openNtfyStorePage(context: Context) {
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=$NTFY")))
    } catch (e: ActivityNotFoundException) {
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://f-droid.org/packages/$NTFY/")))
    }
}

private const val NTFY = "io.heckel.ntfy"
