package org.mtrx.app.push

import android.content.Intent
import android.os.PowerManager
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import org.mtrx.app.MtrxApplication
import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.PushService
import org.unifiedpush.android.connector.data.PushEndpoint
import org.unifiedpush.android.connector.data.PushMessage

/**
 * The connector's service: endpoints and unregistrations go to the app's
 * [PushRegistration][org.mtrx.app.model.PushRegistration], pushes to its
 * [PushHandler] in a coroutine of this service. The connector binds the
 * service for a few seconds per event; while pushes are handled it keeps
 * itself started (allowed while the distributor raises the app to the
 * foreground) and holds a wake lock, so they can take the library's ~10 s.
 * Runs in the app's process, after [MtrxApplication.onCreate].
 */
class MtrxPushService : PushService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private var running = 0
    private var started = false

    private val app get() = application as MtrxApplication

    override fun onNewEndpoint(endpoint: PushEndpoint, instance: String) {
        app.push.onNewEndpoint(endpoint.url)
    }

    override fun onRegistrationFailed(reason: FailedReason, instance: String) {
        app.push.onRegistrationFailed(describe(reason))
    }

    override fun onUnregistered(instance: String) {
        app.push.onUnregistered()
    }

    override fun onMessage(message: PushMessage, instance: String) {
        val wakeLock = getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "mtrx:push")
            .apply { acquire(WAKE_LOCK_MS) }
        begin()
        scope.launch {
            try {
                app.pushHandler.handle(message.content)
            } finally {
                if (wakeLock.isHeld) wakeLock.release()
                end()
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int = START_NOT_STICKY

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    // The connector calls back on the main thread or its own; one at a time.
    @Synchronized
    private fun begin() {
        running++
        if (started) return
        started = try {
            startService(Intent(this, MtrxPushService::class.java))
            true
        } catch (e: IllegalStateException) {
            // Not raised to the foreground: handled while the connector binds us.
            Log.w(TAG, "Could not keep the push service running: $e")
            false
        }
    }

    @Synchronized
    private fun end() {
        running--
        if (running == 0 && started) {
            started = false
            stopSelf()
        }
    }

    private companion object {
        const val TAG = "MtrxPush"

        /** Longer than the handler's timeout (20 s). */
        const val WAKE_LOCK_MS = 25_000L

        fun describe(reason: FailedReason): String = when (reason) {
            FailedReason.NETWORK -> "no network"
            FailedReason.ACTION_REQUIRED -> "the distributor needs you to act in its app"
            FailedReason.VAPID_REQUIRED -> "the distributor requires a VAPID key"
            FailedReason.INTERNAL_ERROR -> "the distributor failed"
        }
    }
}
