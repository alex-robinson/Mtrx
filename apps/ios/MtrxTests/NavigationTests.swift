import MtrxFFI
import XCTest
@testable import Mtrx

/// Recent rooms, the tabs' badges, and where rooms open.
final class RecentRoomsTests: XCTestCase {
    func testOpenedRoomsComeFirstOnce() {
        var recent = RecentRooms()
        recent.opened("!a")
        recent.opened("!b")
        recent.opened("!a")
        XCTAssertEqual(recent.ids, ["!a", "!b"])
        XCTAssertEqual(recent.last, "!a")
    }

    func testKeepsAtMostTheLimit() {
        var recent = RecentRooms()
        for n in 0..<(RecentRooms.limit + 5) {
            recent.opened("!\(n)")
        }
        XCTAssertEqual(recent.ids.count, RecentRooms.limit)
        XCTAssertEqual(recent.last, "!\(RecentRooms.limit + 4)")
        XCTAssertEqual(RecentRooms((0..<30).map { "!\($0)" }).ids.count, RecentRooms.limit)
    }

    func testLeftRoomsGo() {
        var recent = RecentRooms(["!a", "!b", "!c"])
        recent.keep(["!a", "!c"])
        XCTAssertEqual(recent.ids, ["!a", "!c"])
        recent.remove("!a")
        XCTAssertEqual(recent.ids, ["!c"])
    }

    /// Lab (3 unread) and Deep (4) are recent; Loose (1) and Far (2) are
    /// not; the chat with Bob (5) counts nowhere.
    func testBadges() {
        let lists = RoomLists(
            RoomListState(
                rooms: [
                    Make.room("!lab", unread: 3), Make.room("!deep", unread: 4), Make.room("!loose", unread: 1),
                    Make.room("!far", unread: 2), Make.room("!bob", kind: .direct, unread: 5),
                    Make.room("!team", kind: .space, unread: 7),
                ],
                loaded: true))
        let recent = RecentRooms(["!lab", "!deep"])
        XCTAssertEqual(recent.unread(in: lists), 7)
        XCTAssertEqual(recent.unread(in: lists, besides: "!lab"), 4)
        XCTAssertEqual(recent.unreadElsewhere(in: lists), 3)
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

    func testOpenedRoomsAreRememberedAndTheLastReopens() async {
        let first = await launch()
        await send(Make.roomsBySpace(), to: first)
        XCTAssertEqual(first.roomsPath, [])
        first.opened(Make.room("!lab"))
        first.opened(Make.room("!deep"))
        // Not chats, spaces or invites.
        first.opened(Make.room("!bob", kind: .direct))
        first.opened(Make.room("!team", kind: .space))
        first.opened(Make.room("!party", invited: true))
        XCTAssertEqual(first.recent.ids, ["!deep", "!lab"])

        let next = await launch()
        XCTAssertEqual(next.recent.ids, ["!deep", "!lab"])
        XCTAssertEqual(next.roomsPath, ["!deep"])
    }

    func testRoomsLeftGoFromRecentOnceTheListIsLoaded() async {
        defaults.set(["!gone", "!lab"], forKey: "recentRooms")
        let model = await launch()
        await send(Make.roomsBySpace(loaded: false), to: model)
        XCTAssertEqual(model.recent.ids, ["!gone", "!lab"])
        await send(Make.roomsBySpace(), to: model)
        XCTAssertEqual(model.recent.ids, ["!lab"])
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

    func testTappingTheRoomsTabAgainGoesToRecent() async {
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

    func testSigningOutForgetsTheRecentRooms() async throws {
        let model = await launch()
        model.opened(Make.room("!lab"))
        try await model.logout()
        XCTAssertNil(defaults.stringArray(forKey: "recentRooms"))
    }

    func testOurServerName() async {
        let model = await launch()
        XCTAssertEqual(model.serverName, "example.org")
    }
}
