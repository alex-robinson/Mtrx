import Foundation
import MtrxFFI

/// A room's timeline while it is shown: its rows, loading older messages,
/// sending, and marking it read.
@MainActor
final class TimelineModel: ObservableObject {
    /// Older messages loaded per request.
    static let page: UInt16 = 40

    let roomId: String
    @Published private(set) var rows: [TimelineRow] = []
    /// Not opened yet.
    @Published private(set) var opening = true
    /// Back-pagination reached the room's beginning.
    @Published private(set) var reachedStart = false
    @Published private(set) var loadingOlder = false
    @Published var draft = ""
    @Published var error: String?

    private let session: Session
    private let showSenders: Bool
    private var timeline: Timeline?
    private let subscriptions = Subscriptions()
    /// The top of the list (the oldest row) is on screen.
    private var topVisible = false
    private var visible = false
    /// The newest item when the room was last marked read.
    private var readUpTo: String?

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
        markRead()
    }

    /// The timeline is on screen (or not): mark it read while it is.
    func setVisible(_ visible: Bool) {
        self.visible = visible
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
        guard let timeline, !loadingOlder, !reachedStart else { return }
        loadingOlder = true
        defer { loadingOlder = false }
        do {
            while topVisible && !reachedStart {
                if try await timeline.paginateBack(events: Self.page) {
                    reachedStart = true
                }
            }
        } catch {
            self.error = errorMessage(error)
        }
    }

    /// Send the draft; it shows in the timeline at once, and is sent in
    /// the background.
    func send() async {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let timeline, !text.isEmpty else { return }
        draft = ""
        do {
            try await timeline.sendText(text: text)
        } catch {
            if draft.isEmpty {
                draft = text
            }
            self.error = errorMessage(error)
        }
    }

    private func markRead() {
        guard visible, let timeline, let newest = rows.last?.id, newest != readUpTo else { return }
        readUpTo = newest
        Task {
            try? await timeline.markRead()
        }
    }
}
