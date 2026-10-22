package org.mtrx.app.ui

import android.net.Uri
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import org.mtrx.app.model.Account
import org.mtrx.app.model.AccountState
import org.mtrx.app.rooms.RoomListScreen
import org.mtrx.app.settings.RecoveryScreen
import org.mtrx.app.settings.SessionInfo
import org.mtrx.app.settings.SettingsScreen
import org.mtrx.app.settings.VerificationRequestDialog
import org.mtrx.app.settings.VerificationScreen
import org.mtrx.app.timeline.TimelineScreen
import org.mtrx.app.timeline.TimelineViewModel

private const val ROOMS = "rooms"
private const val ROOM = "room/{roomId}"
private const val SETTINGS = "settings"
private const val RECOVERY = "settings/recovery"
private const val VERIFICATION = "settings/verification"

/**
 * The screens of a signed-in session: the room list, a room's timeline, and
 * the settings with recovery and verification; and the prompt for a
 * verification request from another device, over any of them.
 */
@Composable
fun SignedInNavigation(account: Account, signedIn: AccountState.SignedIn) {
    val nav = rememberNavController()
    val info = remember(signedIn.session) {
        val session = signedIn.session
        SessionInfo(session.userId(), session.homeserver(), session.deviceId())
    }
    // The verification screen shows the account's verification of the moment:
    // a new one replaces a finished one still on screen.
    val openVerification = {
        nav.navigate(VERIFICATION) { popUpTo(VERIFICATION) { inclusive = true } }
    }
    val request by account.verificationRequest.collectAsStateWithLifecycle()
    if (request != null) {
        VerificationRequestDialog(
            onAccept = {
                account.acceptVerificationRequest()
                openVerification()
            },
            onDecline = { account.declineVerificationRequest() },
        )
    }

    NavHost(nav, startDestination = ROOMS) {
        composable(ROOMS) {
            RoomListScreen(
                account,
                info.userId,
                signedIn.images,
                onOpenRoom = { roomId -> nav.navigate("room/${Uri.encode(roomId)}") },
                onOpenSettings = { nav.navigate(SETTINGS) },
            )
        }
        composable(SETTINGS) {
            SettingsScreen(
                account,
                info,
                onBack = { nav.popBackStack() },
                onEnterRecoveryKey = { nav.navigate(RECOVERY) },
                onVerify = {
                    account.verifyWithAnotherDevice()
                    openVerification()
                },
            )
        }
        composable(RECOVERY) {
            RecoveryScreen(account, onBack = { nav.popBackStack() }, onDone = { nav.popBackStack() })
        }
        composable(VERIFICATION) {
            VerificationScreen(account, onClose = { nav.popBackStack() })
        }
        composable(ROOM, arguments = listOf(navArgument("roomId") { type = NavType.StringType })) { entry ->
            val roomId = entry.arguments?.getString("roomId") ?: return@composable
            // One per visit of the room (the back stack entry's).
            val viewModel: TimelineViewModel = viewModel {
                TimelineViewModel(roomId) { signedIn.session.timeline(it) }
            }
            val rooms by account.rooms.collectAsStateWithLifecycle()
            val title = rooms.rooms.firstOrNull { it.id == roomId }?.name ?: roomId
            TimelineScreen(title, viewModel, signedIn.images, onBack = { nav.popBackStack() })
        }
    }
}
