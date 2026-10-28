package org.mtrx.app.model

import android.util.Log

/**
 * Where the models note what goes wrong in the background: Android's log
 * in the app ([AndroidLog]), a list in the unit tests (which run beside
 * the screenshot tests' layoutlib, whose Log is native).
 */
interface Logger {
    fun info(tag: String, message: String)

    fun warn(tag: String, message: String)
}

object AndroidLog : Logger {
    override fun info(tag: String, message: String) {
        Log.i(tag, message)
    }

    override fun warn(tag: String, message: String) {
        Log.w(tag, message)
    }
}
