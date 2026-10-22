package org.mtrx.app.model

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mtrx.app.FakeMtrx
import org.mtrx.app.FakeSession
import org.mtrx.app.FakeVerification
import uniffi.mtrx.EncryptionState
import uniffi.mtrx.MtrxException
import uniffi.mtrx.RecoveryStatus
import uniffi.mtrx.RecoveryStep
import uniffi.mtrx.RoomListState
import uniffi.mtrx.VerificationState

@OptIn(ExperimentalCoroutinesApi::class)
class EncryptionTest {
    private fun state(verified: Boolean, recovery: RecoveryStatus) = EncryptionState(verified, recovery)

    private fun loaded(value: Boolean) = RoomListState(rooms = emptyList(), loaded = value)

    @Test
    fun bannerWhileNotVerifiedOrRecoveryNotUnlocked() {
        assertFalse(showsEncryptionBanner(null))
        assertFalse(showsEncryptionBanner(state(false, RecoveryStatus.UNKNOWN)))
        assertFalse(showsEncryptionBanner(state(true, RecoveryStatus.ENABLED)))
        assertFalse(showsEncryptionBanner(state(true, RecoveryStatus.DISABLED)))
        assertTrue(showsEncryptionBanner(state(false, RecoveryStatus.ENABLED)))
        assertTrue(showsEncryptionBanner(state(false, RecoveryStatus.DISABLED)))
        assertTrue(showsEncryptionBanner(state(true, RecoveryStatus.INCOMPLETE)))
    }

    @Test
    fun askedOnceTheRoomListLoadsAndBackInTheForeground() = runTest {
        val session = FakeSession()
        val account = Account(FakeMtrx(restored = session), CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()
        account.setForeground(true)
        assertEquals(0, session.encryptionAsks)
        assertNull(account.encryption.value)

        session.roomListListener!!.onUpdate(loaded(true))
        assertEquals(1, session.encryptionAsks)
        assertEquals(session.encryption, account.encryption.value)
        assertTrue(showsEncryptionBanner(account.encryption.value))

        // Known: later updates do not ask; coming back to the foreground does.
        session.roomListListener!!.onUpdate(loaded(false))
        session.roomListListener!!.onUpdate(loaded(true))
        assertEquals(1, session.encryptionAsks)
        account.setForeground(false)
        account.setForeground(true)
        assertEquals(2, session.encryptionAsks)
    }

    @Test
    fun unknownRecoveryIsAskedAgainWithTheRoomList() = runTest {
        val session = FakeSession().apply { encryption = EncryptionState(false, RecoveryStatus.UNKNOWN) }
        val account = Account(FakeMtrx(restored = session), CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()
        account.setForeground(true)
        session.roomListListener!!.onUpdate(loaded(true))
        assertFalse(showsEncryptionBanner(account.encryption.value))

        session.encryption = EncryptionState(true, RecoveryStatus.INCOMPLETE)
        session.roomListListener!!.onUpdate(loaded(false))
        session.roomListListener!!.onUpdate(loaded(true))
        assertEquals(2, session.encryptionAsks)
        assertTrue(showsEncryptionBanner(account.encryption.value))
    }

    @Test
    fun recoveryReportsItsStepsInOrderThenAsksAgain() = runTest {
        val session = FakeSession()
        val account = Account(FakeMtrx(restored = session), CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()
        val steps = listOf(
            RecoveryStep.CheckingKey,
            RecoveryStep.ImportingIdentity,
            RecoveryStep.DownloadingKeys(0u, 2u),
            RecoveryStep.DownloadingKeys(1u, 2u),
        )
        val seen = mutableListOf<Recovery>()
        session.recoverySteps = steps
        session.onRecoveryStep = { seen += account.recovery.value }
        session.encryption = EncryptionState(true, RecoveryStatus.ENABLED)

        account.recover("EsTc 1234")
        assertEquals(listOf("EsTc 1234"), session.recoveryKeys)
        assertEquals(steps.map { Recovery.Running(it) }, seen)
        assertEquals(Recovery.Done, account.recovery.value)
        assertEquals(1, session.encryptionAsks)
        assertFalse(showsEncryptionBanner(account.encryption.value))

        account.resetRecovery()
        assertEquals(Recovery.Idle, account.recovery.value)
    }

    @Test
    fun failedRecoveryGivesTheReason() = runTest {
        val session = FakeSession().apply {
            recoverySteps = listOf(RecoveryStep.CheckingKey)
            recoveryError = MtrxException.Other("The Base58 decoded key has an invalid length")
        }
        val account = Account(FakeMtrx(restored = session), CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()
        account.recover("wrong")
        assertEquals(Recovery.Failed("The Base58 decoded key has an invalid length"), account.recovery.value)
        assertEquals(0, session.encryptionAsks)
    }

    @Test
    fun requestFromAnotherUserIsCancelled() = runTest {
        val session = FakeSession()
        val account = Account(FakeMtrx(restored = session), CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()
        val request = FakeVerification("@eve:example.org")
        session.verificationRequests!!.onRequest(request)
        assertTrue(request.cancelled)
        assertTrue(request.uniffiIsDestroyed)
        assertNull(account.verificationRequest.value)
    }

    @Test
    fun requestFromOurOtherDeviceIsAcceptedOrDeclined() = runTest {
        val session = FakeSession()
        val account = Account(FakeMtrx(restored = session), CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()

        val declined = FakeVerification(FakeSession.USER_ID)
        session.verificationRequests!!.onRequest(declined)
        assertEquals(declined, account.verificationRequest.value)
        account.declineVerificationRequest()
        assertTrue(declined.cancelled)
        assertNull(account.verificationRequest.value)

        val accepted = FakeVerification(FakeSession.USER_ID)
        session.verificationRequests!!.onRequest(accepted)
        account.acceptVerificationRequest()
        assertTrue(accepted.accepted)
        val model = account.verification.value!!
        assertFalse(model.outgoing)
        assertEquals(VerificationState.Waiting, model.state.value)

        // While it runs, another request is declined.
        val second = FakeVerification(FakeSession.USER_ID)
        session.verificationRequests!!.onRequest(second)
        assertTrue(second.cancelled)
        assertNull(account.verificationRequest.value)
    }

    @Test
    fun verifiedDeviceAsksAgainAndSignOutClosesTheVerification() = runTest {
        val session = FakeSession()
        val account = Account(FakeMtrx(restored = session), CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()
        account.verifyWithAnotherDevice()
        val model = account.verification.value!!
        assertTrue(model.outgoing)
        val first = session.selfVerification
        first.update(VerificationState.Done)
        assertEquals(1, session.encryptionAsks)

        // A new one, still waiting at sign-out: cancelled and closed.
        val second = FakeVerification(FakeSession.USER_ID)
        session.selfVerification = second
        account.verifyWithAnotherDevice()
        // The ended one is closed without cancelling.
        assertTrue(first.uniffiIsDestroyed)
        assertFalse(first.cancelled)
        account.signOut()
        assertNull(account.verification.value)
        assertNull(account.encryption.value)
        assertTrue(second.cancelled)
        assertTrue(second.uniffiIsDestroyed)
    }
}
