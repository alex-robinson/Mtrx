import Combine
import MtrxFFI
import XCTest
@testable import Mtrx

@MainActor
final class RecoveryModelTests: XCTestCase {
    private var session: FakeSession!
    private var model: RecoveryModel!

    override func setUp() async throws {
        session = FakeSession(noHandle: .init())
        model = RecoveryModel(session: session)
    }

    func testShowsEachStepInOrder() async {
        session.recoverySteps = [
            .checkingKey, .importingIdentity,
            .downloadingKeys(done: 0, total: 2), .downloadingKeys(done: 1, total: 2),
            .downloadingKeys(done: 2, total: 2),
        ]
        var shown: [RecoveryStep] = []
        let watching = model.$step.sink { step in
            if let step { shown.append(step) }
        }
        defer { watching.cancel() }

        model.key = "  EsTc abcd efgh\n"
        model.recover()
        XCTAssertTrue(model.recovering)
        await eventually { self.model.recovered }

        XCTAssertEqual(shown, session.recoverySteps)
        XCTAssertEqual(session.recoveredWith, "EsTc abcd efgh")
        XCTAssertFalse(model.recovering)
        XCTAssertNil(model.step)
        XCTAssertNil(model.error)
        XCTAssertEqual(model.key, "")
    }

    func testAnInvalidKeySaysSoUnderTheField() async {
        session.recoverySteps = [.checkingKey]
        session.recoverError = MtrxError.InvalidInput(reason: "The recovery key is not valid")
        model.key = "EsTc"
        model.recover()
        await eventually { !self.model.recovering }

        XCTAssertEqual(model.keyError, "The recovery key is not valid.")
        XCTAssertNil(model.error)
        XCTAssertFalse(model.recovered)
        XCTAssertNil(model.step)
        // The key stays, to correct it.
        XCTAssertEqual(model.key, "EsTc")

        // A new attempt clears the message.
        session.recoverError = nil
        model.recover()
        XCTAssertNil(model.keyError)
        await eventually { self.model.recovered }
    }

    func testOtherFailuresSayWhy() async {
        session.recoverySteps = [.checkingKey, .importingIdentity]
        session.recoverError = MtrxError.Other(reason: "The backup could not be downloaded")
        model.key = "EsTc abcd"
        model.recover()
        await eventually { !self.model.recovering }

        XCTAssertEqual(model.error, "The backup could not be downloaded")
        XCTAssertNil(model.keyError)
        XCTAssertEqual(model.key, "EsTc abcd")
    }

    func testNothingWithoutAKey() {
        model.key = "  \n"
        model.recover()
        XCTAssertFalse(model.recovering)
        XCTAssertNil(session.recoveredWith)
    }
}
