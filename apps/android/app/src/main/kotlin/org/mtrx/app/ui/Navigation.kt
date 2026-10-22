package org.mtrx.app.ui

import android.net.Uri
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
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
import org.mtrx.app.model.PushRegistration
import org.mtrx.app.rooms.topLevelSpaces
import org.mtrx.app.settings.AskNotificationPermissionOnce
import org.mtrx.app.settings.RecoveryScreen
import org.mtrx.app.settings.SessionInfo
import org.mtrx.app.settings.VerificationRequestDialog
import org.mtrx.app.settings.VerificationScreen
import org.mtrx.app.timeline.TimelineScreen
import org.mtrx.app.timeline.TimelineViewModel
import org.mtrx.app.timeline.roomHeader

private const val HOME = "home"
private const val ROOM = "room/{roomId}"
private const val RECOVERY = "settings/recovery"
private const val VERIFICATION = "settings/verification"

/**
 * The screens of a signed-in session: the home (chats, spaces and
 * settings), a room's timeline, recovery and verification; and the prompt
 * for a verification request from another device, over any of them.
 */
@Composable
fun SignedInNavigation(account: Account, push: PushRegistration, signedIn: AccountState.SignedIn) {
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
    AskNotificationPermissionOnce(push)

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

    NavHost(nav, startDestination = HOME) {
        composable(HOME) {
            HomeScreen(
                account,
                push,
                info,
                signedIn.images,
                signedIn.presence,
                onOpenRoom = { roomId -> nav.navigate("room/${Uri.encode(roomId)}") },
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
                TimelineViewModel(roomId, signedIn.session, onRead = { account.roomRead(roomId) }, onInput = account::input)
            }
            // Its notifications go, and pushes for it post none while it shows.
            DisposableEffect(roomId) {
                account.roomOpened(roomId)
                onDispose { account.roomClosed(roomId) }
            }
            val rooms by account.rooms.collectAsStateWithLifecycle()
            val encryption by account.encryption.collectAsStateWithLifecycle()
            val presences by signedIn.presence.presences.collectAsStateWithLifecycle()
            val room = rooms.rooms.firstOrNull { it.id == roomId }
            val spaces = remember(rooms) { topLevelSpaces(rooms) }
            TimelineScreen(
                roomHeader(roomId, room, info.userId),
                viewModel,
                signedIn.images,
                keysArriving = encryption?.keysArriving == true,
                spaces = spaces,
                presences = presences,
                refreshPresence = signedIn.presence::refresh,
                onBack = { nav.popBackStack() },
            )
        }
    }

    // A tapped notification's room, over whatever shows (after the graph is set).
    val roomRequest by account.roomRequest.collectAsStateWithLifecycle()
    LaunchedEffect(roomRequest) {
        val roomId = roomRequest ?: return@LaunchedEffect
        nav.navigate("room/${Uri.encode(roomId)}") { popUpTo(HOME) }
        account.roomRequestHandled()
    }
}
