import MtrxFFI
import XCTest
@testable import Mtrx

/// The Spaces tab's selected space and the Rooms tab's folded groups,
/// remembered across launches until sign-out.
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
        XCTAssertEqual(model.spacesPath, [])
    }

    func testTheSelectedSpaceIsRestoredOnTheRoomsPage() async {
        let first = await launch()
        await send(Make.roomsBySpace(), to: first)
        first.selectSpace(.space("!team"))
        XCTAssertEqual(first.spacesPath, [.space("!team")])

        let next = await launch()
        XCTAssertEqual(next.selectedSpace, .space("!team"))
        XCTAssertEqual(next.spacesPath, [.space("!team")])
        await send(Make.roomsBySpace(), to: next)
        XCTAssertEqual(next.selectedSpace, .space("!team"))
        XCTAssertEqual(next.spacesPath, [.space("!team")])
    }

    func testHomeIsRestoredToo() async {
        let first = await launch()
        first.selectSpace(.home)
        let next = await launch()
        await send(Make.roomsBySpace(), to: next)
        XCTAssertEqual(next.selectedSpace, .home)
    }

    func testAGoneSpaceIsLetGoOnceTheListIsLoaded() async {
        defaults.set("!left", forKey: "selectedSpace")
        let model = await launch()
        XCTAssertEqual(model.selectedSpace, .space("!left"))

        // A cache: the space may come with sync.
        await send(Make.roomsBySpace(loaded: false), to: model)
        XCTAssertEqual(model.selectedSpace, .space("!left"))
        XCTAssertEqual(model.spacesPath, [.space("!left")])

        await send(Make.roomsBySpace(), to: model)
        XCTAssertNil(model.selectedSpace)
        XCTAssertEqual(model.spacesPath, [])
        XCTAssertNil(defaults.string(forKey: "selectedSpace"))
    }

    func testASpaceThatDisappearsWhileShownIsLetGo() async {
        let model = await launch()
        await send(Make.roomsBySpace(), to: model)
        model.selectSpace(.home)

        var state = Make.roomsBySpace()
        state.roomsOutsideSpaces = []
        await send(state, to: model)
        XCTAssertNil(model.selectedSpace)
        XCTAssertEqual(model.spacesPath, [])
    }

    /// The Rooms tab's groups folded here: Home and spaces alike, the
    /// others unfolded.
    func testTheRoomsTabsFoldedGroupsAreRestored() async {
        let first = await launch()
        await send(Make.roomsBySpace(), to: first)
        XCTAssertEqual(first.roomGroups.map(\.folded), [false, false])
        first.toggleGroupFold("!team")
        first.toggleGroupFold(RoomLists.homeKey)
        first.toggleGroupFold(RoomLists.homeKey)
        XCTAssertEqual(first.roomGroups.map(\.folded), [false, true])

        let next = await launch()
        await send(Make.roomsBySpace(), to: next)
        XCTAssertEqual(next.roomGroups.map(\.key), [RoomLists.homeKey, "!team"])
        XCTAssertEqual(next.roomGroups.map(\.folded), [false, true])
        XCTAssertEqual(next.roomGroups.map(\.rooms), next.lists.roomGroups().map(\.rooms))
    }

    func testSigningOutForgetsTheSpace() async throws {
        let model = await launch()
        model.selectSpace(.space("!team"))
        model.toggleGroupFold("!team")
        try await model.logout()
        XCTAssertNil(defaults.string(forKey: "selectedSpace"))
        XCTAssertEqual(model.foldedGroups, [])
        XCTAssertNil(defaults.stringArray(forKey: "foldedRoomGroups"))

        let next = await launch()
        next.selectSpace(.space("!team"))
        try await next.forget()
        XCTAssertNil(defaults.string(forKey: "selectedSpace"))
    }
}
