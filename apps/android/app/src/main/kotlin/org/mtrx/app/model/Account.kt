package org.mtrx.app.model

import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.mtrx.ConnectionListener
import uniffi.mtrx.ConnectionState
import uniffi.mtrx.MtrxException
import uniffi.mtrx.MtrxInterface
import uniffi.mtrx.OAuthLogin
import uniffi.mtrx.RoomListListener
import uniffi.mtrx.RoomListState
import uniffi.mtrx.Session
import uniffi.mtrx.TaskHandle

sealed interface AccountState {
    /** Restoring the saved session. */
    data object Loading : AccountState

    data class SignedOut(val error: String? = null) : AccountState

    data class SignedIn(val session: Session, val images: Images) : AccountState
}

/** A browser sign-in, from the sign-in page to the redirect. */
sealed interface BrowserLogin {
    data object Idle : BrowserLogin

    /** The sign-in page is open in the browser. */
    data object Waiting : BrowserLogin

    /** The browser returned; completing the sign-in. */
    data object Finishing : BrowserLogin

    data class Failed(val message: String) : BrowserLogin
}

/**
 * The app's one account: signs in, restores and signs out the session, syncs
 * it while the app is in the foreground, and keeps its room list and
 * connection state as flows. Lives as long as the process; [scope] runs on
 * the main thread. Listener callbacks arrive on the library's threads and
 * only set flows.
 */
class Account(private val mtrx: MtrxInterface, private val scope: CoroutineScope) {
    private val _state = MutableStateFlow<AccountState>(AccountState.Loading)
    val state: StateFlow<AccountState> = _state.asStateFlow()

    private val _rooms = MutableStateFlow(NO_ROOMS)
    val rooms: StateFlow<RoomListState> = _rooms.asStateFlow()

    private val _connection = MutableStateFlow(ConnectionState.ONLINE)
    val connection: StateFlow<ConnectionState> = _connection.asStateFlow()

    private val _browserLogin = MutableStateFlow<BrowserLogin>(BrowserLogin.Idle)
    val browserLogin: StateFlow<BrowserLogin> = _browserLogin.asStateFlow()

    /** Errors of background work, to show briefly. */
    private val _messages = MutableSharedFlow<String>(extraBufferCapacity = 8)
    val messages: SharedFlow<String> = _messages.asSharedFlow()

    // The session, its subscriptions and its sync, changed under the lock only.
    private val lock = Mutex()
    private var session: Session? = null
    private var subscriptions: List<TaskHandle> = emptyList()
    private var foreground = false
    private var syncing = false

    private val pendingLogin = AtomicReference<OAuthLogin?>(null)

    /** Restore the saved session, if any. Call once. */
    fun start(): Job = scope.launch {
        val restored = try {
            mtrx.restore()
        } catch (e: MtrxException) {
            _state.value = AccountState.SignedOut(describe(e))
            return@launch
        }
        if (restored == null) {
            _state.value = AccountState.SignedOut()
        } else {
            lock.withLock { open(restored) }
        }
    }

    /** The app came to the foreground (sync) or went to the background (stop). */
    fun setForeground(value: Boolean): Job = scope.launch {
        lock.withLock {
            foreground = value
            updateSync()
        }
    }

    /** Throws [MtrxException]. */
    suspend fun supportsOauth(homeserver: String): Boolean = mtrx.supportsOauth(homeserver)

    /** Sign in with a password. Throws [MtrxException]. */
    suspend fun login(homeserver: String, user: String, password: String) {
        val signedIn = mtrx.login(homeserver, user, password)
        lock.withLock { open(signedIn) }
    }

    /**
     * Start a browser sign-in and return the page to open; the redirect goes
     * to [finishBrowserLogin]. Throws [MtrxException].
     */
    suspend fun startBrowserLogin(homeserver: String): String {
        val login = mtrx.startOauthLogin(homeserver)
        pendingLogin.getAndSet(login)?.close()
        _browserLogin.value = BrowserLogin.Waiting
        return login.url()
    }

    /** Give up the browser sign-in, or forget its error. */
    fun cancelBrowserLogin() {
        pendingLogin.getAndSet(null)?.close()
        _browserLogin.value = BrowserLogin.Idle
    }

    /** The browser returned to [redirectUrl]; ignored without a sign-in waiting. */
    fun finishBrowserLogin(redirectUrl: String) {
        val login = pendingLogin.getAndSet(null) ?: return
        _browserLogin.value = BrowserLogin.Finishing
        scope.launch {
            try {
                val signedIn = login.finish(redirectUrl)
                lock.withLock { open(signedIn) }
                _browserLogin.value = BrowserLogin.Idle
            } catch (e: MtrxException) {
                _browserLogin.value = describe(e)?.let { BrowserLogin.Failed(it) } ?: BrowserLogin.Idle
            } finally {
                login.close()
            }
        }
    }

    /** Sign out on the server and delete the account's data on this device. */
    fun signOut(): Job = scope.launch {
        lock.withLock {
            val current = session ?: return@withLock
            stopSync()
            try {
                current.logout()
            } catch (e: MtrxException) {
                _messages.tryEmit("Could not sign out: ${describe(e)}")
                updateSync()
                return@withLock
            }
            close()
        }
    }

    /**
     * After [ConnectionState.SIGNED_OUT] (the server no longer accepts the
     * session): delete its data, to sign in again.
     */
    fun forget(): Job = scope.launch {
        lock.withLock {
            val current = session ?: return@withLock
            stopSync()
            try {
                current.forget()
            } catch (e: MtrxException) {
                _messages.tryEmit("Could not delete the session's data: ${describe(e)}")
            }
            close()
        }
    }

    // Under the lock.
    private suspend fun open(signedIn: Session) {
        check(session == null) { "already signed in" }
        session = signedIn
        _connection.value = signedIn.connection()
        val handles = mutableListOf<TaskHandle>()
        handles += signedIn.subscribeConnection(object : ConnectionListener {
            override fun onUpdate(state: ConnectionState) {
                _connection.value = state
            }
        })
        try {
            handles += signedIn.subscribeRoomList(object : RoomListListener {
                override fun onUpdate(state: RoomListState) {
                    _rooms.value = state
                }
            })
        } catch (e: MtrxException) {
            _messages.tryEmit("No room list: ${describe(e)}")
        }
        subscriptions = handles
        _state.value = AccountState.SignedIn(signedIn, Images(signedIn))
        updateSync()
    }

    // Under the lock.
    private suspend fun updateSync() {
        val current = session
        if (current != null && foreground && !syncing) {
            current.startSync()
            syncing = true
        } else if ((current == null || !foreground) && syncing) {
            stopSync()
        }
    }

    // Under the lock.
    private suspend fun stopSync() {
        if (syncing) {
            session?.stopSync()
            syncing = false
        }
    }

    // Under the lock, after logout or forget.
    private fun close() {
        _state.value = AccountState.SignedOut()
        subscriptions.forEach {
            it.cancel()
            it.close()
        }
        subscriptions = emptyList()
        session?.close()
        session = null
        _rooms.value = NO_ROOMS
        _connection.value = ConnectionState.ONLINE
    }

    private companion object {
        val NO_ROOMS = RoomListState(rooms = emptyList(), loaded = false)
    }
}
