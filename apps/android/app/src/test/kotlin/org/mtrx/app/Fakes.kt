package org.mtrx.app

import java.time.Instant
import uniffi.mtrx.ConnectionListener
import uniffi.mtrx.ConnectionState
import uniffi.mtrx.DeviceInfo
import uniffi.mtrx.EncryptionState
import uniffi.mtrx.EventContent
import uniffi.mtrx.Message
import uniffi.mtrx.MessageKind
import uniffi.mtrx.MtrxException
import uniffi.mtrx.MtrxInterface
import uniffi.mtrx.NoHandle
import uniffi.mtrx.OAuthLogin
import uniffi.mtrx.RecoveryListener
import uniffi.mtrx.RecoveryStatus
import uniffi.mtrx.RecoveryStep
import uniffi.mtrx.RoomListListener
import uniffi.mtrx.SendState
import uniffi.mtrx.Session
import uniffi.mtrx.TaskHandle
import uniffi.mtrx.Timeline
import uniffi.mtrx.TimelineEvent
import uniffi.mtrx.TimelineItem
import uniffi.mtrx.TimelineItemContent
import uniffi.mtrx.TimelineListener
import uniffi.mtrx.Verification
import uniffi.mtrx.VerificationListener
import uniffi.mtrx.VerificationRequestListener
import uniffi.mtrx.VerificationState

// Fakes of the library's objects: subclasses made with NoHandle, which never
// reach the native library.

class FakeHandle : TaskHandle(NoHandle) {
    var cancelled = false

    override fun cancel() {
        cancelled = true
    }

    override fun isFinished(): Boolean = cancelled
}

class FakeSession : Session(NoHandle) {
    var syncStarts = 0
    var syncStops = 0
    var loggedOut = false
    var logoutError: MtrxException? = null
    val handles = mutableListOf<FakeHandle>()
    var roomListListener: RoomListListener? = null

    var encryption = EncryptionState(deviceVerified = false, recovery = RecoveryStatus.INCOMPLETE)
    var encryptionAsks = 0

    /** The steps [recover] reports, then [recoveryError] if set. */
    var recoverySteps: List<RecoveryStep> = emptyList()
    var recoveryError: MtrxException? = null

    /** Called after each recovery step reached the listener. */
    var onRecoveryStep: () -> Unit = {}
    val recoveryKeys = mutableListOf<String>()

    var selfVerification: FakeVerification = FakeVerification(USER_ID)
    var verificationRequests: VerificationRequestListener? = null

    override fun userId(): String = USER_ID

    override fun connection(): ConnectionState = ConnectionState.ONLINE

    override suspend fun startSync() {
        syncStarts++
    }

    override suspend fun stopSync() {
        syncStops++
    }

    override fun subscribeConnection(listener: ConnectionListener): TaskHandle = FakeHandle().also { handles += it }

    override suspend fun subscribeRoomList(listener: RoomListListener): TaskHandle {
        roomListListener = listener
        return FakeHandle().also { handles += it }
    }

    override suspend fun logout() {
        logoutError?.let { throw it }
        loggedOut = true
    }

    override suspend fun encryptionState(): EncryptionState {
        encryptionAsks++
        return encryption
    }

    override suspend fun recover(recoveryKey: String, listener: RecoveryListener) {
        recoveryKeys += recoveryKey
        for (step in recoverySteps) {
            listener.onUpdate(step)
            onRecoveryStep()
        }
        recoveryError?.let { throw it }
    }

    override suspend fun devices(): List<DeviceInfo> =
        listOf(DeviceInfo(id = "PHONE", name = "Mtrx Android", verified = false, current = true))

    override suspend fun requestSelfVerification(): Verification = selfVerification

    override fun subscribeVerificationRequests(listener: VerificationRequestListener): TaskHandle {
        verificationRequests = listener
        return FakeHandle().also { handles += it }
    }

    companion object {
        const val USER_ID = "@me:example.org"
    }
}

/** A verification with another device of [otherUser]; the test drives [listener]. */
class FakeVerification(private val otherUser: String) : Verification(NoHandle) {
    var listener: VerificationListener? = null
    var subscription: FakeHandle? = null
    var accepted = false
    var confirmed = false
    var mismatched = false
    var cancelled = false
    var current: VerificationState = VerificationState.Waiting

    override fun otherUser(): String = otherUser

    override fun state(): VerificationState = current

    override fun subscribe(listener: VerificationListener): TaskHandle {
        this.listener = listener
        listener.onUpdate(current)
        return FakeHandle().also { subscription = it }
    }

    /** The library reports [state]. */
    fun update(state: VerificationState) {
        current = state
        listener?.onUpdate(state)
    }

    override suspend fun accept() {
        accepted = true
    }

    override suspend fun confirm() {
        confirmed = true
        update(VerificationState.Confirmed)
    }

    override suspend fun mismatch() {
        mismatched = true
        update(VerificationState.Cancelled("The short authentication strings did not match"))
    }

    override suspend fun cancel() {
        cancelled = true
        update(VerificationState.Cancelled("The user cancelled the verification"))
    }
}

class FakeMtrx(private val restored: Session?) : MtrxInterface {
    var loginSession: Session? = null

    override suspend fun login(homeserver: String, user: String, password: String): Session =
        loginSession ?: throw MtrxException.Other("wrong password")

    override suspend fun restore(): Session? = restored

    override suspend fun startOauthLogin(homeserver: String): OAuthLogin = throw MtrxException.Other("no OAuth")

    override suspend fun supportsOauth(homeserver: String): Boolean = false
}

class FakeTimeline : Timeline(NoHandle) {
    var listener: TimelineListener? = null
    val sent = mutableListOf<String>()
    var sendError: MtrxException? = null
    var pages = 0
    var markedRead = 0

    override fun roomId(): String = "!room:example.org"

    override fun subscribe(listener: TimelineListener): TaskHandle {
        this.listener = listener
        return FakeHandle()
    }

    override suspend fun paginateBack(events: UShort): Boolean {
        pages++
        return true
    }

    override suspend fun sendText(text: String) {
        sendError?.let { throw it }
        sent += text
    }

    override suspend fun markRead() {
        markedRead++
    }
}

fun textEvent(id: String, sender: String, body: String, own: Boolean = false, at: Long = 0): TimelineItem = TimelineItem(
    id = id,
    content = TimelineItemContent.Event(
        TimelineEvent(
            eventId = "\$$id",
            senderId = sender,
            senderName = null,
            senderAvatarUrl = null,
            timestamp = Instant.ofEpochSecond(at),
            own = own,
            content = EventContent.Message(Message(kind = MessageKind.TEXT, body = body, edited = false, media = null)),
            sendState = SendState.Sent,
        ),
    ),
)

fun stateEvent(id: String, sender: String, content: EventContent): TimelineItem = TimelineItem(
    id = id,
    content = TimelineItemContent.Event(
        TimelineEvent(
            eventId = "\$$id",
            senderId = sender,
            senderName = null,
            senderAvatarUrl = null,
            timestamp = Instant.EPOCH,
            own = false,
            content = content,
            sendState = SendState.Sent,
        ),
    ),
)
