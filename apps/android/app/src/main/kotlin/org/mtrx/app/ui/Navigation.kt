package org.mtrx.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Button
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import org.mtrx.app.model.Account
import org.mtrx.app.model.AccountState

/** The screens of a signed-in session. */
@Composable
fun SignedInNavigation(account: Account, signedIn: AccountState.SignedIn) {
    val userId = remember(signedIn.session) { signedIn.session.userId() }
    Surface(Modifier.fillMaxSize()) {
        Column(
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text("Signed in as $userId")
            Button(onClick = { account.signOut() }) { Text("Sign out") }
        }
    }
}
