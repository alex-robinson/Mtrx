import MtrxFFI
import XCTest
@testable import Mtrx

final class RoomSectionsTests: XCTestCase {
    func testSectionsInOrderWithoutJoinedSpaces() {
        let rooms = [
            Make.room("general"),
            Make.room("alice", kind: .direct),
            Make.room("space", kind: .space),
            Make.room("invited-space", kind: .space, invited: true),
            Make.room("ice", invited: true),
            Make.room("models"),
        ]
        let groups = RoomSections.group(rooms)
        XCTAssertEqual(groups.map(\.section), [.invites, .people, .rooms])
        XCTAssertEqual(groups[0].rooms.map(\.id), ["invited-space", "ice"])
        XCTAssertEqual(groups[1].rooms.map(\.id), ["alice"])
        XCTAssertEqual(groups[2].rooms.map(\.id), ["general", "models"])
    }

    func testSearchPutsNamesStartingWithTheFilterFirst() {
        let rooms = [
            Make.room("1", name: "Sea ice"),
            Make.room("2", name: "Ice sheets"),
            Make.room("3", name: "Glaciers"),
            Make.room("4", kind: .space, name: "Ice space"),
        ]
        XCTAssertEqual(RoomSections.search(rooms, " ICE").map(\.name), ["Ice sheets", "Sea ice"])
        XCTAssertEqual(RoomSections.search(rooms, ""), [])
    }
}
