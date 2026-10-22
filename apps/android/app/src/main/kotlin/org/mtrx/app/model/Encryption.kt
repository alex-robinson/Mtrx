package org.mtrx.app.model

import uniffi.mtrx.EncryptionState
import uniffi.mtrx.RecoveryStatus
import uniffi.mtrx.RecoveryStep

/**
 * Whether the room list asks to verify this device or enter the recovery
 * key: this device is not verified, or recovery is not unlocked on it. Not
 * while the state is not known (null, or recovery not known before the
 * first sync).
 */
fun showsEncryptionBanner(state: EncryptionState?): Boolean =
    state != null &&
        state.recovery != RecoveryStatus.UNKNOWN &&
        (!state.deviceVerified || state.recovery == RecoveryStatus.INCOMPLETE)

/** A recovery with the recovery key ([Account.recover]). */
sealed interface Recovery {
    data object Idle : Recovery

    /** [step] is null until the library reports the first. */
    data class Running(val step: RecoveryStep?) : Recovery

    data class Failed(val reason: String) : Recovery

    data object Done : Recovery
}
