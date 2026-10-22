import Foundation
import MtrxFFI

/// A room's timeline while it is shown: its rows, loading older messages,
/// sending, replying, editing and acting on messages, who types, and
/// marking it read.
@MainActor
final class TimelineModel: ObservableObject {
    /// Older messages loaded per request.
    static let page: UInt16 = 40
    /// Older events loaded per request while looking for a quoted message,
    /// and at most how many requests.
    static let seekPage: UInt16 = 50
    static let seekPages = 20
    static let notFound = "The original message was not found in the room's history"
    /// How long a message gone to stays marked.
    static let highlightTime: UInt64 = 2_000_000_000

    let roomId: String
    @Published private(set) var rows: [TimelineRow] = []
    /// Not opened yet.
    @Published private(set) var opening = true
    /// Back-pagination reached the room's beginning.
    @Published private(set) var reachedStart = false
    @Published private(set) var loadingOlder = false
    @Published var draft = "" {
        didSet {
            if draft != oldValue {
                setTyping(!draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
    }
    /// What sending the draft does instead of sending a new message.
    @Published private(set) var compose: Compose?
    /// The names of the others typing.
    @Published private(set) var typing: [String] = []
    /// Looking for a quoted message in older events.
    @Published private(set) var seeking = false
    /// The row gone to, marked for a moment.
    @Published private(set) var highlighted: String?
    /// The row to scroll to: a new value for each request.
    @Published private(set) var scrollTarget: ScrollTarget?
    @Published var error: String?

    private let session: Session
    private let showSenders: Bool
    private var timeline: Timeline?
    private let subscriptions = Subscriptions()
    /// Who types, while the room is on screen.
    private let typingSubscription = Subscriptions()
    private var watchingTyping = false
    /// The top of the list (the oldest row) is on screen.
    private var topVisible = false
    private var visible = false
    /// The newest item when the room was last marked read.
    private var readUpTo: String?
    /// The draft before an edit took the field; back after it.
    private var savedDraft: String?
    /// The last `setTyping` call: each waits for the one before.
    private var typingCall: Task<Void, Never>?
    /// The last back-pagination: each waits for the one before.
    private var pagination: Task<Bool, Error>?
    private var seek: Seek?

    init(session: Session, room: RoomSummary) {
        self.session = session
        roomId = room.id
        showSenders = room.kind != .direct
    }

    func open() async {
        guard timeline == nil else { return }
        do {
            let timeline = try await session.timeline(roomId: roomId)
            self.timeline = timeline
            let feed = Feed<[TimelineItem]>()
            subscriptions.add(timeline.subscribe(listener: feed), feed) { [weak self] items in
                self?.apply(items)
            }
            opening = false
            watchTyping()
            if topVisible {
                await loadOlder()
            }
        } catch {
            opening = false
            self.error = errorMessage(error)
        }
    }

    func apply(_ items: [TimelineItem]) {
        rows = TimelineRows.build(items, showSenders: showSenders)
        if items.contains(where: { $0.content == .start }) {
            reachedStart = true
        }
        checkSeek()
        markRead()
    }

    /// The timeline is on screen (or not): mark it read and show who types
    /// while it is; leaving it, we stop typing.
    func setVisible(_ visible: Bool) {
        self.visible = visible
        if visible {
            watchTyping()
        } else {
            typingSubscription.cancelAll()
            watchingTyping = false
            typing = []
            setTyping(false)
        }
        markRead()
    }

    /// The oldest row is on screen (or not): load older ones while it is.
    func setTopVisible(_ visible: Bool) async {
        topVisible = visible
        if visible {
            await loadOlder()
        }
    }

    /// Load older messages until the top of the list is off screen or the
    /// room's start is reached.
    func loadOlder() async {
        guard timeline != nil, !loadingOlder, !reachedStart else { return }
        loadingOlder = true
        defer { loadingOlder = false }
        do {
            while topVisible && !reachedStart {
                _ = try await paginate(Self.page)
            }
        } catch {
            self.error = errorMessage(error)
        }
    }

    // MARK: Composing

    /// Send the draft: a new message, a reply or an edit, as `compose`
    /// says. It shows in the timeline at once, and is sent in the
    /// background.
    func send() async {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let timeline, !text.isEmpty else { return }
        let compose = self.compose
        // After an edit, the draft from before it is back.
        endCompose()
        if case .edit = compose {} else {
            draft = ""
        }
        let left = draft
        do {
            switch compose {
            case nil:
                try await timeline.sendText(text: text)
            case .reply(let target):
                try await timeline.reply(itemId: target.itemId, text: text)
            case .edit(let target):
                try await timeline.edit(itemId: target.itemId, text: text)
            }
        } catch {
            // Back as it was, unless something else was begun meanwhile.
            if self.compose == nil && draft == left {
                if case .edit = compose {
                    savedDraft = left
                }
                draft = text
                self.compose = compose
            }
            self.error = errorMessage(error)
        }
    }

    /// Answer row `itemId`'s message with the next one sent.
    func startReply(to itemId: String) {
        guard let row = messageRow(itemId) else { return }
        if case .edit = compose {
            endCompose()
        }
        compose = .reply(ComposeTarget(itemId: itemId, row: row))
    }

    /// Change row `itemId`'s message (ours): the field holds its text as we
    /// wrote it.
    func startEdit(_ itemId: String) {
        guard let row = messageRow(itemId), let message = row.message else { return }
        if case .edit = compose {} else {
            savedDraft = draft
        }
        compose = .edit(ComposeTarget(itemId: itemId, row: row))
        draft = message.rawBody.isEmpty ? message.body : message.rawBody
    }

    /// Stop replying or editing; after an edit, the draft from before is
    /// back.
    func cancelCompose() {
        endCompose()
    }

    private func endCompose() {
        if case .edit = compose {
            draft = savedDraft ?? ""
            savedDraft = nil
        }
        compose = nil
    }

    /// Tell the others whether we type (the library sends at most a notice
    /// every few seconds).
    private func setTyping(_ typing: Bool) {
        guard let timeline else { return }
        let previous = typingCall
        typingCall = Task {
            await previous?.value
            try? await timeline.setTyping(typing: typing)
        }
    }

    private func watchTyping() {
        guard visible, !watchingTyping, let timeline else { return }
        watchingTyping = true
        let feed = Feed<[String]>()
        typingSubscription.add(timeline.subscribeTyping(listener: feed), feed) { [weak self] names in
            self?.typing = names
        }
    }

    /// One line saying who types, or nil if nobody does.
    static func typingLine(_ names: [String]) -> String? {
        switch names.count {
        case 0: return nil
        case 1: return "\(names[0]) is typing…"
        case 2: return "\(names[0]) and \(names[1]) are typing…"
        case 3: return "\(names[0]), \(names[1]) and \(names[2]) are typing…"
        default: return "\(names[0]), \(names[1]) and \(names.count - 2) others are typing…"
        }
    }

    // MARK: Acting on messages

    /// Add our reaction `key` to row `itemId`'s message, or remove it.
    @discardableResult
    func react(_ itemId: String, key: String) -> Task<Void, Never> {
        act { try await $0.react(itemId: itemId, key: key) }
    }

    /// Delete row `itemId`'s message (ours) for everyone.
    @discardableResult
    func delete(_ itemId: String) -> Task<Void, Never> {
        act { try await $0.delete(itemId: itemId) }
    }

    /// Send row `itemId`'s refused message again.
    @discardableResult
    func retry(_ itemId: String) -> Task<Void, Never> {
        act { try await $0.retry(itemId: itemId) }
    }

    /// Drop row `itemId`'s refused message.
    @discardableResult
    func discard(_ itemId: String) -> Task<Void, Never> {
        act { try await $0.discard(itemId: itemId) }
    }

    /// Run `action` in a task of the model's (leaving the room does not
    /// cancel it); a failure shows as `error`.
    private func act(_ action: @escaping (Timeline) async throws -> Void) -> Task<Void, Never> {
        guard let timeline else { return Task {} }
        return Task {
            do {
                try await action(timeline)
            } catch {
                self.error = errorMessage(error)
            }
        }
    }

    func messageRow(_ itemId: String) -> MessageRow? {
        for row in rows where row.id == itemId {
            if case .message(let message) = row.content { return message }
        }
        return nil
    }

    // MARK: Going to a quoted message

    /// Scroll to the message with `eventId` and mark it, loading older
    /// events until it is there (at most `seekPages` times, or to the
    /// room's start).
    func goTo(eventId: String) async {
        guard timeline != nil else { return }
        if reveal(eventId) { return }
        guard seek == nil else { return }
        if startLoaded {
            error = Self.notFound
            return
        }
        seek = Seek(eventId: eventId)
        seeking = true
        do {
            for _ in 0..<Self.seekPages {
                let atStart = try await paginate(Self.seekPage)
                // The items arrive apart from the call: `apply` may have
                // found it meanwhile.
                guard seek?.eventId == eventId else { return }
                if atStart {
                    // Decided once the items with the start are here.
                    seek?.atStart = true
                    checkSeek()
                    return
                }
            }
            if seek?.eventId == eventId {
                endSeek(error: reveal(eventId) ? nil : Self.notFound)
            }
        } catch {
            endSeek(error: errorMessage(error))
        }
    }

    /// The message looked for is in the rows (scrolled to), or cannot be.
    private func checkSeek() {
        guard let seek else { return }
        if reveal(seek.eventId) {
            endSeek(error: nil)
        } else if seek.atStart && startLoaded {
            endSeek(error: Self.notFound)
        }
    }

    /// Stop looking, found or not (`error`).
    private func endSeek(error: String?) {
        seek = nil
        seeking = false
        if let error {
            self.error = error
        }
    }

    /// The room's start is among the rows.
    private var startLoaded: Bool {
        rows.first?.content == .start
    }

    /// Scroll to the row of the message with `eventId` and mark it, if it
    /// is there.
    private func reveal(_ eventId: String) -> Bool {
        let row = rows.first { row in
            if case .message(let message) = row.content { return message.event.eventId == eventId }
            return false
        }
        guard let row else { return false }
        scrollTarget = ScrollTarget(rowId: row.id, serial: (scrollTarget?.serial ?? 0) + 1)
        highlighted = row.id
        Task {
            try? await Task.sleep(nanoseconds: Self.highlightTime)
            if highlighted == row.id {
                highlighted = nil
            }
        }
        return true
    }

    /// Load `events` older events, after any loading already asked for.
    /// Returns whether the room's start is reached.
    private func paginate(_ events: UInt16) async throws -> Bool {
        guard let timeline else { return true }
        let previous = pagination
        let call = Task {
            _ = try? await previous?.value
            return try await timeline.paginateBack(events: events)
        }
        pagination = call
        let atStart = try await call.value
        if atStart {
            reachedStart = true
        }
        return atStart
    }

    private func markRead() {
        guard visible, let timeline, let newest = rows.last?.id, newest != readUpTo else { return }
        readUpTo = newest
        Task {
            try? await timeline.markRead()
        }
    }
}

extension TimelineModel {
    /// Replying to a message, or editing one of ours.
    enum Compose: Equatable {
        case reply(ComposeTarget)
        case edit(ComposeTarget)

        var target: ComposeTarget {
            switch self {
            case .reply(let target), .edit(let target): return target
            }
        }
    }

    struct ScrollTarget: Equatable {
        let rowId: String
        let serial: Int
    }

    private struct Seek {
        let eventId: String
        /// Back-pagination reached the room's start.
        var atStart = false
    }
}

/// The message replied to or edited, as the composer's bar shows it.
struct ComposeTarget: Equatable {
    let itemId: String
    let sender: String
    /// Its first line.
    let preview: String

    init(itemId: String, sender: String, preview: String) {
        self.itemId = itemId
        self.sender = sender
        self.preview = preview
    }

    init(itemId: String, row: MessageRow) {
        self.init(itemId: itemId, sender: row.event.displayName, preview: row.message?.summary ?? "")
    }
}

extension Message {
    /// Its first line, or for a file its kind and name.
    var summary: String {
        let line = body.split(separator: "\n", maxSplits: 1).first.map(String.init) ?? ""
        guard media != nil, let label = kind.fileLabel else { return line }
        return line.isEmpty ? label : "\(label): \(line)"
    }
}
