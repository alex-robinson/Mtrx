#if DEBUG
import Foundation
import MtrxFFI

/// For the UI tests (launch argument `-ui-testing`, debug builds only): an
/// account signed in with made-up rooms and messages, and no server. A
/// message sent shows at once, then as the server's copy; one deleted
/// shows as deleted. The library's classes are faked as UniFFI provides
/// for (`init(noHandle:)`, overriding what the app calls).
enum UITesting {
    static var on: Bool {
        ProcessInfo.processInfo.arguments.contains("-ui-testing")
    }

    @MainActor
    static func app() -> AppModel {
        AppModel(core: UITestCore(noHandle: .init()), browser: WebAuthenticator())
    }

    static let lab = "!lab:example.org"
}

private final class UITestCore: Mtrx, @unchecked Sendable {
    override func restore() async throws -> Session? {
        UITestSession(noHandle: .init())
    }
}

private final class UITestHandle: TaskHandle, @unchecked Sendable {
    override func cancel() {}
    override func isFinished() -> Bool { false }
}

private final class UITestSession: Session, @unchecked Sendable {
    private let lab = UITestTimeline(noHandle: .init())
    private let lock = NSLock()
    private var drafts: [String: Draft] = [:]

    override func userId() -> String { "@me:example.org" }
    override func deviceId() -> String { "UITEST" }
    override func homeserver() -> String { "https://example.org" }

    override func subscribeConnection(listener: ConnectionListener) -> TaskHandle {
        listener.onUpdate(state: .online)
        return UITestHandle(noHandle: .init())
    }

    override func subscribeEncryptionState(listener: EncryptionStateListener) -> TaskHandle {
        listener.onUpdate(state: EncryptionState(deviceVerified: true, recovery: .enabled))
        return UITestHandle(noHandle: .init())
    }

    override func encryptionState() async throws -> EncryptionState {
        EncryptionState(deviceVerified: true, recovery: .enabled)
    }

    override func subscribeVerificationRequests(listener: VerificationRequestListener) -> TaskHandle {
        UITestHandle(noHandle: .init())
    }

    override func subscribeRoomList(listener: RoomListListener) async throws -> TaskHandle {
        let lab = RoomSummary(
            id: UITesting.lab, name: "Lab", avatarUrl: nil, topic: nil, kind: .room, invited: false,
            encrypted: false, unreadMessages: 0, unreadMentions: 0, unreadNotifications: 0, lastActivity: Date(),
            directUserId: nil, alias: nil, joinedMembers: 3, parents: [], public: false, pinned: [])
        listener.onUpdate(
            state: RoomListState(rooms: [lab], loaded: true, spaces: [], roomsOutsideSpaces: [UITesting.lab]))
        return UITestHandle(noHandle: .init())
    }

    override func startSync() async {}
    override func stopSync() async {}
    override func setPresence(state: PresenceState, statusMessage: String?) async throws {}

    override func presence(userId: String) async throws -> Presence {
        throw MtrxError.Other(reason: "No presence here")
    }

    override func subscribePresence(userIds: [String], listener: PresenceListener) throws -> TaskHandle {
        UITestHandle(noHandle: .init())
    }

    override func devices() async throws -> [DeviceInfo] { [] }

    override func timeline(roomId: String) async throws -> Timeline { lab }

    override func canPin(roomId: String) async throws -> Bool { false }

    override func roomSettings(roomId: String) async throws -> RoomSettings {
        throw MtrxError.Other(reason: "No settings here")
    }

    override func members(roomId: String) async throws -> [Member] { [] }

    override func draft(roomId: String) async throws -> Draft? {
        lock.withLock { drafts[roomId] }
    }

    override func saveDraft(roomId: String, draft: Draft) async throws {
        lock.withLock { drafts[roomId] = draft }
    }
}

/// The Lab room's messages: Ann's and ours, the last two not read yet, and
/// last one of ours to delete.
private final class UITestTimeline: Timeline, @unchecked Sendable {
    private let lock = NSLock()
    private var items = UITestTimeline.history()
    private var listener: TimelineListener?
    private var sent = 0

    private static func history() -> [TimelineItem] {
        let start = Date().addingTimeInterval(-3_600)
        var items = [TimelineItem(id: "day", content: .dayDivider(date: start))]
        for i in 0..<30 {
            if i == 28 {
                items.append(TimelineItem(id: "read-marker", content: .readMarker))
            }
            let text = Array(repeating: "Message \(i), a line of it.", count: 1 + i % 4).joined(separator: "\n")
            items.append(message("m\(i)", text, own: i % 3 == 0, at: start.addingTimeInterval(Double(i) * 60)))
        }
        items.append(message("mine", "Mine, to delete", own: true, at: start.addingTimeInterval(1_900)))
        return items
    }

    private static func message(_ id: String, _ text: String, own: Bool, at: Date, sent: Bool = true) -> TimelineItem {
        let message = Message(
            kind: .text, body: text, edited: false, media: nil, reactions: [], replyTo: nil, rawBody: text)
        let event = TimelineEvent(
            eventId: sent ? "$\(id)" : nil, senderId: own ? "@me:example.org" : "@ann:example.org",
            senderName: own ? "Me" : "Ann", senderAvatarUrl: nil, timestamp: at, own: own,
            content: .message(message: message), sendState: sent ? .sent : .sending, editable: own)
        return TimelineItem(id: id, content: .event(event: event))
    }

    /// Change the items and give them to the listener.
    private func change(_ change: (inout [TimelineItem]) -> Void) {
        let (items, listener) = lock.withLock {
            change(&self.items)
            return (self.items, self.listener)
        }
        listener?.onUpdate(items: items)
    }

    override func roomId() -> String { UITesting.lab }

    override func subscribe(listener: TimelineListener) -> TaskHandle {
        let items = lock.withLock {
            self.listener = listener
            return self.items
        }
        listener.onUpdate(items: items)
        return UITestHandle(noHandle: .init())
    }

    override func subscribeTyping(listener: TypingListener) -> TaskHandle {
        UITestHandle(noHandle: .init())
    }

    /// Shown at once; read (the marker gone) and the server's copy a moment
    /// later.
    override func sendText(text: String) async throws {
        let id = lock.withLock {
            sent += 1
            return "sent\(sent)"
        }
        change { $0.append(Self.message(id, text, own: true, at: Date(), sent: false)) }
        try? await Task.sleep(nanoseconds: 300_000_000)
        change { items in
            items.removeAll { $0.id == "read-marker" }
            if let at = items.firstIndex(where: { $0.id == id }) {
                items[at] = Self.message(id, text, own: true, at: Date())
            }
        }
    }

    override func delete(itemId: String) async throws {
        change { items in
            guard let at = items.firstIndex(where: { $0.id == itemId }),
                case .event(var event) = items[at].content
            else { return }
            event.content = .redacted
            event.editable = false
            items[at] = TimelineItem(id: itemId, content: .event(event: event))
        }
    }

    override func react(itemId: String, key: String) async throws {}
    override func setTyping(typing: Bool) async throws {}
    override func markRead() async throws {}
    override func paginateBack(events: UInt16) async throws -> Bool { true }
}
#endif
