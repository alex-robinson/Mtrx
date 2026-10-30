import MtrxFFI
import XCTest
@testable import Mtrx

final class RoomListsTests: XCTestCase {
    private let lists = RoomLists(Make.roomsBySpace())

    func testChatsAreTheDirectChatsInListOrder() {
        XCTAssertEqual(lists.chats.map(\.id), ["!carol", "!bob"])
        XCTAssertTrue(lists.chats[0].invited)
    }

    func testSpacesInOrderThenHomeWithUnreadDots() {
        XCTAssertEqual(lists.spaces.map(\.entry), [.space("!team"), .space("!other"), .home])
        XCTAssertEqual(lists.spaces.map(\.name), ["Team", "Other", "Home"])
        // Bob's chat in Team, Loose outside spaces.
        XCTAssertEqual(lists.spaces.map(\.unread), [true, false, true])
    }

    func testASpacesRoomsAndHomesInListOrder() {
        // Through the nested space, not the nested space itself; the invite
        // and the direct chat with a member.
        XCTAssertEqual(lists.space(.space("!team"))?.rooms.map(\.id), ["!offer", "!bob", "!deep", "!lab"])
        XCTAssertEqual(lists.space(.space("!other"))?.rooms, [])
        XCTAssertEqual(lists.space(.home)?.rooms.map(\.id), ["!party", "!loose"])
        XCTAssertNil(lists.space(.space("!sub")))
    }

    func testTheTopLevelSpacesToInviteTo() {
        XCTAssertEqual(lists.topLevelSpaces.map(\.id), ["!team", "!other"])
        XCTAssertEqual(lists.topLevelSpaces.map(\.name), ["Team", "Other"])
    }

    func testHomeLeftOutWhileEmpty() {
        var state = Make.roomsBySpace()
        state.roomsOutsideSpaces = []
        XCTAssertEqual(RoomLists(state).spaces.map(\.entry), [.space("!team"), .space("!other")])
    }

    /// Home first, then Team; Other has no rooms. No direct chats, nested
    /// spaces or invites to spaces; invites to rooms are among them.
    func testTheRoomsTabsGroups() {
        let groups = lists.roomGroups()
        XCTAssertEqual(groups.map(\.key), [RoomLists.homeKey, "!team"])
        XCTAssertEqual(groups.map(\.title), ["Home", "Team"])
        XCTAssertEqual(groups.map(\.space), [nil, "!team"])
        XCTAssertEqual(groups.map { $0.rooms.map(\.id) }, [["!party", "!loose"], ["!offer", "!deep", "!lab"]])
        XCTAssertEqual(groups[0].unread.count, 1)
    }

    /// Newest first, those without activity last; a room in two spaces in
    /// both.
    func testTheRoomsTabsGroupsByLatestMessage() {
        let now = Date()
        let state = RoomListState(
            rooms: [
                Make.room("!a", lastActivity: now.addingTimeInterval(-60)),
                Make.room("!b"),
                Make.room("!c", lastActivity: now),
                Make.room("!one", kind: .space),
                Make.room("!two", kind: .space),
            ],
            loaded: true,
            spaces: [
                SpaceRooms(spaceId: "!one", roomIds: ["!a", "!b", "!c"]),
                SpaceRooms(spaceId: "!two", roomIds: ["!b", "!a"]),
            ])
        let groups = RoomLists(state).roomGroups()
        XCTAssertEqual(groups.map(\.key), ["!one", "!two"])
        XCTAssertEqual(groups.map { $0.rooms.map(\.id) }, [["!c", "!a", "!b"], ["!a", "!b"]])
    }

    /// The rooms shared with someone: those of the given IDs.
    func testTheRoomsTabsGroupsAmongSome() {
        let groups = lists.roomGroups(among: ["!lab", "!bob", "!team", "!loose"])
        XCTAssertEqual(groups.map { $0.rooms.map(\.id) }, [["!loose"], ["!lab"]])
        XCTAssertEqual(lists.roomGroups(among: ["!bob"]), [])
    }

    /// Through the nested space; none for a room in no space.
    func testARoomsFirstSpace() {
        XCTAssertEqual(lists.firstSpace(of: "!deep")?.id, "!team")
        XCTAssertNil(lists.firstSpace(of: "!loose"))
        var state = Make.roomsBySpace()
        state.spaces[1].roomIds = ["!lab"]
        XCTAssertEqual(RoomLists(state).firstSpace(of: "!lab")?.id, "!team")
        state.spaces.swapAt(0, 1)
        XCTAssertEqual(RoomLists(state).firstSpace(of: "!lab")?.id, "!other")
    }

    /// The spaces that list a chat, in the Spaces tab's order; none for a
    /// chat in no space.
    func testTheSpacesSharedInAChat() {
        XCTAssertEqual(lists.chatSpaces, ["!bob": ["Team"]])
        var state = Make.roomsBySpace()
        state.spaces[1].roomIds = ["!bob", "!carol"]
        XCTAssertEqual(RoomLists(state).chatSpaces, ["!bob": ["Team", "Other"], "!carol": ["Other"]])
    }

    func testTheRoomsWithADraft() {
        XCTAssertEqual(lists.drafts, [])
        var state = Make.roomsBySpace()
        state.drafts = ["!lab", "!bob"]
        XCTAssertEqual(RoomLists(state).drafts, ["!lab", "!bob"])
    }

    func testTheSavedEntryRoundTrips() {
        for entry in [SpaceEntry.space("!team:example.org"), .home] {
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
        XCTAssertEqual(RoomLists.search(lists.spaces, "h").map(\.entry), [.home, .space("!other")])
    }
}
