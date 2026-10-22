package org.mtrx.app

import java.time.Instant
import kotlinx.coroutines.CompletableDeferred
import org.mtrx.app.model.RoomSort
import org.mtrx.app.model.RoomSortStore
import org.mtrx.app.model.SelectedSpaceStore
import org.mtrx.app.model.SpaceEntry
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
import uniffi.mtrx.Reader
import uniffi.mtrx.RecoveryListener
import uniffi.mtrx.RecoveryStatus
import uniffi.mtrx.RecoveryStep
import uniffi.mtrx.RoomKind
import uniffi.mtrx.RoomListListener
import uniffi.mtrx.RoomListState
import uniffi.mtrx.RoomSummary
import uniffi.mtrx.SendState
import uniffi.mtrx.SpaceRooms
import uniffi.mtrx.Session
import uniffi.mtrx.TaskHandle
import uniffi.mtrx.Timeline
import uniffi.mtrx.TimelineEvent
import uniffi.mtrx.TimelineItem
import uniffi.mtrx.TimelineItemContent
import uniffi.mtrx.TimelineListener
import uniffi.mtrx.TypingListener
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

/** The app's preferences, in memory: one instance across "launches". */
class FakeSpaceStore(var saved: SpaceEntry? = null) : SelectedSpaceStore {
    override suspend fun load(): SpaceEntry? = saved

    override suspend fun save(entry: SpaceEntry?) {
        saved = entry
    }
}

/** The order of the spaces' rooms, in memory: one instance across "launches". */
class FakeSortStore(var saved: RoomSort? = null) : RoomSortStore {
    /** When set, [load] reads the saved order, then waits for it to return. */
    var loading: CompletableDeferred<Unit>? = null

    override suspend fun load(): RoomSort? {
        val read = saved
        loading?.await()
        return read
    }

    override suspend fun save(sort: RoomSort) {
        saved = sort
    }
}

fun summary(
    id: String,
    kind: RoomKind = RoomKind.ROOM,
    invited: Boolean = false,
    unread: Long = 0,
    directUserId: String? = null,
    name: String = id.removePrefix("!").substringBefore(':'),
    lastActivity: Instant? = null,
): RoomSummary = RoomSummary(
    id = id,
    name = name,
    avatarUrl = null,
    topic = null,
    kind = kind,
    invited = invited,
    encrypted = false,
    unreadMessages = unread.toULong(),
    unreadMentions = 0uL,
    unreadNotifications = unread.toULong(),
    lastActivity = lastActivity,
    directUserId = directUserId,
    alias = null,
    joinedMembers = 2uL,
)

class FakeMtrx(private val restored: Session?) : MtrxInterface {
    var loginSession: Session? = null

    override suspend fun login(homeserver: String, user: String, password: String): Session =
        loginSession ?: throw MtrxException.Other("wrong password")

    override suspend fun restore(): Session? = restored

    override suspend fun startOauthLogin(homeserver: String): OAuthLogin = throw MtrxException.Other("no OAuth")

    override suspend fun supportsOauth(homeserver: String): Boolean = false
}

/**
 * A room's timeline: the test drives [listener] and [typingListener]; the
 * actions are recorded. [paginateBack] calls [onPage] with the call's number,
 * then reports the start at call [pagesUntilStart]; [gate], when set, holds
 * the sends and actions until completed.
 */
class FakeTimeline : Timeline(NoHandle) {
    var listener: TimelineListener? = null
    var typingListener: TypingListener? = null
    val typingSubscription = FakeHandle()
    val sent = mutableListOf<String>()
    var sendError: MtrxException? = null
    var actionError: MtrxException? = null
    var gate: CompletableDeferred<Unit>? = null
    var pages = 0
    var pagesUntilStart = 1
    var onPage: (Int) -> Unit = {}
    var markedRead = 0
    val replies = mutableListOf<Pair<String, String>>()
    val edits = mutableListOf<Pair<String, String>>()
    val reactions = mutableListOf<Pair<String, String>>()
    val deleted = mutableListOf<String>()
    val retried = mutableListOf<String>()
    val discarded = mutableListOf<String>()
    val typing = mutableListOf<Boolean>()
    var closed = false

    override fun roomId(): String = "!room:example.org"

    override fun subscribe(listener: TimelineListener): TaskHandle {
        this.listener = listener
        return FakeHandle()
    }

    override fun subscribeTyping(listener: TypingListener): TaskHandle {
        typingListener = listener
        return typingSubscription
    }

    override suspend fun paginateBack(events: UShort): Boolean {
        pages++
        onPage(pages)
        return pages >= pagesUntilStart
    }

    private suspend fun action(record: () -> Unit) {
        gate?.await()
        actionError?.let { throw it }
        record()
    }

    override suspend fun sendText(text: String) {
        gate?.await()
        sendError?.let { throw it }
        sent += text
    }

    override suspend fun reply(itemId: String, text: String) = action { replies += itemId to text }

    override suspend fun edit(itemId: String, text: String) = action { edits += itemId to text }

    override suspend fun react(itemId: String, key: String) = action { reactions += itemId to key }

    override suspend fun delete(itemId: String) = action { deleted += itemId }

    override suspend fun retry(itemId: String) = action { retried += itemId }

    override suspend fun discard(itemId: String) = action { discarded += itemId }

    override suspend fun setTyping(typing: Boolean) {
        this.typing += typing
    }

    override suspend fun markRead() {
        markedRead++
    }

    override fun close() {
        closed = true
        super.close()
    }
}

fun textEvent(id: String, sender: String, body: String, own: Boolean = false, at: Long = 0): TimelineItem =
    messageEvent(id, sender, Message(kind = MessageKind.TEXT, body = body, edited = false, media = null), own = own, at = at)

/** A message's row; [eventId] null while not sent. */
fun messageEvent(
    id: String,
    sender: String,
    message: Message,
    own: Boolean = false,
    at: Long = 0,
    eventId: String? = "\$$id",
    sendState: SendState = SendState.Sent,
    editable: Boolean = false,
    readBy: List<Reader> = emptyList(),
): TimelineItem = eventItem(id, sender, EventContent.Message(message), own, at, eventId, sendState, editable, readBy)

fun eventItem(
    id: String,
    sender: String,
    content: EventContent,
    own: Boolean = false,
    at: Long = 0,
    eventId: String? = "\$$id",
    sendState: SendState = SendState.Sent,
    editable: Boolean = false,
    readBy: List<Reader> = emptyList(),
): TimelineItem = TimelineItem(
    id = id,
    content = TimelineItemContent.Event(
        TimelineEvent(
            eventId = eventId,
            senderId = sender,
            senderName = null,
            senderAvatarUrl = null,
            timestamp = Instant.ofEpochSecond(at),
            own = own,
            content = content,
            sendState = sendState,
            editable = editable,
            readBy = readBy,
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

/**
 * The rooms by space as in the library's integration test, most recently
 * active first: spaces Team and Other; Lab and, through the nested space
 * Sub, Deep in Team; an invite to Offer in Team; a direct chat with Bob, a
 * member of Team; an invite from Carol to a direct chat, and to Party,
 * outside spaces with Loose; and the Notes to self, listed in every space.
 */
object SpacesFixture {
    const val TEAM = "!team:x"
    const val OTHER = "!other:x"
    const val SUB = "!sub:x"
    const val LAB = "!lab:x"
    const val DEEP = "!deep:x"
    const val LOOSE = "!loose:x"
    const val WITH_BOB = "!with-bob:x"
    const val OFFER = "!offer:x"
    const val WITH_CAROL = "!with-carol:x"
    const val PARTY = "!party:x"
    const val NOTES = "!notes:x"

    fun state(loaded: Boolean = true): RoomListState = RoomListState(
        rooms = listOf(
            summary(WITH_CAROL, RoomKind.DIRECT, invited = true, directUserId = "@carol:x"),
            summary(WITH_BOB, RoomKind.DIRECT, unread = 2, directUserId = "@bob:x"),
            summary(OFFER, invited = true),
            summary(LAB, unread = 1),
            summary(DEEP),
            summary(PARTY, invited = true),
            summary(NOTES, RoomKind.DIRECT, directUserId = FakeSession.USER_ID),
            summary(LOOSE),
            summary(SUB, RoomKind.SPACE),
            summary(OTHER, RoomKind.SPACE),
            summary(TEAM, RoomKind.SPACE),
        ),
        loaded = loaded,
        spaces = listOf(
            SpaceRooms(spaceId = OTHER, roomIds = listOf(NOTES)),
            SpaceRooms(spaceId = TEAM, roomIds = listOf(WITH_BOB, OFFER, LAB, DEEP, NOTES)),
        ),
        roomsOutsideSpaces = listOf(PARTY, LOOSE),
    )
}
