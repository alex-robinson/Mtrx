package org.mtrx.app.model

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mtrx.app.FakeVerification
import uniffi.mtrx.MtrxException
import uniffi.mtrx.VerificationEmoji
import uniffi.mtrx.VerificationState

@OptIn(ExperimentalCoroutinesApi::class)
class VerificationModelTest {
    private val verification = FakeVerification("@me:example.org")

    private fun TestScope.requested(): VerificationModel =
        VerificationModel.request(CoroutineScope(UnconfinedTestDispatcher(testScheduler))) {
            verification
        }

    private val emojis = VerificationState.Emojis(
        listOf("🐶" to "Dog", "🔑" to "Key", "🎩" to "Hat", "🌵" to "Cactus", "🚀" to "Rocket", "⏰" to "Clock", "🍄" to "Mushroom")
            .map { (symbol, description) -> VerificationEmoji(symbol, description) },
    )

    @Test
    fun followsTheStatesUntilDone() = runTest {
        val model = requested()
        assertTrue(model.outgoing)
        assertEquals(VerificationState.Waiting, model.state.value)
        assertFalse(model.busy.value)

        verification.update(emojis)
        assertEquals(emojis, model.state.value)
        model.confirm()
        assertTrue(verification.confirmed)
        assertEquals(VerificationState.Confirmed, model.state.value)
        verification.update(VerificationState.SharingKeys)
        assertFalse(model.ended)
        verification.update(VerificationState.Done)
        assertTrue(model.ended)

        model.close()
        assertTrue(verification.subscription!!.cancelled)
        assertTrue(verification.uniffiIsDestroyed)
        assertFalse(verification.cancelled)
    }

    @Test
    fun cancelledEndsWithTheReason() = runTest {
        val model = requested()
        model.cancel()
        assertTrue(verification.cancelled)
        assertEquals(VerificationState.Cancelled("The user cancelled the verification"), model.state.value)
        assertTrue(model.ended)
        // Nothing to answer after it ended.
        model.confirm()
        assertFalse(verification.confirmed)
    }

    @Test
    fun mismatchCancels() = runTest {
        val model = requested()
        verification.update(emojis)
        model.mismatch()
        assertTrue(verification.mismatched)
        assertTrue(model.state.value is VerificationState.Cancelled)
    }

    @Test
    fun closingAnUnfinishedVerificationCancelsIt() = runTest {
        val model = requested()
        verification.update(emojis)
        model.close()
        assertTrue(verification.cancelled)
        assertTrue(verification.uniffiIsDestroyed)
    }

    @Test
    fun failedRequestEndsCancelledWithTheReason() = runTest {
        val model = VerificationModel.request(CoroutineScope(UnconfinedTestDispatcher(testScheduler))) {
            throw MtrxException.Other("account has no cross-signing identity")
        }
        assertEquals(VerificationState.Cancelled("account has no cross-signing identity"), model.state.value)
        assertFalse(model.busy.value)
    }

    @Test
    fun acceptingAnIncomingRequestWaitsForTheOtherDevice() = runTest {
        val model = VerificationModel.accept(CoroutineScope(UnconfinedTestDispatcher(testScheduler)), verification)
        assertFalse(model.outgoing)
        assertTrue(verification.accepted)
        assertEquals(VerificationState.Waiting, model.state.value)
        verification.update(emojis)
        assertEquals(emojis, model.state.value)
    }
}
