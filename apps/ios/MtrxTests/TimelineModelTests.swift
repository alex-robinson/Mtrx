import MtrxFFI
import XCTest
@testable import Mtrx

@MainActor
final class TimelineModelTests: XCTestCase {
    private var session: FakeSession!
    private var model: TimelineModel!

    override func setUp() async throws {
        session = FakeSession(noHandle: .init())
        model = TimelineModel(session: session, room: Make.room("!room:example.org"))
        await model.open()
    }

    func testShowsTheListenersItems() async {
        XCTAssertFalse(model.opening)
        session.fakeTimeline.listener?.onUpdate(items: [Make.item("1", Make.event("alice", Make.text("Hi")))])
        await eventually { self.model.rows.map(\.id) == ["1"] }
    }

    func testMarksReadWhenVisible() async {
        model.setVisible(true)
        session.fakeTimeline.listener?.onUpdate(items: [Make.item("1", Make.event("alice", Make.text("Hi")))])
        await eventually { self.session.fakeTimeline.markedRead == 1 }
    }

    func testSendsTheDraft() async {
        model.draft = "  Hello  "
        await model.send()
        XCTAssertEqual(session.fakeTimeline.sent, ["Hello"])
        XCTAssertEqual(model.draft, "")
    }

    func testKeepsTheDraftWhenSendingFails() async {
        session.fakeTimeline.sendError = MtrxError.Other(message: "No connection")
        model.draft = "Hello"
        await model.send()
        XCTAssertEqual(model.draft, "Hello")
        XCTAssertEqual(model.error, "No connection")
    }

    func testLoadsOlderWhileTheTopIsVisible() async {
        session.fakeTimeline.pages = [false, false, true]
        await model.setTopVisible(true)
        XCTAssertEqual(session.fakeTimeline.paginations, 3)
        XCTAssertTrue(model.reachedStart)
        XCTAssertFalse(model.loadingOlder)
        // At the start: nothing more to load.
        await model.loadOlder()
        XCTAssertEqual(session.fakeTimeline.paginations, 3)
    }
}
