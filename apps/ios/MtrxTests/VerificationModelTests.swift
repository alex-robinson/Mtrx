import MtrxFFI
import XCTest
@testable import Mtrx

@MainActor
final class VerificationModelTests: XCTestCase {
    private var verification: FakeVerification!

    override func setUp() async throws {
        verification = FakeVerification(noHandle: .init())
    }

    private func makeModel(incoming: Bool = false) -> VerificationModel {
        VerificationModel(verification: verification, incoming: incoming)
    }

    private let emojis = [
        VerificationEmoji(symbol: "🐶", description: "Dog"),
        VerificationEmoji(symbol: "🔑", description: "Key"),
    ]

    func testFollowsTheStatesToDone() async {
        let model = makeModel()
        XCTAssertEqual(model.state, .waiting)
        XCTAssertFalse(model.asking)

        verification.listener?.onUpdate(state: .emojis(emojis: emojis))
        await eventually { model.state == .emojis(emojis: self.emojis) }
        model.confirm()
        XCTAssertTrue(model.busy)
        await eventually { !model.busy }
        XCTAssertEqual(verification.confirmed, 1)

        verification.listener?.onUpdate(state: .confirmed)
        await eventually { model.state == .confirmed }
        verification.listener?.onUpdate(state: .sharingKeys)
        await eventually { model.state == .sharingKeys }
        verification.listener?.onUpdate(state: .done)
        await eventually { model.state == .done }
        XCTAssertTrue(model.ended)
    }

    func testStartsAtTheCurrentState() {
        verification.initialState = .emojis(emojis: emojis)
        XCTAssertEqual(makeModel().state, .emojis(emojis: emojis))
    }

    func testMismatchEndsCancelled() async {
        verification.initialState = .emojis(emojis: emojis)
        let model = makeModel()
        model.mismatch()
        await eventually { self.verification.mismatched == 1 }
        verification.listener?.onUpdate(state: .cancelled(reason: "The emoji did not match"))
        await eventually { model.ended }
        XCTAssertEqual(model.state, .cancelled(reason: "The emoji did not match"))
        XCTAssertEqual(verification.confirmed, 0)
    }

    func testCancels() async {
        let model = makeModel()
        model.cancel()
        await eventually { self.verification.cancelled == 1 }
    }

    func testCancelledByTheOtherDevice() async {
        let model = makeModel()
        verification.listener?.onUpdate(state: .cancelled(reason: "The other device cancelled"))
        await eventually { model.ended }
        // Nothing more to cancel or confirm.
        model.cancel()
        model.confirm()
        try? await Task.sleep(nanoseconds: 50_000_000)
        XCTAssertEqual(verification.cancelled, 0)
        XCTAssertEqual(verification.confirmed, 0)
    }

    func testAnIncomingRequestIsAcceptedThenWaits() async {
        let model = makeModel(incoming: true)
        XCTAssertTrue(model.asking)
        model.accept()
        await eventually { !model.asking }
        XCTAssertEqual(verification.accepted, 1)
        XCTAssertEqual(model.state, .waiting)
    }

    func testAFailedAcceptKeepsAsking() async {
        verification.answerError = MtrxError.Other(reason: "No connection")
        let model = makeModel(incoming: true)
        model.accept()
        await eventually { !model.busy }
        XCTAssertTrue(model.asking)
        XCTAssertEqual(model.error, "No connection")
    }

    func testDroppingTheModelEndsTheSubscription() {
        var model: VerificationModel? = makeModel()
        XCTAssertNotNil(model)
        XCTAssertEqual(verification.subscription?.cancelled, false)
        model = nil
        XCTAssertEqual(verification.subscription?.cancelled, true)
    }
}
