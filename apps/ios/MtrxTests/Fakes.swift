import AuthenticationServices
import Foundation
import MtrxFFI
import XCTest
@testable import Mtrx

// Fakes of the library's objects, made the way UniFFI provides for: the
// generated classes are open, and `init(noHandle:)` makes one without a
// Rust object behind it. Only overridden methods may be called. They hold
// unsynchronised state, used from the tests' main actor only.

final class FakeTaskHandle: TaskHandle, @unchecked Sendable {
    var cancelled = false
    override func cancel() { cancelled = true }
    override func isFinished() -> Bool { cancelled }
}

final class FakeTimeline: Timeline, @unchecked Sendable {
    var listener: TimelineListener?
    var typingListener: TypingListener?
    var typingSubscription: FakeTaskHandle?
    var sent: [String] = []
    var sendError: Error?
    /// What successive `paginateBack` calls return (then: the start).
    var pages: [Bool] = []
    var paginations = 0
    /// Called with the number of each `paginateBack` call (from 1), e.g. to
    /// give the listener older items.
    var onPaginate: ((Int) -> Void)?
    var markedRead = 0
    var replies: [(itemId: String, text: String)] = []
    var edits: [(itemId: String, text: String)] = []
    var reactions: [(itemId: String, key: String)] = []
    var deleted: [String] = []
    var retried: [String] = []
    var discarded: [String] = []
    var typing: [Bool] = []
    var actionError: Error?

    override func roomId() -> String { "!room:example.org" }

    override func subscribe(listener: TimelineListener) -> TaskHandle {
        self.listener = listener
        return FakeTaskHandle(noHandle: .init())
    }

    override func subscribeTyping(listener: TypingListener) -> TaskHandle {
        typingListener = listener
        let handle = FakeTaskHandle(noHandle: .init())
        typingSubscription = handle
        return handle
    }

    override func sendText(text: String) async throws {
        if let sendError { throw sendError }
        sent.append(text)
    }

    override func reply(itemId: String, text: String) async throws {
        if let sendError { throw sendError }
        replies.append((itemId, text))
    }

    override func edit(itemId: String, text: String) async throws {
        if let sendError { throw sendError }
        edits.append((itemId, text))
    }

    override func react(itemId: String, key: String) async throws {
        if let actionError { throw actionError }
        reactions.append((itemId, key))
    }

    override func delete(itemId: String) async throws {
        if let actionError { throw actionError }
        deleted.append(itemId)
    }

    override func retry(itemId: String) async throws {
        if let actionError { throw actionError }
        retried.append(itemId)
    }

    override func discard(itemId: String) async throws {
        if let actionError { throw actionError }
        discarded.append(itemId)
    }

    override func setTyping(typing: Bool) async throws {
        self.typing.append(typing)
    }

    override func paginateBack(events: UInt16) async throws -> Bool {
        paginations += 1
        onPaginate?(paginations)
        return pages.isEmpty ? true : pages.removeFirst()
    }

    override func markRead() async throws {
        markedRead += 1
    }
}

final class FakeVerification: Verification, @unchecked Sendable {
    var user = "@me:example.org"
    var initialState = VerificationState.waiting
    var listener: VerificationListener?
    var subscription: FakeTaskHandle?
    var accepted = 0
    var confirmed = 0
    var mismatched = 0
    var cancelled = 0
    var answerError: Error?

    override func otherUser() -> String { user }
    override func state() -> VerificationState { initialState }

    override func subscribe(listener: VerificationListener) -> TaskHandle {
        self.listener = listener
        let handle = FakeTaskHandle(noHandle: .init())
        subscription = handle
        return handle
    }

    override func accept() async throws {
        if let answerError { throw answerError }
        accepted += 1
    }

    override func confirm() async throws {
        if let answerError { throw answerError }
        confirmed += 1
    }

    override func mismatch() async throws {
        mismatched += 1
    }

    override func cancel() async throws {
        cancelled += 1
    }
}

final class FakeSession: Session, @unchecked Sendable {
    let fakeTimeline = FakeTimeline(noHandle: .init())
    var roomListListener: RoomListListener?
    var connectionListener: ConnectionListener?
    var verificationRequestListener: VerificationRequestListener?
    var syncStarts = 0
    var encryption = EncryptionState(deviceVerified: false, recovery: .unknown)
    var encryptionRequests = 0
    var deviceList: [DeviceInfo] = []
    /// What `recover` reports, then its outcome.
    var recoverySteps: [RecoveryStep] = []
    var recoverError: Error?
    var recoveredWith: String?
    var selfVerification = FakeVerification(noHandle: .init())

    override func userId() -> String { "@me:example.org" }
    override func deviceId() -> String { "DEVICE" }
    override func homeserver() -> String { "https://example.org" }

    override func timeline(roomId: String) async throws -> Timeline {
        fakeTimeline
    }

    override func subscribeRoomList(listener: RoomListListener) async throws -> TaskHandle {
        roomListListener = listener
        return FakeTaskHandle(noHandle: .init())
    }

    override func subscribeConnection(listener: ConnectionListener) -> TaskHandle {
        connectionListener = listener
        return FakeTaskHandle(noHandle: .init())
    }

    override func subscribeVerificationRequests(listener: VerificationRequestListener) -> TaskHandle {
        verificationRequestListener = listener
        return FakeTaskHandle(noHandle: .init())
    }

    override func startSync() async {
        syncStarts += 1
    }

    override func encryptionState() async throws -> EncryptionState {
        encryptionRequests += 1
        return encryption
    }

    override func devices() async throws -> [DeviceInfo] { deviceList }

    override func recover(recoveryKey: String, listener: RecoveryListener) async throws {
        recoveredWith = recoveryKey
        for step in recoverySteps {
            listener.onUpdate(step: step)
        }
        if let recoverError { throw recoverError }
    }

    override func requestSelfVerification() async throws -> Verification {
        selfVerification
    }

    override func logout() async throws {}

    override func forget() async throws {}
}

final class FakeOAuthLogin: OAuthLogin, @unchecked Sendable {
    var session: Session?
    var finishedWith: String?

    override func url() -> String { "https://auth.example.org/login" }

    override func finish(redirectUrl: String) async throws -> Session {
        finishedWith = redirectUrl
        guard let session else { throw MtrxError.LoginCancelled }
        return session
    }
}

final class FakeMtrx: Mtrx, @unchecked Sendable {
    var oauth = false
    var loginResult: Result<Session, Error> = .failure(MtrxError.Other(reason: "Invalid password"))
    let oauthLogin = FakeOAuthLogin(noHandle: .init())

    override func supportsOauth(homeserver: String) async throws -> Bool { oauth }

    override func login(homeserver: String, user: String, password: String) async throws -> Session {
        try loginResult.get()
    }

    override func startOauthLogin(homeserver: String) async throws -> OAuthLogin {
        oauthLogin
    }
}

@MainActor
final class FakeBrowser: BrowserSignIn {
    var result: Result<URL, Error> = .failure(ASWebAuthenticationSessionError(.canceledLogin))
    var opened: URL?

    func signIn(at url: URL, callbackScheme: String) async throws -> URL {
        opened = url
        return try result.get()
    }
}

/// Wait for `condition` (e.g. a listener's update reaching the main actor).
@MainActor
func eventually(_ condition: @MainActor () -> Bool, file: StaticString = #filePath, line: UInt = #line) async {
    for _ in 0..<200 {
        if condition() { return }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    XCTFail("Condition not met in time", file: file, line: line)
}

enum Make {
    static let start = Date(timeIntervalSince1970: 1_700_000_000)

    static func event(
        _ sender: String, _ content: EventContent, at minutes: Double = 0, own: Bool = false,
        state: SendState = .sent, eventId: String? = nil, editable: Bool = false, readBy: [Reader] = []
    ) -> TimelineEvent {
        TimelineEvent(
            eventId: eventId, senderId: "@\(sender):example.org", senderName: sender.capitalized,
            senderAvatarUrl: nil, timestamp: start.addingTimeInterval(minutes * 60), own: own, content: content,
            sendState: state, editable: editable, readBy: readBy)
    }

    static func text(
        _ body: String, edited: Bool = false, reactions: [Reaction] = [], replyTo: ReplyQuote? = nil,
        rawBody: String = ""
    ) -> EventContent {
        .message(
            message: Message(
                kind: .text, body: body, edited: edited, media: nil, reactions: reactions, replyTo: replyTo,
                rawBody: rawBody))
    }

    static func reader(_ name: String, named: Bool = true) -> Reader {
        Reader(userId: "@\(name):example.org", name: named ? name.capitalized : nil, avatarUrl: nil)
    }

    static func item(_ id: String, _ event: TimelineEvent) -> TimelineItem {
        TimelineItem(id: id, content: .event(event: event))
    }

    static func room(
        _ id: String, kind: RoomKind = .room, invited: Bool = false, name: String? = nil, unread: UInt64 = 0,
        directUser: String? = nil, lastActivity: Date? = nil
    ) -> RoomSummary {
        RoomSummary(
            id: id, name: name ?? id, avatarUrl: nil, topic: nil, kind: kind, invited: invited, encrypted: false,
            unreadMessages: unread, unreadMentions: 0, unreadNotifications: 0, lastActivity: lastActivity,
            directUserId: directUser, alias: nil, joinedMembers: 2)
    }

    /// The rooms by space as the library's integration test makes them, most
    /// recently active first: spaces Team and Other; Lab in Team, Deep in
    /// Team's nested space Sub; Loose in no space; a direct chat with Bob, a
    /// member of Team (unread); invites to Offer, in Team, and from Carol,
    /// in no space, to a direct chat and Party. Loose is unread.
    static func roomsBySpace(loaded: Bool = true) -> RoomListState {
        RoomListState(
            rooms: [
                room("!carol", kind: .direct, invited: true, name: "Carol", directUser: "@carol:example.org"),
                room("!party", invited: true, name: "Party"),
                room("!offer", invited: true, name: "Offer"),
                room("!bob", kind: .direct, name: "Bob", unread: 2, directUser: "@bob:example.org"),
                room("!loose", name: "Loose", unread: 1),
                room("!deep", name: "Deep"),
                room("!sub", kind: .space, name: "Sub"),
                room("!lab", name: "Lab"),
                room("!team", kind: .space, name: "Team"),
                room("!other", kind: .space, name: "Other"),
            ],
            loaded: loaded,
            spaces: [
                SpaceRooms(spaceId: "!team", roomIds: ["!offer", "!bob", "!deep", "!lab"]),
                SpaceRooms(spaceId: "!other", roomIds: []),
            ],
            roomsOutsideSpaces: ["!party", "!loose"])
    }
}
