package org.mtrx.app

import java.time.Instant
import uniffi.mtrx.ConnectionListener
import uniffi.mtrx.ConnectionState
import uniffi.mtrx.EventContent
import uniffi.mtrx.Message
import uniffi.mtrx.MessageKind
import uniffi.mtrx.MtrxException
import uniffi.mtrx.MtrxInterface
import uniffi.mtrx.NoHandle
import uniffi.mtrx.OAuthLogin
import uniffi.mtrx.RoomListListener
import uniffi.mtrx.SendState
import uniffi.mtrx.Session
import uniffi.mtrx.TaskHandle
import uniffi.mtrx.Timeline
import uniffi.mtrx.TimelineEvent
import uniffi.mtrx.TimelineItem
import uniffi.mtrx.TimelineItemContent
import uniffi.mtrx.TimelineListener

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

    override fun userId(): String = "@me:example.org"

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
