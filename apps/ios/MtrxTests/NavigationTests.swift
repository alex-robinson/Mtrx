import MtrxFFI
import XCTest
@testable import Mtrx

/// The tabs' badges and the back button's number.
final class UnreadCountsTests: XCTestCase {
    /// Lab (3 unread) and Deep (4) are in Team, Loose (1) in no space; the
    /// chat with Bob (5), in Team too, and the space itself count as no
    /// room; Bob and the Notes to self (6) as chats.
    func testBadges() {
        let lists = RoomLists(
            RoomListState(
                rooms: [
                    Make.room("!lab", unread: 3), Make.room("!deep", unread: 4), Make.room("!loose", unread: 1),
                    Make.room("!bob", kind: .direct, unread: 5), Make.room("!team", kind: .space, unread: 7),
                    Make.room("!notes", kind: .direct, unread: 6),
                ],
                loaded: true, spaces: [SpaceRooms(spaceId: "!team", roomIds: ["!lab", "!deep", "!bob"])],
                roomsOutsideSpaces: ["!loose"], notesAtTop: true, notesRoomId: "!notes"))
        XCTAssertEqual(lists.chatsUnread, 11)
        XCTAssertEqual(lists.roomsUnread, 8)
        XCTAssertEqual(lists.spacesUnread, 7)
        XCTAssertEqual(lists.unread(besides: "!lab"), 16)
        XCTAssertEqual(lists.unread(besides: "!bob"), 14)
    }
}

@MainActor
final class NavigationTests: XCTestCase {
    private var defaults: UserDefaults!
    private var session: FakeSession!

    override func setUp() async throws {
        defaults = UserDefaults(suiteName: "NavigationTests")
        defaults.removePersistentDomain(forName: "NavigationTests")
        session = FakeSession(noHandle: .init())
    }

    override func tearDown() async throws {
        defaults.removePersistentDomain(forName: "NavigationTests")
    }

    private func launch() async -> SessionModel {
        session.roomListListener = nil
        let model = SessionModel(session: session, defaults: defaults)
        model.start()
        await eventually { self.session.roomListListener != nil }
        return model
    }

    private func send(_ state: RoomListState, to model: SessionModel) async {
        session.roomListListener?.onUpdate(state: state)
        await eventually { model.lists == RoomLists(state) && model.loaded == state.loaded }
    }

    /// Not on the last room, as before: the rooms opened lately are no
    /// longer kept.
    func testTheRoomsTabOpensOnItsList() async {
        defaults.set(["!lab"], forKey: "recentRooms")
        let model = await launch()
        await send(Make.roomsBySpace(), to: model)
        XCTAssertEqual(model.roomsPath, [])
        XCTAssertNil(defaults.stringArray(forKey: "recentRooms"))
    }

    func testRoomsOpenInTheirTabs() async {
        let model = await launch()
        await send(Make.roomsBySpace(), to: model)
        model.open("!lab")
        XCTAssertEqual(model.tab, .rooms)
        XCTAssertEqual(model.roomsPath, ["!lab"])
        model.open("!bob")
        XCTAssertEqual(model.tab, .chats)
        XCTAssertEqual(model.chatsPath, ["!bob"])
        model.open("!team")
        XCTAssertEqual(model.tab, .spaces)
        XCTAssertEqual(model.spacesPath, [.space("!team")])
        // Not in the list yet: as said.
        model.open("!new", kind: .space)
        XCTAssertEqual(model.selectedSpace, .space("!new"))
    }

    func testTappingTheRoomsTabAgainGoesToItsList() async {
        let model = await launch()
        model.select(.chats)
        model.open("!lab")
        model.select(.chats)
        XCTAssertEqual(model.tab, .chats)
        model.select(.rooms)
        XCTAssertEqual(model.roomsPath, ["!lab"])
        model.select(.rooms)
        XCTAssertEqual(model.roomsPath, [])
        XCTAssertEqual(model.tab, .rooms)
    }

    func testAJustMadeSpaceIsNotLetGoBeforeItComes() async {
        let model = await launch()
        await send(Make.roomsBySpace(), to: model)
        model.expect("!new")
        model.open("!new", kind: .space)
        await send(Make.roomsBySpace(), to: model)
        XCTAssertEqual(model.selectedSpace, .space("!new"))
    }

    func testAcceptingAnInviteOpensTheRoom() async throws {
        let model = await launch()
        await send(Make.roomsBySpace(), to: model)
        try await model.accept(Make.room("!party", invited: true))
        XCTAssertEqual(session.changes, ["join !party"])
        XCTAssertEqual(model.tab, .rooms)
        XCTAssertEqual(model.roomsPath, ["!party"])
        XCTAssertTrue(model.expecting.contains("!party"))
    }

    func testDecliningAnInviteLeavesIt() async throws {
        let model = await launch()
        await send(Make.roomsBySpace(), to: model)
        model.open("!party")
        try await model.decline(Make.room("!party", invited: true))
        XCTAssertEqual(session.changes, ["leave !party"])
        XCTAssertEqual(model.roomsPath, [])
    }

    func testOurServerName() async {
        let model = await launch()
        XCTAssertEqual(model.serverName, "example.org")
    }
}
