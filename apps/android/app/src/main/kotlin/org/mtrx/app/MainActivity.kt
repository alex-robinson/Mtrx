package org.mtrx.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import org.mtrx.app.model.Account
import org.mtrx.app.model.AccountState
import org.mtrx.app.signin.SignInScreen
import org.mtrx.app.signin.SignInViewModel
import org.mtrx.app.ui.MtrxTheme
import org.mtrx.app.ui.SignedInNavigation

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val account = (application as MtrxApplication).account
        setContent {
            MtrxTheme {
                MtrxApp(account)
            }
        }
    }
}

@Composable
private fun MtrxApp(account: Account) {
    val state by account.state.collectAsStateWithLifecycle()
    when (val current = state) {
        AccountState.Loading -> Surface(Modifier.fillMaxSize()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        }
        is AccountState.SignedOut -> {
            val viewModel: SignInViewModel = viewModel { SignInViewModel(account) }
            SignInScreen(viewModel, restoreError = current.error)
        }
        // A new navigation (and new screens) for each session.
        is AccountState.SignedIn -> key(current.session) {
            SignedInNavigation(account, current)
        }
    }
}
