package org.mtrx.app.settings

import org.junit.Assert.assertEquals
import org.junit.Test
import org.mtrx.app.model.Distributor
import org.mtrx.app.model.PushState

class NotificationSettingsTest {
    private val ntfy = Distributor("io.heckel.ntfy", "ntfy")
    private val other = Distributor("org.example.push", "Push")

    private fun texts(enabled: Boolean, state: PushState, allowed: Boolean = true) =
        notificationsLines(enabled, state, allowed).map(::lineText)

    @Test
    fun registeredSaysThroughWhichApp() {
        assertEquals(listOf("Via ntfy"), texts(true, PushState.Registered(ntfy)))
        assertEquals(PushSwitch(checked = true, usable = true), pushSwitch(true, PushState.Registered(ntfy)))
    }

    @Test
    fun withoutADistributorItAsksToInstallOneAndTheSwitchIsOff() {
        assertEquals(
            listOf("Install a UnifiedPush app, such as ntfy, to get notifications while Mtrx is closed."),
            texts(true, PushState.NoDistributor),
        )
        assertEquals(listOf(NotificationsLine.Install), notificationsLines(true, PushState.NoDistributor, true))
        assertEquals(PushSwitch(checked = false, usable = false), pushSwitch(true, PushState.NoDistributor))
    }

    @Test
    fun severalDistributorsAreOffered() {
        val state = PushState.Choose(listOf(ntfy, other))
        assertEquals(listOf(NotificationsLine.Choose(listOf(ntfy, other))), notificationsLines(true, state, true))
        assertEquals(listOf("Several apps can deliver notifications"), texts(true, state))
    }

    @Test
    fun offShowsNothingUnderTheSwitch() {
        assertEquals(emptyList<String>(), texts(false, PushState.Off, allowed = false))
        assertEquals(PushSwitch(checked = false, usable = true), pushSwitch(false, PushState.Off))
    }

    @Test
    fun notificationsOffInAndroidAreSaid() {
        assertEquals(
            listOf("Via ntfy", "Notifications are off in Android's settings"),
            texts(true, PushState.Registered(ntfy), allowed = false),
        )
    }

    @Test
    fun failuresAndRegisteringAreSaid() {
        assertEquals(listOf("Registration failed: no network"), texts(true, PushState.Failed("no network")))
        assertEquals(listOf("Registering…"), texts(true, PushState.Registering(ntfy)))
    }
}
