package org.mtrx.app.model

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mtrx.app.FakeMtrx
import org.mtrx.app.FakeSession
import org.mtrx.app.FakeSortStore
import org.mtrx.app.FakeSpaceStore
import org.mtrx.app.testAccount

@OptIn(ExperimentalCoroutinesApi::class)
class RoomSortChoiceTest {
    private fun TestScope.scope() = CoroutineScope(UnconfinedTestDispatcher(testScheduler))

    @Test
    fun withoutASavedOrderTheListsOrderIsUsed() = runTest {
        assertEquals(RoomSort.DEFAULT, RoomSortChoice(FakeSortStore(), scope()).sort.value)
    }

    @Test
    fun theChosenOrderIsSavedAndRestoredAtTheNextLaunch() = runTest {
        val store = FakeSortStore()
        val first = RoomSortChoice(store, scope())
        first.choose(RoomSort.Z_TO_A)
        assertEquals(RoomSort.Z_TO_A, first.sort.value)
        assertEquals(RoomSort.Z_TO_A, store.saved)

        assertEquals(RoomSort.Z_TO_A, RoomSortChoice(store, scope()).sort.value)
    }

    @Test
    fun aChoiceBeforeTheSavedOrderIsReadWins() = runTest {
        val store = FakeSortStore(RoomSort.A_TO_Z).apply { loading = CompletableDeferred() }
        val choice = RoomSortChoice(store, scope())
        choice.choose(RoomSort.RECENT)
        store.loading!!.complete(Unit)
        assertEquals(RoomSort.RECENT, choice.sort.value)
        assertEquals(RoomSort.RECENT, store.saved)
    }

    @Test
    fun signingOutKeepsTheOrder() = runTest {
        val store = FakeSortStore(RoomSort.RECENT)
        val account = testAccount(FakeMtrx(restored = FakeSession()), scope(), FakeSpaceStore(), store)
        account.start()
        account.signOut()
        assertEquals(RoomSort.RECENT, account.roomSort.sort.value)
        assertEquals(RoomSort.RECENT, store.saved)
    }
}
