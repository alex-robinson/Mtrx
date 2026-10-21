package org.mtrx.ffi

import android.content.Context

/**
 * Android set-up of the Mtrx library: call [initialize] once at app start
 * (e.g. in `Application.onCreate`), before any call into `uniffi.mtrx`.
 */
object AndroidPlatform {
    init {
        System.loadLibrary("mtrx_ffi")
    }

    /** The platform TLS verifier (rustls-platform-verifier) needs the app's context. */
    @JvmStatic
    external fun initialize(context: Context)
}
