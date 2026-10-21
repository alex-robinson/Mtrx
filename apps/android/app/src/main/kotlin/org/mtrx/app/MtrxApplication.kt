package org.mtrx.app

import android.app.Application
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import java.io.File
import kotlinx.coroutines.MainScope
import org.mtrx.app.model.Account
import org.mtrx.app.model.AesGcmSealer
import org.mtrx.app.model.FileSecretBackend
import org.mtrx.ffi.AndroidPlatform
import uniffi.mtrx.Mtrx

class MtrxApplication : Application() {
    lateinit var account: Account
        private set

    override fun onCreate() {
        super.onCreate()
        // Before any other call into the library: its TLS verifier uses Android's.
        AndroidPlatform.initialize(this)
        val secrets = FileSecretBackend(File(noBackupFilesDir, "secrets"), AesGcmSealer.keystore())
        val mtrx = Mtrx(filesDir.absolutePath, cacheDir.absolutePath, secrets)
        account = Account(mtrx, MainScope())
        account.start()
        // Sync while the app is in the foreground.
        ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStart(owner: LifecycleOwner) {
                account.setForeground(true)
            }

            override fun onStop(owner: LifecycleOwner) {
                account.setForeground(false)
            }
        })
    }
}
