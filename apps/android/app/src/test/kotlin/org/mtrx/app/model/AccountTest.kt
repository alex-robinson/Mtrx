package org.mtrx.app.model

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mtrx.app.FakeMtrx
import org.mtrx.app.FakeSession
import uniffi.mtrx.MtrxException
import uniffi.mtrx.RoomListState

@OptIn(ExperimentalCoroutinesApi::class)
class AccountTest {
    @Test
    fun withoutASavedSessionItIsSignedOut() = runTest {
        val account = Account(FakeMtrx(restored = null), CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()
        assertEquals(AccountState.SignedOut(), account.state.value)
    }

    @Test
    fun restoredSessionSyncsInTheForegroundOnly() = runTest {
        val session = FakeSession()
        val account = Account(FakeMtrx(restored = session), CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()
        assertTrue(account.state.value is AccountState.SignedIn)
        assertEquals(0, session.syncStarts)

        account.setForeground(true)
        account.setForeground(true)
        assertEquals(1, session.syncStarts)
        account.setForeground(false)
        assertEquals(1, session.syncStops)
    }

    @Test
    fun roomListUpdatesBecomeState() = runTest {
        val session = FakeSession()
        val account = Account(FakeMtrx(restored = session), CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()
        assertFalse(account.rooms.value.loaded)
        session.roomListListener!!.onUpdate(RoomListState(rooms = emptyList(), loaded = true))
        assertTrue(account.rooms.value.loaded)
    }

    @Test
    fun signingOutStopsSyncAndSubscriptions() = runTest {
        val session = FakeSession()
        val account = Account(FakeMtrx(restored = session), CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()
        account.setForeground(true)
        account.signOut()
        assertTrue(session.loggedOut)
        assertEquals(1, session.syncStops)
        assertTrue(session.handles.all { it.cancelled })
        assertEquals(AccountState.SignedOut(), account.state.value)
        // Back in the foreground, nothing syncs.
        account.setForeground(false)
        account.setForeground(true)
        assertEquals(1, session.syncStarts)
    }

    @Test
    fun failedSignOutKeepsTheSessionSyncing() = runTest {
        val session = FakeSession().apply { logoutError = MtrxException.Other("offline") }
        val account = Account(FakeMtrx(restored = session), CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()
        account.setForeground(true)
        account.signOut()
        assertTrue(account.state.value is AccountState.SignedIn)
        assertEquals(2, session.syncStarts)
        assertFalse(session.handles.any { it.cancelled })
    }

    @Test
    fun passwordSignIn() = runTest {
        val session = FakeSession()
        val mtrx = FakeMtrx(restored = null).apply { loginSession = session }
        val account = Account(mtrx, CoroutineScope(UnconfinedTestDispatcher(testScheduler)))
        account.start()
        account.setForeground(true)
        account.login("example.org", "me", "secret")
        assertTrue(account.state.value is AccountState.SignedIn)
        assertEquals(1, session.syncStarts)
    }
}
