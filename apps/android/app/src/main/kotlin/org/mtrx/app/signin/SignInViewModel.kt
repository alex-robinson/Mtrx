package org.mtrx.app.signin

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.mtrx.app.model.Account
import org.mtrx.app.model.BrowserLogin
import org.mtrx.app.model.describe
import uniffi.mtrx.MtrxException

/** Sign-in in two steps: the homeserver, then a password or the browser. */
enum class SignInStep { Homeserver, Password, Browser }

data class SignInForm(
    val homeserver: String = "matrix.org",
    val user: String = "",
    val password: String = "",
    val step: SignInStep = SignInStep.Homeserver,
    val busy: Boolean = false,
    val error: String? = null,
    /** A sign-in page to open in a Custom Tab (then [SignInViewModel.onBrowserOpened]). */
    val browserUrl: String? = null,
)

class SignInViewModel(private val account: Account) : ViewModel() {
    private val _form = MutableStateFlow(SignInForm())
    val form: StateFlow<SignInForm> = _form.asStateFlow()

    /** Where a browser sign-in is (the redirect goes to [Account]). */
    val browser: StateFlow<BrowserLogin> = account.browserLogin

    /** The sign-in page was shown: returning without a redirect cancels. */
    private var browserOpened = false

    fun onHomeserver(value: String) = _form.update { it.copy(homeserver = value, error = null) }

    fun onUser(value: String) = _form.update { it.copy(user = value, error = null) }

    fun onPassword(value: String) = _form.update { it.copy(password = value, error = null) }

    /** Ask the server how to sign in. */
    fun next() {
        val homeserver = _form.value.homeserver.trim()
        if (homeserver.isEmpty() || _form.value.busy) return
        account.cancelBrowserLogin()
        _form.update { it.copy(busy = true, error = null) }
        viewModelScope.launch {
            try {
                val oauth = account.supportsOauth(homeserver)
                _form.update {
                    it.copy(busy = false, step = if (oauth) SignInStep.Browser else SignInStep.Password)
                }
            } catch (e: MtrxException) {
                _form.update { it.copy(busy = false, error = describe(e)) }
            }
        }
    }

    /** Back to the homeserver. */
    fun changeServer() {
        account.cancelBrowserLogin()
        browserOpened = false
        _form.update { it.copy(step = SignInStep.Homeserver, password = "", busy = false, error = null) }
    }

    fun signInWithPassword() {
        val form = _form.value
        if (form.busy || form.user.isBlank() || form.password.isEmpty()) return
        _form.update { it.copy(busy = true, error = null) }
        viewModelScope.launch {
            try {
                account.login(form.homeserver.trim(), form.user.trim(), form.password)
                // Signed in: the app shows the rooms. Ready for a later sign-in.
                _form.value = SignInForm(homeserver = form.homeserver)
            } catch (e: MtrxException) {
                _form.update { it.copy(busy = false, error = describe(e)) }
            }
        }
    }

    fun signInWithBrowser() {
        val form = _form.value
        if (form.busy) return
        _form.update { it.copy(busy = true, error = null) }
        viewModelScope.launch {
            try {
                val url = account.startBrowserLogin(form.homeserver.trim())
                _form.update { it.copy(busy = false, browserUrl = url) }
            } catch (e: MtrxException) {
                _form.update { it.copy(busy = false, error = describe(e)) }
            }
        }
    }

    fun onBrowserOpened() {
        browserOpened = true
        _form.update { it.copy(browserUrl = null) }
    }

    /** No app can show the sign-in page. */
    fun onNoBrowser() {
        account.cancelBrowserLogin()
        _form.update { it.copy(browserUrl = null, error = "No browser to sign in with") }
    }

    fun cancelBrowser() {
        account.cancelBrowserLogin()
        browserOpened = false
    }

    /**
     * The screen is back. If the browser was left without signing in (the
     * redirect would have come first), the sign-in is cancelled.
     */
    fun onResumed() {
        if (browserOpened && account.browserLogin.value == BrowserLogin.Waiting) {
            cancelBrowser()
        }
        if (account.browserLogin.value != BrowserLogin.Waiting) browserOpened = false
    }
}
