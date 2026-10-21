import MtrxFFI
import XCTest
@testable import Mtrx

final class TimelineRowsTests: XCTestCase {
    func testGroupsMessagesBySenderAndTime() {
        let items = [
            Make.item("1", Make.event("alice", Make.text("Hi"), at: 0)),
            Make.item("2", Make.event("alice", Make.text("there"), at: 1)),
            Make.item("3", Make.event("alice", Make.text("later"), at: 10)),
            Make.item("4", Make.event("me", Make.text("Hello"), at: 11, own: true)),
        ]
        let rows = TimelineRows.build(items, showSenders: true)
        XCTAssertEqual(rows.map(\.id), ["1", "2", "3", "4"])
        let messages = rows.compactMap { row -> MessageRow? in
            if case .message(let message) = row.content { return message }
            return nil
        }
        XCTAssertEqual(messages.map(\.firstInGroup), [true, false, true, true])
        // Our own messages carry no name.
        XCTAssertEqual(messages.map(\.showSender), [true, false, true, false])
    }

    func testDirectChatsShowNoNames() {
        let rows = TimelineRows.build([Make.item("1", Make.event("alice", Make.text("Hi")))], showSenders: false)
        guard case .message(let message) = rows.first?.content else { return XCTFail("no message") }
        XCTAssertTrue(message.firstInGroup)
        XCTAssertFalse(message.showSender)
    }

    func testDayDividerStartsAGroup() {
        let items = [
            Make.item("1", Make.event("alice", Make.text("Hi"), at: 0)),
            TimelineItem(id: "d", content: .dayDivider(date: Make.start)),
            Make.item("2", Make.event("alice", Make.text("again"), at: 1)),
        ]
        let rows = TimelineRows.build(items, showSenders: true)
        XCTAssertEqual(rows[1].content, .day(Make.start))
        guard case .message(let message) = rows[2].content else { return XCTFail("no message") }
        XCTAssertTrue(message.firstInGroup)
    }

    func testFoldsStateEventsIntoOneLine() {
        let items = [
            Make.item("c", Make.event("alice", .otherState(eventType: "m.room.create"))),
            Make.item("j1", Make.event("alice", .membership(userId: "@alice:example.org", name: "Alice", change: .joined))),
            Make.item("n", Make.event("alice", .otherState(eventType: "m.room.name"))),
            Make.item("i", Make.event("alice", .membership(userId: "@bob:example.org", name: "Bob", change: .invited))),
            Make.item("j2", Make.event("bob", .membership(userId: "@bob:example.org", name: "Bob", change: .joined))),
            Make.item("m", Make.event("bob", Make.text("Hi all"))),
        ]
        let rows = TimelineRows.build(items, showSenders: true)
        XCTAssertEqual(rows.count, 2)
        XCTAssertEqual(rows[0].id, "c")
        XCTAssertEqual(
            rows[0].content, .summary("Alice created and configured the room · Alice invited Bob · Bob joined"))
    }

    func testLocalpart() {
        XCTAssertEqual(localpart("@alice:example.org"), "alice")
        XCTAssertEqual(localpart("bob"), "bob")
    }
}
