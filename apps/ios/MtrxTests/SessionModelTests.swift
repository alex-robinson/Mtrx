import MtrxFFI
import XCTest
@testable import Mtrx

@MainActor
final class SessionModelTests: XCTestCase {
    private var session: FakeSession!
    private var model: SessionModel!

    override func setUp() async throws {
        session = FakeSession(noHandle: .init())
        model = SessionModel(session: session)
        model.start()
        await eventually { self.session.roomListListener != nil }
    }

    func testBannerWhileTheDeviceIsUnverifiedOrRecoveryLocked() async {
        // Not asked yet.
        XCTAssertFalse(model.needsEncryptionSetup)
        let cases: [(EncryptionState, Bool)] = [
            // No sync yet: ask again later.
            (EncryptionState(deviceVerified: false, recovery: .unknown), false),
            (EncryptionState(deviceVerified: false, recovery: .incomplete), true),
            (EncryptionState(deviceVerified: false, recovery: .disabled), true),
            (EncryptionState(deviceVerified: true, recovery: .incomplete), true),
            (EncryptionState(deviceVerified: true, recovery: .enabled), false),
            (EncryptionState(deviceVerified: true, recovery: .disabled), false),
        ]
        for (state, banner) in cases {
            session.encryption = state
            model.refreshEncryption()
            await eventually { self.model.encryption == state }
            XCTAssertEqual(model.needsEncryptionSetup, banner, "\(state)")
        }
    }

    func testAsksForTheEncryptionStateWhenSyncStartsAndTheRoomListLoads() async {
        model.setSyncing(true)
        // Returning to the foreground asks at once.
        await eventually { self.session.syncStarts == 1 && self.session.encryptionRequests == 1 }

        session.roomListListener?.onUpdate(state: RoomListState(rooms: [Make.room("!a")], loaded: false))
        await eventually { self.model.rooms.count == 1 }
        await settle()
        XCTAssertEqual(session.encryptionRequests, 1)

        session.roomListListener?.onUpdate(state: RoomListState(rooms: [Make.room("!a")], loaded: true))
        await eventually { self.session.encryptionRequests == 2 }

        // Only the first time it reports being loaded.
        session.roomListListener?.onUpdate(state: RoomListState(rooms: [Make.room("!a"), Make.room("!b")], loaded: true))
        await eventually { self.model.rooms.count == 2 }
        await settle()
        XCTAssertEqual(session.encryptionRequests, 2)
    }

    func testRecoveryAsksForTheEncryptionStateAgain() async {
        model.recovery.key = "EsTc abcd"
        model.recovery.recover()
        await eventually { self.model.recovery.recovered }
        await eventually { self.session.encryptionRequests == 1 }
    }

    func testVerifiesWithAnotherDevice() async {
        model.verifyWithAnotherDevice()
        await eventually { self.model.verification != nil }
        XCTAssertFalse(model.requestingVerification)
        XCTAssertEqual(model.verification?.incoming, false)
        XCTAssertEqual(model.verification?.asking, false)

        session.selfVerification.listener?.onUpdate(state: .done)
        await eventually { self.model.verification?.state == .done }
        // Verified: the state is asked again.
        await eventually { self.session.encryptionRequests == 1 }

        model.closeVerification()
        XCTAssertNil(model.verification)
        XCTAssertEqual(session.selfVerification.subscription?.cancelled, true)
    }

    func testKnowsWhetherAnotherDeviceCanVerify() async {
        session.deviceList = [DeviceInfo(id: "DEVICE", name: "iPhone", verified: false, current: true)]
        await model.loadDevices()
        XCTAssertEqual(model.hasOtherDevice, false)
        session.deviceList.append(DeviceInfo(id: "OTHER", name: "Mtrx desktop", verified: true, current: false))
        await model.loadDevices()
        XCTAssertEqual(model.hasOtherDevice, true)
    }

    func testAsksAboutARequestFromOurOtherDevice() async {
        let request = FakeVerification(noHandle: .init())
        session.verificationRequestListener?.onRequest(verification: request)
        await eventually { self.model.verification != nil }
        XCTAssertEqual(model.verification?.incoming, true)
        XCTAssertEqual(model.verification?.asking, true)
        XCTAssertEqual(request.cancelled, 0)
    }

    func testCancelsARequestFromAnotherPerson() async {
        let request = FakeVerification(noHandle: .init())
        request.user = "@bob:example.org"
        session.verificationRequestListener?.onRequest(verification: request)
        await eventually { request.cancelled == 1 }
        XCTAssertNil(model.verification)
    }

    func testANewRequestReplacesAnUnansweredOne() async {
        let first = FakeVerification(noHandle: .init())
        session.verificationRequestListener?.onRequest(verification: first)
        await eventually { self.model.verification != nil }
        let shown = model.verification

        let second = FakeVerification(noHandle: .init())
        session.verificationRequestListener?.onRequest(verification: second)
        await eventually { self.model.verification !== shown }
        await eventually { first.cancelled == 1 }
        XCTAssertEqual(second.cancelled, 0)
    }

    func testARequestDuringAVerificationIsCancelled() async {
        model.verifyWithAnotherDevice()
        await eventually { self.model.verification != nil }
        let shown = model.verification

        let request = FakeVerification(noHandle: .init())
        session.verificationRequestListener?.onRequest(verification: request)
        await eventually { request.cancelled == 1 }
        XCTAssertTrue(model.verification === shown)
    }

    /// Let tasks started on the main actor run.
    private func settle() async {
        try? await Task.sleep(nanoseconds: 50_000_000)
    }
}
