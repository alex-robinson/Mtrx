package org.mtrx.app.signin

import android.content.ActivityNotFoundException
import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import org.mtrx.app.R
import org.mtrx.app.model.BrowserLogin

@Composable
fun SignInScreen(viewModel: SignInViewModel, restoreError: String?) {
    val form by viewModel.form.collectAsStateWithLifecycle()
    val browser by viewModel.browser.collectAsStateWithLifecycle()
    val context = LocalContext.current

    // Open the server's sign-in page once the browser sign-in started.
    LaunchedEffect(form.browserUrl) {
        val url = form.browserUrl ?: return@LaunchedEffect
        try {
            CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(url))
            viewModel.onBrowserOpened()
        } catch (e: ActivityNotFoundException) {
            viewModel.onNoBrowser()
        }
    }
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) { viewModel.onResumed() }

    val inBrowser = browser == BrowserLogin.Waiting || browser == BrowserLogin.Finishing
    val idle = !form.busy && !inBrowser
    val error = (browser as? BrowserLogin.Failed)?.message ?: form.error ?: restoreError.takeIf {
        form.step == SignInStep.Homeserver
    }

    Surface(Modifier.fillMaxSize()) {
        Box(
            Modifier
                .fillMaxSize()
                .safeDrawingPadding()
                .verticalScroll(rememberScrollState()),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                Modifier
                    .widthIn(max = 420.dp)
                    .fillMaxWidth()
                    .padding(24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Image(painterResource(R.drawable.logo), contentDescription = null, modifier = Modifier.size(96.dp))
                Text("Mtrx", style = MaterialTheme.typography.headlineLarge)
                Text("Sign in to your Matrix account", style = MaterialTheme.typography.bodyMedium)

                OutlinedTextField(
                    value = form.homeserver,
                    onValueChange = viewModel::onHomeserver,
                    modifier = Modifier.fillMaxWidth(),
                    enabled = idle && form.step == SignInStep.Homeserver,
                    label = { Text("Homeserver") },
                    placeholder = { Text("matrix.org") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
                    keyboardActions = KeyboardActions(onGo = { viewModel.next() }),
                )

                when (form.step) {
                    SignInStep.Homeserver -> Button(
                        onClick = viewModel::next,
                        enabled = idle && form.homeserver.isNotBlank(),
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(if (form.busy) "Checking the server…" else "Continue") }

                    SignInStep.Browser -> BrowserStep(
                        browser = browser,
                        idle = idle,
                        onSignIn = viewModel::signInWithBrowser,
                        onCancel = viewModel::cancelBrowser,
                    )

                    SignInStep.Password -> PasswordStep(
                        form = form,
                        idle = idle,
                        onUser = viewModel::onUser,
                        onPassword = viewModel::onPassword,
                        onSignIn = viewModel::signInWithPassword,
                    )
                }

                if (form.step != SignInStep.Homeserver && !inBrowser) {
                    TextButton(onClick = viewModel::changeServer, enabled = !form.busy) { Text("Change server") }
                }
                if (error != null) {
                    Text(error, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
    }
}

@Composable
private fun BrowserStep(browser: BrowserLogin, idle: Boolean, onSignIn: () -> Unit, onCancel: () -> Unit) {
    when (browser) {
        BrowserLogin.Waiting -> {
            Text("Continue in your browser…")
            OutlinedButton(onClick = onCancel) { Text("Cancel") }
        }
        BrowserLogin.Finishing -> Row(
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
            Text("Signing in…")
        }
        BrowserLogin.Idle, is BrowserLogin.Failed -> {
            Button(onClick = onSignIn, enabled = idle, modifier = Modifier.fillMaxWidth()) {
                Text("Sign in with browser")
            }
            Text(
                "The server's sign-in page opens in your browser",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun PasswordStep(
    form: SignInForm,
    idle: Boolean,
    onUser: (String) -> Unit,
    onPassword: (String) -> Unit,
    onSignIn: () -> Unit,
) {
    HorizontalDivider()
    OutlinedTextField(
        value = form.user,
        onValueChange = onUser,
        modifier = Modifier.fillMaxWidth(),
        enabled = idle,
        label = { Text("Username") },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Next),
    )
    OutlinedTextField(
        value = form.password,
        onValueChange = onPassword,
        modifier = Modifier.fillMaxWidth(),
        enabled = idle,
        label = { Text("Password") },
        singleLine = true,
        visualTransformation = PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { onSignIn() }),
    )
    Button(
        onClick = onSignIn,
        enabled = idle && form.user.isNotBlank() && form.password.isNotEmpty(),
        modifier = Modifier.fillMaxWidth(),
    ) { Text(if (form.busy) "Signing in…" else "Sign in") }
}
