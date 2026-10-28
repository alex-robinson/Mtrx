import MtrxFFI
import XCTest
@testable import Mtrx

/// A room's draft kept when leaving it, our reactions shown at once, and
/// pinning messages.
@MainActor
final class TimelineChangesTests: XCTestCase {
    private var session: FakeSession!
    private var drafts: Drafts!
    private var model: TimelineModel!

    override func setUp() async throws {
        session = FakeSession(noHandle: .init())
        drafts = Drafts()
        model = open(Make.room("!room:example.org"))
        await model.open()
    }

    private var timeline: FakeTimeline { session.fakeTimeline }

    private func open(_ room: RoomSummary) -> TimelineModel {
        TimelineModel(session: session, room: room, keysArriving: Make.noKeysArriving, drafts: drafts)
    }

    private func show(_ items: [TimelineItem]) async {
        timeline.listener?.onUpdate(items: items)
        await eventually { self.model.rows.count == TimelineRows.build(items, showSenders: true).count }
    }

    private func bobsMessage(reactions: [Reaction] = []) -> TimelineItem {
        Make.item("b", Make.event("bob", Make.text("Lunch?", reactions: reactions), eventId: "$b"))
    }

    private func reactions(_ itemId: String = "b") -> [Reaction] {
        model.messageRow(itemId)?.reactions ?? []
    }

    // MARK: Drafts

    func testTheDraftStaysWithItsRoom() {
        model.draft = "Half written"
        let other = open(Make.room("!other:example.org"))
        XCTAssertEqual(other.draft, "")
        other.draft = "Elsewhere"
        XCTAssertEqual(open(Make.room("!room:example.org")).draft, "Half written")
        XCTAssertEqual(open(Make.room("!other:example.org")).draft, "Elsewhere")
    }

    func testASentDraftIsGone() async {
        model.draft = "Hello"
        await model.send()
        XCTAssertEqual(open(Make.room("!room:example.org")).draft, "")
    }

    func testWhileEditingTheDraftFromBeforeIsKept() async {
        await show([Make.item("o", Make.event("me", Make.text("Mine"), own: true, eventId: "$o", editable: true))])
        model.draft = "Half written"
        model.startEdit("o")
        model.draft = "Mine, changed"
        XCTAssertEqual(open(Make.room("!room:example.org")).draft, "Half written")
    }

    // MARK: Reactions

    func testOurReactionShowsAtOnce() async {
        await show([bobsMessage()])
        let task = model.react("b", key: "👍")
        XCTAssertEqual(reactions(), [Reaction(key: "👍", count: 1, byUs: true, senders: [])])
        await task.value
        XCTAssertEqual(timeline.reactions.map(\.key), ["👍"])
        // The library's own, once it has it.
        let alice = Make.reader("alice")
        await show([bobsMessage(reactions: [Reaction(key: "👍", count: 2, byUs: true, senders: [alice])])])
        XCTAssertEqual(reactions(), [Reaction(key: "👍", count: 2, byUs: true, senders: [alice])])
    }

    func testTakingOursAwayShowsAtOnce() async {
        await show([bobsMessage(reactions: [Reaction(key: "👍", count: 2, byUs: true, senders: [])])])
        model.react("b", key: "👍")
        XCTAssertEqual(reactions(), [Reaction(key: "👍", count: 1, byUs: false, senders: [])])
        // Until the library shows it, its stale state is not shown.
        await show([bobsMessage(reactions: [Reaction(key: "👍", count: 2, byUs: true, senders: [])])])
        XCTAssertEqual(reactions().first?.byUs, false)
    }

    func testAFailedReactionIsTakenBack() async {
        await show([bobsMessage()])
        timeline.actionError = MtrxError.Other(reason: "No connection")
        await model.react("b", key: "🎉").value
        XCTAssertEqual(reactions(), [])
        XCTAssertEqual(model.error, "No connection")
    }

    func testReactions() {
        let ours = Reaction(key: "👍", count: 1, byUs: true, senders: [])
        let theirs = Reaction(key: "🎉", count: 3, byUs: false, senders: [])
        XCTAssertEqual(Reactions.setting("👍", ours: false, in: [ours, theirs]), [theirs])
        XCTAssertEqual(
            Reactions.setting("🎉", ours: true, in: [theirs]),
            [Reaction(key: "🎉", count: 4, byUs: true, senders: [])])
        XCTAssertEqual(Reactions.setting("👍", ours: true, in: [ours]), [ours])
    }

    // MARK: Pins

    func testPinningWhereWeMay() async {
        await show([bobsMessage()])
        guard let row = model.messageRow("b") else { return XCTFail("no row") }
        XCTAssertTrue(model.canPin)
        XCTAssertEqual(model.actions(for: row), [.react, .reply, .copy, .selectText, .pin])
        await model.togglePin("b").value
        XCTAssertEqual(session.changes, ["pin $b true"])
        XCTAssertEqual(model.actions(for: row).last, .unpin)
        await model.togglePin("b").value
        XCTAssertEqual(session.changes, ["pin $b true", "pin $b false"])
    }

    func testPinnedAsTheRoomListSays() async {
        let pinned = open(Make.room("!room:example.org", pinned: ["$b"]))
        await pinned.open()
        timeline.listener?.onUpdate(items: [bobsMessage()])
        await eventually { pinned.messageRow("b") != nil }
        XCTAssertEqual(pinned.actions(for: pinned.messageRow("b")!).last, .unpin)
        pinned.setPinned([])
        XCTAssertEqual(pinned.actions(for: pinned.messageRow("b")!).last, .pin)
    }

    func testNoPinsWhereWeMayNot() async {
        session.canPinResult = false
        let model = open(Make.room("!room:example.org"))
        await model.open()
        timeline.listener?.onUpdate(items: [bobsMessage()])
        await eventually { model.messageRow("b") != nil }
        XCTAssertFalse(model.actions(for: model.messageRow("b")!).contains(.pin))
    }

    func testAFailedPinIsTakenBack() async {
        await show([bobsMessage()])
        session.changeError = MtrxError.Other(reason: "Not allowed")
        await model.togglePin("b").value
        XCTAssertEqual(model.actions(for: model.messageRow("b")!).last, .pin)
        XCTAssertEqual(model.error, "Not allowed")
    }
}
