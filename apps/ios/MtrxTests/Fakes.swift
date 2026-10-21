import AuthenticationServices
import Foundation
import MtrxFFI
import XCTest
@testable import Mtrx

// Fakes of the library's objects, made the way UniFFI provides for: the
// generated classes are open, and `init(noHandle:)` makes one without a
// Rust object behind it. Only overridden methods may be called.

final class FakeTaskHandle: TaskHandle {
    var cancelled = false
    override func cancel() { cancelled = true }
    override func isFinished() -> Bool { cancelled }
}

final class FakeTimeline: Timeline {
    var listener: TimelineListener?
    var sent: [String] = []
    var sendError: Error?
    /// What successive `paginateBack` calls return (then: the start).
    var pages: [Bool] = []
    var paginations = 0
    var markedRead = 0

    override func roomId() -> String { "!room:example.org" }

    override func subscribe(listener: TimelineListener) -> TaskHandle {
        self.listener = listener
        return FakeTaskHandle(noHandle: .init())
    }

    override func sendText(text: String) async throws {
        if let sendError { throw sendError }
        sent.append(text)
    }

    override func paginateBack(events: UInt16) async throws -> Bool {
        paginations += 1
        return pages.isEmpty ? true : pages.removeFirst()
    }

    override func markRead() async throws {
        markedRead += 1
    }
}

final class FakeSession: Session {
    let fakeTimeline = FakeTimeline(noHandle: .init())
    var roomListListener: RoomListListener?
    var connectionListener: ConnectionListener?

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
}

final class FakeOAuthLogin: OAuthLogin {
    var session: Session?
    var finishedWith: String?

    override func url() -> String { "https://auth.example.org/login" }

    override func finish(redirectUrl: String) async throws -> Session {
        finishedWith = redirectUrl
        guard let session else { throw MtrxError.LoginCancelled }
        return session
    }
}

final class FakeMtrx: Mtrx {
    var oauth = false
    var loginResult: Result<Session, Error> = .failure(MtrxError.Other(message: "Invalid password"))
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
        state: SendState = .sent
    ) -> TimelineEvent {
        TimelineEvent(
            eventId: nil, senderId: "@\(sender):example.org", senderName: sender.capitalized, senderAvatarUrl: nil,
            timestamp: start.addingTimeInterval(minutes * 60), own: own, content: content, sendState: state)
    }

    static func text(_ body: String) -> EventContent {
        .message(message: Message(kind: .text, body: body, edited: false, media: nil))
    }

    static func item(_ id: String, _ event: TimelineEvent) -> TimelineItem {
        TimelineItem(id: id, content: .event(event: event))
    }

    static func room(
        _ id: String, kind: RoomKind = .room, invited: Bool = false, name: String? = nil
    ) -> RoomSummary {
        RoomSummary(
            id: id, name: name ?? id, avatarUrl: nil, topic: nil, kind: kind, invited: invited, encrypted: false,
            unreadMessages: 0, unreadMentions: 0, unreadNotifications: 0, lastActivity: nil, directUserId: nil,
            alias: nil, joinedMembers: 2)
    }
}
