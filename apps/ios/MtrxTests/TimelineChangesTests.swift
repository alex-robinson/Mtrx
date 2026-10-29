import MtrxFFI
import XCTest
@testable import Mtrx

/// A room's draft kept on this device and back when it opens, our
/// reactions shown at once, and pinning messages.
@MainActor
final class TimelineChangesTests: XCTestCase {
    private var session: FakeSession!
    private var model: TimelineModel!
    private let roomId = "!room:example.org"

    override func setUp() async throws {
        session = FakeSession(noHandle: .init())
        model = open(Make.room(roomId))
        await model.open()
    }

    private var timeline: FakeTimeline { session.fakeTimeline }

    /// Saved a minute after typing stops, unless said otherwise.
    private func open(_ room: RoomSummary, draftDelay: TimeInterval = 60) -> TimelineModel {
        TimelineModel(session: session, room: room, keysArriving: Make.noKeysArriving, draftDelay: draftDelay)
    }

    /// The room opened again, with its draft.
    private func reopen(_ id: String = "!room:example.org") async -> TimelineModel {
        let model = open(Make.room(id))
        await model.open()
        return model
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

    func testTheDraftStaysWithItsRoom() async {
        model.draft = "Half written"
        model.setVisible(false)
        let other = await reopen("!other:example.org")
        XCTAssertEqual(other.draft, "")
        other.draft = "Elsewhere"
        other.setVisible(false)
        await eventually { self.session.drafts.count == 2 }
        let back = await reopen()
        XCTAssertEqual(back.draft, "Half written")
        XCTAssertNil(back.compose)
        let otherBack = await reopen("!other:example.org")
        XCTAssertEqual(otherBack.draft, "Elsewhere")
    }

    func testSavedOnceTypingStops() async {
        let model = open(Make.room("!typed:example.org"), draftDelay: 0.05)
        await model.open()
        model.draft = "H"
        model.draft = "Hi"
        await eventually { self.session.drafts["!typed:example.org"] == Draft(text: "Hi") }
        try? await Task.sleep(nanoseconds: 100_000_000)
        XCTAssertEqual(session.savedDrafts.map(\.draft.text), ["Hi"])
    }

    func testSavedWhenTheAppLeavesTheForeground() async {
        model.draft = "Half written"
        model.leftForeground()
        await eventually { self.session.drafts[self.roomId] == Draft(text: "Half written") }
    }

    func testUnchangedIsNotSavedAgain() async {
        session.drafts[roomId] = Draft(text: "Half written")
        let model = await reopen()
        await model.saveDraft().value
        model.setVisible(false)
        await model.saveDraft().value
        XCTAssertTrue(session.savedDrafts.isEmpty)
    }

    func testNothingSavedBeforeTheDraftIsRead() async {
        session.drafts[roomId] = Draft(text: "Half written")
        let model = open(Make.room(roomId))
        model.draft = "Other"
        await model.saveDraft().value
        XCTAssertTrue(session.savedDrafts.isEmpty)
        // What was written meanwhile stays.
        await model.open()
        XCTAssertEqual(model.draft, "Other")
    }

    func testBringingTheDraftBackIsNotTyping() async {
        session.drafts[roomId] = Draft(text: "Half written")
        let model = await reopen()
        model.setVisible(true)
        XCTAssertEqual(model.draft, "Half written")
        try? await Task.sleep(nanoseconds: 50_000_000)
        XCTAssertEqual(timeline.typing, [])
    }

    func testTheReplyIsKeptWithTheDraft() async {
        await show([bobsMessage()])
        model.startReply(to: "b")
        model.draft = "Sure"
        model.setVisible(false)
        await eventually { self.session.drafts[self.roomId] == Draft(text: "Sure", replyTo: "$b") }
        timeline.initialItems = [bobsMessage()]
        let back = await reopen()
        await eventually { back.compose != nil }
        XCTAssertEqual(back.compose, .reply(ComposeTarget(itemId: "b", eventId: "$b", sender: "Bob", preview: "Lunch?")))
        XCTAssertEqual(back.draft, "Sure")
    }

    func testTheReplyIsBackOnceItsMessageLoads() async {
        session.drafts[roomId] = Draft(text: "Sure", replyTo: "$b")
        let back = await reopen()
        XCTAssertEqual(back.draft, "Sure")
        XCTAssertNil(back.compose)
        timeline.listener?.onUpdate(items: [bobsMessage()])
        await eventually { back.compose?.target.eventId == "$b" }
    }

    func testAReplyNotFoundIsSentAsANewMessage() async {
        session.drafts[roomId] = Draft(text: "Sure", replyTo: "$gone")
        let back = await reopen()
        XCTAssertNil(back.compose)
        await back.send()
        XCTAssertEqual(timeline.sent, ["Sure"])
        XCTAssertEqual(timeline.replies.count, 0)
        await eventually { self.session.drafts[self.roomId] == nil }
    }

    func testASentDraftIsCleared() async {
        model.draft = "Hello"
        await model.saveDraft().value
        XCTAssertEqual(session.drafts[roomId], Draft(text: "Hello"))
        await model.send()
        await eventually { self.session.drafts[self.roomId] == nil }
        let back = await reopen()
        XCTAssertEqual(back.draft, "")
    }

    func testWhileEditingTheDraftFromBeforeIsKept() async {
        await show([Make.item("o", Make.event("me", Make.text("Mine"), own: true, eventId: "$o", editable: true))])
        model.draft = "Half written"
        model.startEdit("o")
        model.draft = "Mine, changed"
        await model.saveDraft().value
        XCTAssertEqual(session.drafts[roomId], Draft(text: "Half written"))
        // Leaving drops the edit.
        model.setVisible(false)
        let back = await reopen()
        XCTAssertEqual(back.draft, "Half written")
        XCTAssertNil(back.compose)
    }

    func testAFailedSaveIsTriedAgain() async {
        session.draftError = MtrxError.Other(reason: "Store locked")
        model.draft = "Hello"
        await model.saveDraft().value
        session.draftError = nil
        await model.saveDraft().value
        XCTAssertEqual(session.drafts[roomId], Draft(text: "Hello"))
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
        timeline.listener?.onUpdate(
            items: [bobsMessage(reactions: [Reaction(key: "👍", count: 2, byUs: true, senders: [alice])])])
        await eventually { self.reactions() == [Reaction(key: "👍", count: 2, byUs: true, senders: [alice])] }
    }

    func testTakingOursAwayShowsAtOnce() async {
        await show([bobsMessage(reactions: [Reaction(key: "👍", count: 2, byUs: true, senders: [])])])
        model.react("b", key: "👍")
        XCTAssertEqual(reactions(), [Reaction(key: "👍", count: 1, byUs: false, senders: [])])
        // Until the library shows it, its stale state is not shown: the
        // reaction of someone else, added meanwhile, is.
        timeline.listener?.onUpdate(
            items: [bobsMessage(reactions: [Reaction(key: "👍", count: 3, byUs: true, senders: [])])])
        await eventually { self.reactions() == [Reaction(key: "👍", count: 2, byUs: false, senders: [])] }
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
