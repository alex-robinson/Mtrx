import MtrxFFI
import XCTest
@testable import Mtrx

/// Which of a space's sections show on this phone, folded or not, in which
/// order; sections new to the phone.
final class SectionLayoutTests: XCTestCase {
    private let account = ["invites", "direct_messages", "favourites", "category:a", "rooms", "low_priority"]

    func testTheAccountsOrderUntilReordered() {
        XCTAssertEqual(SectionLayout().arranged(account), account)
    }

    func testReordering() {
        var layout = SectionLayout()
        // Rooms (4) before Direct messages (1).
        layout.move(account, from: [4], to: 1)
        XCTAssertEqual(
            layout.arranged(account),
            ["invites", "rooms", "direct_messages", "favourites", "category:a", "low_priority"])
        // Invites (0) last.
        layout.move(account, from: [0], to: 6)
        XCTAssertEqual(layout.order?.last, "invites")
    }

    func testANewSectionTakesItsPlaceInTheAccountsOrderShownAndUnfolded() {
        var layout = SectionLayout()
        layout.move(account, from: [4], to: 0)
        layout.setShown("favourites", false)
        layout.toggleFold("direct_messages")
        let withB = ["invites", "direct_messages", "favourites", "category:a", "category:b", "rooms", "low_priority"]
        let arranged = layout.arranged(withB)
        XCTAssertEqual(
            arranged,
            ["rooms", "invites", "direct_messages", "favourites", "category:a", "category:b", "low_priority"])
        XCTAssertTrue(layout.shows("category:b"))
        XCTAssertFalse(layout.folds("category:b"))
        // A section first in the account's order, new: first.
        XCTAssertEqual(layout.arranged(["new"] + account).first, "new")
        // Gone sections leave.
        XCTAssertFalse(layout.arranged(["rooms", "invites"]).contains("favourites"))
    }

    func testFoldingAndShowing() {
        var layout = SectionLayout()
        layout.toggleFold("rooms")
        XCTAssertTrue(layout.folds("rooms"))
        layout.toggleFold("rooms")
        XCTAssertFalse(layout.folds("rooms"))
        layout.setAllShown(account, false)
        XCTAssertFalse(account.contains(where: layout.shows))
        layout.setShown("rooms", true)
        XCTAssertTrue(layout.shows("rooms"))
        layout.setAllShown(account, true)
        XCTAssertTrue(account.allSatisfy(layout.shows))
    }
}

/// A space's list in its sections, and the Notes to self at the top.
@MainActor
final class SpaceSectionsTests: XCTestCase {
    private func state(notesAtTop: Bool = true) -> RoomListState {
        RoomListState(
            rooms: [
                Make.room("!notes", kind: .direct, name: "Notes to self", directUser: "@me:example.org"),
                Make.room("!ann", kind: .direct, name: "Ann", unread: 2, directUser: "@ann:example.org"),
                Make.room("!lab", name: "Lab", unread: 3),
                Make.room("!paper", name: "Paper", unread: 1),
                Make.room("!old", name: "Old"),
                Make.room("!team", kind: .space, name: "Team"),
            ],
            loaded: true,
            spaces: [
                SpaceRooms(
                    spaceId: "!team", roomIds: ["!notes", "!ann", "!lab", "!paper", "!old"],
                    sections: [
                        Make.section(.invites, []),
                        Make.section(.directMessages, ["!notes", "!ann"]),
                        Make.section(.favourites, []),
                        Make.section(.category, ["!paper"], name: "Papers"),
                        Make.section(.rooms, ["!lab"]),
                        Make.section(.lowPriority, ["!old"]),
                    ])
            ],
            notesAtTop: notesAtTop, notesRoomId: "!notes")
    }

    private func titles(_ sections: [ListSection]) -> [String?] { sections.map(\.title) }

    func testSectionsInOrderNamedWithoutEmptyOnes() {
        let lists = RoomLists(state())
        let sections = lists.sections(
            of: lists.space(.space("!team"))!, layout: SectionLayout(), sort: .listOrder, search: "")
        XCTAssertEqual(titles(sections), ["Direct messages", "Papers", "Rooms", "Low priority"])
        XCTAssertEqual(sections.first?.rooms.map(\.id), ["!ann"])
    }

    func testFoldedHiddenAndReordered() {
        let lists = RoomLists(state())
        var layout = SectionLayout()
        layout.toggleFold("rooms")
        layout.setShown("low_priority", false)
        layout.move(lists.space(.space("!team"))!.sections.map(\.key), from: [4], to: 0)
        let sections = lists.sections(of: lists.space(.space("!team"))!, layout: layout, sort: .listOrder, search: "")
        XCTAssertEqual(titles(sections), ["Rooms", "Direct messages", "Papers"])
        XCTAssertTrue(sections[0].folded)
        XCTAssertEqual(sections[0].unread.count, 3)
        // Searching unfolds all.
        let found = lists.sections(of: lists.space(.space("!team"))!, layout: layout, sort: .listOrder, search: "la")
        XCTAssertEqual(found.map(\.folded), [false])
    }

    func testNotesAtTopLeaveTheChatsAndSections() {
        let lists = RoomLists(state())
        XCTAssertEqual(lists.notes?.id, "!notes")
        XCTAssertEqual(lists.chats.map(\.id), ["!ann"])
        let off = RoomLists(state(notesAtTop: false))
        XCTAssertNil(off.notes)
        XCTAssertEqual(off.chats.map(\.id), ["!notes", "!ann"])
        let sections = off.sections(
            of: off.space(.space("!team"))!, layout: SectionLayout(), sort: .listOrder, search: "")
        XCTAssertEqual(sections.first?.rooms.map(\.id), ["!notes", "!ann"])
    }

    func testTheLayoutIsKeptPerSpaceOnThisPhone() {
        let defaults = UserDefaults(suiteName: "SpaceSectionsTests")!
        defaults.removePersistentDomain(forName: "SpaceSectionsTests")
        defer { defaults.removePersistentDomain(forName: "SpaceSectionsTests") }
        SectionLayouts(defaults: defaults).change(.space("!team")) { $0.toggleFold("rooms") }
        let again = SectionLayouts(defaults: defaults)
        XCTAssertTrue(again[.space("!team")].folds("rooms"))
        XCTAssertFalse(again[.space("!other")].folds("rooms"))
        again.forget()
        XCTAssertFalse(SectionLayouts(defaults: defaults)[.space("!team")].folds("rooms"))
    }

    func testNotesAtTopIsChangedForTheAccount() async {
        let session = FakeSession(noHandle: .init())
        let model = SessionModel(session: session)
        model.start()
        await eventually { session.roomListListener != nil }
        session.roomListListener?.onUpdate(state: state())
        await eventually { model.lists.notes != nil }
        await model.setNotesAtTop(false).value
        XCTAssertEqual(session.changes, ["notes at top false"])
        // As chosen until the list has it.
        XCTAssertFalse(model.notesAtTop)
        XCTAssertNil(model.lists.notes)
        session.roomListListener?.onUpdate(state: state(notesAtTop: false))
        await eventually { !model.notesAtTop }
        session.changeError = MtrxError.Other(reason: "No connection")
        await model.setNotesAtTop(true).value
        XCTAssertFalse(model.notesAtTop)
        XCTAssertEqual(model.error, "No connection")
    }
}
