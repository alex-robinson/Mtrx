import Combine
import MtrxFFI
import XCTest
@testable import Mtrx

@MainActor
final class RecoveryModelTests: XCTestCase {
    private var session: FakeSession!
    private var recovered = 0
    private var model: RecoveryModel!

    override func setUp() async throws {
        session = FakeSession(noHandle: .init())
        recovered = 0
        model = RecoveryModel(session: session) { [unowned self] in self.recovered += 1 }
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
        XCTAssertEqual(recovered, 1)
    }

    func testAWrongKeySaysWhy() async {
        session.recoverySteps = [.checkingKey]
        session.recoverError = MtrxError.Other(reason: "The Base58 decoded key has an invalid length")
        model.key = "EsTc"
        model.recover()
        await eventually { !self.model.recovering }

        XCTAssertEqual(model.error, "The Base58 decoded key has an invalid length")
        XCTAssertFalse(model.recovered)
        XCTAssertNil(model.step)
        // The key stays, to correct it.
        XCTAssertEqual(model.key, "EsTc")
        XCTAssertEqual(recovered, 0)

        // A new attempt clears the error.
        session.recoverError = nil
        model.recover()
        XCTAssertNil(model.error)
        await eventually { self.model.recovered }
    }

    func testNothingWithoutAKey() {
        model.key = "  \n"
        model.recover()
        XCTAssertFalse(model.recovering)
        XCTAssertNil(session.recoveredWith)
    }
}
