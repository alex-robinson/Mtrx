import MtrxFFI
import XCTest
@testable import Mtrx

/// The Rooms tab's selected space, remembered across launches until
/// sign-out.
@MainActor
final class SelectedSpaceTests: XCTestCase {
    private var defaults: UserDefaults!
    private var session: FakeSession!

    override func setUp() async throws {
        defaults = UserDefaults(suiteName: "SelectedSpaceTests")
        defaults.removePersistentDomain(forName: "SelectedSpaceTests")
        session = FakeSession(noHandle: .init())
    }

    override func tearDown() async throws {
        defaults.removePersistentDomain(forName: "SelectedSpaceTests")
    }

    /// A model as at a launch, following the room list.
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

    func testOpensOnTheSpacesWithoutASavedSpace() async {
        let model = await launch()
        XCTAssertNil(model.selectedSpace)
        XCTAssertEqual(model.roomsPage, .spaces)
    }

    func testTheSelectedSpaceIsRestoredOnTheRoomsPage() async {
        let first = await launch()
        await send(Make.roomsBySpace(), to: first)
        first.selectSpace(.space("!team"))
        XCTAssertEqual(first.roomsPage, .rooms)

        let next = await launch()
        XCTAssertEqual(next.selectedSpace, .space("!team"))
        XCTAssertEqual(next.roomsPage, .rooms)
        await send(Make.roomsBySpace(), to: next)
        XCTAssertEqual(next.selectedSpace, .space("!team"))
        XCTAssertEqual(next.roomsPage, .rooms)
    }

    func testOtherRoomsIsRestoredToo() async {
        let first = await launch()
        first.selectSpace(.otherRooms)
        let next = await launch()
        await send(Make.roomsBySpace(), to: next)
        XCTAssertEqual(next.selectedSpace, .otherRooms)
    }

    func testAGoneSpaceIsLetGoOnceTheListIsLoaded() async {
        defaults.set("!left", forKey: "selectedSpace")
        let model = await launch()
        XCTAssertEqual(model.selectedSpace, .space("!left"))

        // A cache: the space may come with sync.
        await send(Make.roomsBySpace(loaded: false), to: model)
        XCTAssertEqual(model.selectedSpace, .space("!left"))
        XCTAssertEqual(model.roomsPage, .rooms)

        await send(Make.roomsBySpace(), to: model)
        XCTAssertNil(model.selectedSpace)
        XCTAssertEqual(model.roomsPage, .spaces)
        XCTAssertNil(defaults.string(forKey: "selectedSpace"))
    }

    func testASpaceThatDisappearsWhileShownIsLetGo() async {
        let model = await launch()
        await send(Make.roomsBySpace(), to: model)
        model.selectSpace(.otherRooms)

        var state = Make.roomsBySpace()
        state.roomsOutsideSpaces = []
        await send(state, to: model)
        XCTAssertNil(model.selectedSpace)
        XCTAssertEqual(model.roomsPage, .spaces)
    }

    func testSigningOutForgetsTheSpace() async throws {
        let model = await launch()
        model.selectSpace(.space("!team"))
        try await model.logout()
        XCTAssertNil(defaults.string(forKey: "selectedSpace"))

        let next = await launch()
        next.selectSpace(.space("!team"))
        try await next.forget()
        XCTAssertNil(defaults.string(forKey: "selectedSpace"))
    }
}
