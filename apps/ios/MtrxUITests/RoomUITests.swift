import XCTest

/// The app with made-up messages (`-ui-testing`), in the Lab room, as a
/// finger uses it: deleting one of our messages through its menu, and
/// sending. Each step is saved as a PNG where `SCREENSHOTS_DIR` says.
final class RoomUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() async throws {
        continueAfterFailure = false
        await MainActor.run {
            let app = XCUIApplication()
            app.launchArguments = ["-ui-testing", "-recentRooms", "(\"!lab:example.org\")"]
            app.launch()
            self.app = app
        }
    }

    /// The Rooms tab opens on the Lab room, its last message ours.
    @MainActor
    private func openLab() {
        app.tabBars.buttons["Rooms"].tap()
        XCTAssertTrue(app.staticTexts["Mine, to delete"].waitForExistence(timeout: 15), "The room did not open")
        snap("ui-room")
    }

    @MainActor
    func testDeletingASentMessage() {
        openLab()
        app.staticTexts["Mine, to delete"].press(forDuration: 1.5)
        let delete = app.buttons["Delete"].firstMatch
        let inMenu = delete.waitForExistence(timeout: 5)
        snap("ui-delete-menu")
        XCTAssertTrue(inMenu, "No Delete in the message's menu")
        delete.tap()
        sleep(1)
        let confirm = app.buttons["Delete"].firstMatch
        let asked = confirm.waitForExistence(timeout: 5)
        snap("ui-delete-dialog")
        XCTAssertTrue(asked, "No dialog to confirm")
        confirm.tap()
        let deleted = app.staticTexts["Message deleted"].waitForExistence(timeout: 5)
        snap("ui-deleted")
        XCTAssertTrue(deleted, "The message was not deleted")
    }

    @MainActor
    func testTheTimelineStaysShownWhileSending() {
        openLab()
        let field = app.textViews.firstMatch.exists ? app.textViews.firstMatch : app.textFields.firstMatch
        field.tap()
        let newest = inView(app.staticTexts["Mine, to delete"])
        snap("ui-keyboard")
        XCTAssertTrue(newest, "The newest message is under the keyboard")
        for n in 1...4 {
            field.tap()
            field.typeText("Sent number \(n), with a few words")
            snap("ui-written-\(n)")
            app.buttons["Send"].tap()
            let shown = inView(app.staticTexts["Sent number \(n), with a few words"])
            snap("ui-sent-\(n)")
            XCTAssertTrue(shown, "Sent message \(n) not in view")
        }
    }

    /// `element` is in view (hittable), once the keyboard and the timeline
    /// have moved: within 3 seconds.
    @MainActor
    private func inView(_ element: XCUIElement) -> Bool {
        let hittable = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isHittable == true"), object: element)
        return XCTWaiter().wait(for: [hittable], timeout: 3) == .completed
    }

    @MainActor
    private func snap(_ name: String) {
        let shot = XCUIScreen.main.screenshot()
        let attachment = XCTAttachment(screenshot: shot)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
        if let folder = ProcessInfo.processInfo.environment["SCREENSHOTS_DIR"] {
            try? FileManager.default.createDirectory(atPath: folder, withIntermediateDirectories: true)
            try? shot.pngRepresentation.write(to: URL(fileURLWithPath: folder).appendingPathComponent("\(name).png"))
        }
    }
}
