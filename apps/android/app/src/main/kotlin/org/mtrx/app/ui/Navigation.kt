package org.mtrx.app.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import org.mtrx.app.model.Account
import org.mtrx.app.model.AccountState
import org.mtrx.app.rooms.RoomListScreen

/** The screens of a signed-in session. */
@Composable
fun SignedInNavigation(account: Account, signedIn: AccountState.SignedIn) {
    val nav = rememberNavController()
    val userId = remember(signedIn.session) { signedIn.session.userId() }
    NavHost(nav, startDestination = "rooms") {
        composable("rooms") {
            RoomListScreen(account, userId, signedIn.images, onOpenRoom = {})
        }
    }
}
