import MtrxFFI
import XCTest
@testable import Mtrx

final class RoomListsTests: XCTestCase {
    private let lists = RoomLists(Make.roomsBySpace())

    func testChatsAreTheDirectChatsInListOrder() {
        XCTAssertEqual(lists.chats.map(\.id), ["!carol", "!bob"])
        XCTAssertTrue(lists.chats[0].invited)
    }

    func testSpacesInOrderThenOtherRoomsWithUnreadDots() {
        XCTAssertEqual(lists.spaces.map(\.entry), [.space("!team"), .space("!other"), .otherRooms])
        XCTAssertEqual(lists.spaces.map(\.name), ["Team", "Other", "Other rooms"])
        // Bob's chat in Team, Loose outside spaces.
        XCTAssertEqual(lists.spaces.map(\.unread), [true, false, true])
    }

    func testASpacesRoomsAndOtherRoomsInListOrder() {
        // Through the nested space, not the nested space itself; the invite
        // and the direct chat with a member.
        XCTAssertEqual(lists.space(.space("!team"))?.rooms.map(\.id), ["!offer", "!bob", "!deep", "!lab"])
        XCTAssertEqual(lists.space(.space("!other"))?.rooms, [])
        XCTAssertEqual(lists.space(.otherRooms)?.rooms.map(\.id), ["!party", "!loose"])
        XCTAssertNil(lists.space(.space("!sub")))
    }

    func testTheTopLevelSpacesToInviteTo() {
        XCTAssertEqual(lists.topLevelSpaces.map(\.id), ["!team", "!other"])
        XCTAssertEqual(lists.topLevelSpaces.map(\.name), ["Team", "Other"])
    }

    func testOtherRoomsLeftOutWhileEmpty() {
        var state = Make.roomsBySpace()
        state.roomsOutsideSpaces = []
        XCTAssertEqual(RoomLists(state).spaces.map(\.entry), [.space("!team"), .space("!other")])
    }

    func testTheRoomsWithADraft() {
        XCTAssertEqual(lists.drafts, [])
        var state = Make.roomsBySpace()
        state.drafts = ["!lab", "!bob"]
        XCTAssertEqual(RoomLists(state).drafts, ["!lab", "!bob"])
    }

    func testTheSavedEntryRoundTrips() {
        for entry in [SpaceEntry.space("!team:example.org"), .otherRooms] {
            XCTAssertEqual(SpaceEntry(saved: entry.saved), entry)
        }
    }

    func testSearchPutsNamesStartingWithTheFilterFirst() {
        let rooms = [
            Make.room("1", name: "Sea ice"),
            Make.room("2", name: "Ice sheets"),
            Make.room("3", name: "Glaciers"),
        ]
        XCTAssertEqual(RoomLists.search(rooms, " ICE").map(\.name), ["Ice sheets", "Sea ice"])
        XCTAssertEqual(RoomLists.search(rooms, ""), rooms)
        XCTAssertEqual(RoomLists.search(lists.spaces, "other").map(\.entry), [.space("!other"), .otherRooms])
    }
}
