package org.mtrx.app.model

import uniffi.mtrx.MtrxException

/** What to show for a failed library call; null for a cancelled sign-in. */
fun describe(error: MtrxException): String? = when (error) {
    is MtrxException.LoginCancelled -> null
    is MtrxException.NotLoggedIn -> "Not signed in"
    is MtrxException.AlreadyLoggedIn -> "Already signed in; sign out first"
    is MtrxException.RoomNotFound -> "No room ${error.roomId}"
    is MtrxException.InvalidInput -> error.message
    is MtrxException.SecretStorage -> "Secret storage: ${error.message}"
    is MtrxException.Other -> error.message
}
