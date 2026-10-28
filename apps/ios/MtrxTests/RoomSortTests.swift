import MtrxFFI
import XCTest
@testable import Mtrx

/// A space's rooms in two sections, Direct messages and Rooms, in the order
/// chosen.
final class RoomSortTests: XCTestCase {
    private static let now = Date(timeIntervalSince1970: 1_700_000_000)

    /// In the room list's order; Bob and Lab never active.
    private let rooms = [
        Make.room("!lab", name: "Lab"),
        Make.room("!anna", kind: .direct, name: "anna", lastActivity: now.addingTimeInterval(-60)),
        Make.room("!field", name: "Field work", lastActivity: now.addingTimeInterval(-3600)),
        Make.room("!bob", kind: .direct, name: "Bob"),
        Make.room("!General", name: "general", lastActivity: now),
        Make.room("!cleo", kind: .direct, name: "Cleo", lastActivity: now.addingTimeInterval(-7200)),
        Make.room("!offer", invited: true, name: "Offer", lastActivity: now.addingTimeInterval(-60)),
    ]

    private func sections(_ sort: RoomSort, search: String = "") -> [String: [String]] {
        let sections = RoomLists.sections(rooms, sort: sort, search: search)
        return Dictionary(uniqueKeysWithValues: sections.map { ($0.title ?? "", $0.rooms.map(\.id)) })
    }

    func testDirectMessagesThenRooms() {
        let sections = RoomLists.sections(rooms, sort: .listOrder, search: "")
        XCTAssertEqual(sections.map(\.title), ["Direct messages", "Rooms"])
    }

    func testDefaultKeepsTheListsOrder() {
        XCTAssertEqual(sections(.listOrder)["Rooms"], ["!lab", "!field", "!General", "!offer"])
        XCTAssertEqual(sections(.listOrder)["Direct messages"], ["!anna", "!bob", "!cleo"])
    }

    func testAToZAndZToAIgnoreCase() {
        XCTAssertEqual(sections(.aToZ)["Rooms"], ["!field", "!General", "!lab", "!offer"])
        XCTAssertEqual(sections(.aToZ)["Direct messages"], ["!anna", "!bob", "!cleo"])
        XCTAssertEqual(sections(.zToA)["Rooms"], ["!offer", "!lab", "!General", "!field"])
        XCTAssertEqual(sections(.zToA)["Direct messages"], ["!cleo", "!bob", "!anna"])
    }

    func testMostRecentFirstWithoutActivityLast() {
        XCTAssertEqual(sections(.mostRecent)["Rooms"], ["!General", "!offer", "!field", "!lab"])
        XCTAssertEqual(sections(.mostRecent)["Direct messages"], ["!anna", "!cleo", "!bob"])
    }

    func testEqualActivityKeepsTheListsOrder() {
        let same = [
            Make.room("!b", name: "B", lastActivity: Self.now),
            Make.room("!a", name: "A", lastActivity: Self.now),
            Make.room("!c", name: "C"),
            Make.room("!d", name: "D"),
        ]
        XCTAssertEqual(RoomSort.mostRecent.apply(same).map(\.id), ["!b", "!a", "!c", "!d"])
    }

    func testEmptySectionsLeftOut() {
        let onlyRooms = RoomLists.sections(rooms.filter { $0.kind != .direct }, sort: .aToZ, search: "")
        XCTAssertEqual(onlyRooms.map(\.title), ["Rooms"])
        XCTAssertEqual(RoomLists.sections([], sort: .aToZ, search: ""), [])
    }

    func testSearchingOrdersAsTheSearchDoes() {
        // Names starting with the text first, whatever the sort.
        XCTAssertEqual(sections(.zToA, search: "l")["Rooms"], ["!lab", "!field", "!General"])
        XCTAssertEqual(sections(.zToA, search: "l")["Direct messages"], ["!cleo"])
    }
}

/// The order chosen is remembered across launches, for every space.
@MainActor
final class RememberedSortTests: XCTestCase {
    private var defaults: UserDefaults!

    override func setUp() async throws {
        defaults = UserDefaults(suiteName: "RememberedSortTests")
        defaults.removePersistentDomain(forName: "RememberedSortTests")
    }

    override func tearDown() async throws {
        defaults.removePersistentDomain(forName: "RememberedSortTests")
    }

    private func launch() -> SessionModel {
        SessionModel(session: FakeSession(noHandle: .init()), defaults: defaults)
    }

    func testDefaultUntilChosen() {
        XCTAssertEqual(launch().roomSort, .listOrder)
    }

    func testTheChoiceIsRemembered() {
        let first = launch()
        first.setRoomSort(.mostRecent)
        XCTAssertEqual(first.roomSort, .mostRecent)
        XCTAssertEqual(launch().roomSort, .mostRecent)
        first.setRoomSort(.zToA)
        XCTAssertEqual(launch().roomSort, .zToA)
    }

    func testAnUnknownSavedValueIsTheDefault() {
        defaults.set("by-size", forKey: "roomSort")
        XCTAssertEqual(launch().roomSort, .listOrder)
    }
}
