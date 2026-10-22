import MtrxFFI
import UIKit

/// A signed-in account: its room lists, connection and pictures, sync,
/// which runs while the app is in the foreground, and whether this device
/// can read encrypted messages.
@MainActor
final class SessionModel: ObservableObject {
    let session: Session
    let images: ImageCache
    let userId: String
    let deviceId: String
    let homeserver: String

    /// Most recently active first.
    @Published private(set) var rooms: [RoomSummary] = []
    /// Synced with the server since start; before, `rooms` may be a stale
    /// cache, or empty on the first sign-in.
    @Published private(set) var loaded = false
    /// The chats, and the rooms by space.
    @Published private(set) var lists = RoomLists()
    /// The Rooms tab's selected space, remembered across launches (until
    /// sign-out).
    @Published private(set) var selectedSpace: SpaceEntry?
    /// The Rooms tab's page: the spaces, or the selected space's rooms.
    @Published var roomsPage = RoomsPage.spaces
    @Published private(set) var connection: ConnectionState = .online
    @Published var error: String?

    /// Whether this device is verified, and the account's recovery; nil
    /// until first asked. Not pushed by the library: asked again when the
    /// room list loads after sync starts, after a recovery or verification,
    /// and when the app returns to the foreground.
    @Published private(set) var encryption: EncryptionState?
    /// Whether another of our devices is signed in (to verify with); nil
    /// until known.
    @Published private(set) var hasOtherDevice: Bool?
    /// The verification on screen, asked for here or by another device.
    @Published private(set) var verification: VerificationModel?
    @Published private(set) var requestingVerification = false
    /// Why asking another device to verify failed.
    @Published var verificationError: String?
    private(set) lazy var recovery = RecoveryModel(session: session) { [weak self] in
        self?.refreshEncryption()
    }

    private let subscriptions = Subscriptions()
    private var syncing = false
    /// The last start or stop of sync; the next waits for it.
    private var syncChange: Task<Void, Never>?
    /// Sync started and the room list has not reported being loaded since.
    private var encryptionOnLoad = false
    /// Counts requests of the encryption state; only the latest one's
    /// answer is kept.
    private var encryptionRequest = 0
    private var verificationRequest: Task<Void, Never>?
    private let defaults: UserDefaults
    private static let selectedSpaceKey = "selectedSpace"

    init(session: Session, defaults: UserDefaults = .standard) {
        self.session = session
        self.defaults = defaults
        images = ImageCache(session: session)
        userId = session.userId()
        deviceId = session.deviceId()
        homeserver = session.homeserver()
        // Opens on the space's rooms, until the room list says it is gone.
        if let saved = defaults.string(forKey: Self.selectedSpaceKey) {
            selectedSpace = SpaceEntry(saved: saved)
            roomsPage = .rooms
        }
    }

    /// Follow the connection, the room list and verification requests.
    func start() {
        let connection = Feed<ConnectionState>()
        subscriptions.add(session.subscribeConnection(listener: connection), connection) { [weak self] in
            self?.connection = $0
        }
        let roomList = Feed<RoomListState>()
        let session = self.session
        subscriptions.add(Task { [weak self] in
            do {
                let handle = try await session.subscribeRoomList(listener: roomList)
                self?.subscriptions.add(handle, roomList) { [weak self] state in
                    self?.apply(state)
                }
            } catch {
                self?.error = errorMessage(error)
            }
        })
        // Requests arrive while sync runs.
        let requests = Feed<Verification>(keepingAll: true)
        subscriptions.add(session.subscribeVerificationRequests(listener: requests), requests) { [weak self] in
            self?.received($0)
        }
    }

    private func apply(_ state: RoomListState) {
        rooms = state.rooms
        loaded = state.loaded
        lists = RoomLists(state)
        // The first states may be a cache: the space may come with sync.
        if let selectedSpace, state.loaded, lists.space(selectedSpace) == nil {
            selectSpace(nil)
        }
        if state.loaded && encryptionOnLoad {
            encryptionOnLoad = false
            refreshEncryption()
        }
    }

    func room(_ id: String) -> RoomSummary? {
        rooms.first { $0.id == id }
    }

    /// Sync while the app is in the foreground; when it goes to the
    /// background, stop and save the sync position before iOS suspends it.
    func setSyncing(_ on: Bool) {
        guard on != syncing else { return }
        syncing = on
        let previous = syncChange
        let session = self.session
        if on {
            syncChange = Task {
                await previous?.value
                await session.startSync()
            }
            // Back in the foreground: other devices may have verified this
            // one meanwhile. Asked again once the room list has loaded.
            encryptionOnLoad = true
            refreshEncryption()
        } else {
            let background = BackgroundTask(name: "Stop sync")
            syncChange = Task {
                await previous?.value
                await session.stopSync()
                background.end()
            }
        }
    }

    // MARK: Encryption

    /// The banner shows: encrypted messages cannot all be read until the
    /// device is verified or recovery unlocked. Not before the first sync
    /// (recovery unknown).
    var needsEncryptionSetup: Bool {
        guard let encryption, encryption.recovery != .unknown else { return false }
        return !encryption.deviceVerified || encryption.recovery == .incomplete
    }

    /// Ask the library for the encryption state again.
    func refreshEncryption() {
        encryptionRequest += 1
        let request = encryptionRequest
        let session = self.session
        Task {
            // A failure (e.g. no connection) keeps the last answer; the
            // next moment asks again.
            guard let state = try? await session.encryptionState(), request == encryptionRequest else { return }
            encryption = state
        }
    }

    /// Learn whether another device can verify this one.
    func loadDevices() async {
        guard let devices = try? await session.devices() else { return }
        hasOtherDevice = devices.contains { !$0.current }
    }

    /// Ask our other devices to verify this one, and show the verification.
    func verifyWithAnotherDevice() {
        guard !requestingVerification, verification == nil else { return }
        requestingVerification = true
        verificationError = nil
        let session = self.session
        verificationRequest = Task {
            do {
                let verification = try await session.requestSelfVerification()
                show(verification, incoming: false)
            } catch {
                verificationError = errorMessage(error)
            }
            requestingVerification = false
            verificationRequest = nil
        }
    }

    /// The verification screen closes: the verification is dropped.
    func closeVerification() {
        verification = nil
    }

    /// Another device asks to verify. Only our own devices are verified
    /// here: requests from other people are cancelled. One that comes while
    /// an earlier request is unanswered replaces it; while a verification
    /// runs, it is cancelled.
    func received(_ request: Verification) {
        let busy = verification.map { !$0.ended && !$0.asking } ?? false
        guard request.otherUser() == userId, !busy else {
            Task { try? await request.cancel() }
            return
        }
        verification?.cancel()
        show(request, incoming: true)
    }

    private func show(_ verification: Verification, incoming: Bool) {
        self.verification = VerificationModel(verification: verification, incoming: incoming) { [weak self] in
            self?.refreshEncryption()
        }
    }

    // MARK: Rooms

    /// Show `entry`'s rooms (and remember it), or the spaces with none
    /// selected.
    func selectSpace(_ entry: SpaceEntry?) {
        selectedSpace = entry
        roomsPage = entry == nil ? .spaces : .rooms
        if let entry {
            defaults.set(entry.saved, forKey: Self.selectedSpaceKey)
        } else {
            defaults.removeObject(forKey: Self.selectedSpaceKey)
        }
    }

    func timeline(for room: RoomSummary) -> TimelineModel {
        TimelineModel(session: session, room: room)
    }

    /// Sign out on the server and delete the account's data here.
    func logout() async throws {
        subscriptions.cancelAll()
        verification = nil
        try await session.logout()
        defaults.removeObject(forKey: Self.selectedSpaceKey)
    }

    /// Delete the data of a session the server no longer accepts.
    func forget() async throws {
        subscriptions.cancelAll()
        verification = nil
        try await session.forget()
        defaults.removeObject(forKey: Self.selectedSpaceKey)
    }
}

/// The pages of the Rooms tab, left to right.
enum RoomsPage: Hashable {
    case spaces
    case rooms
}

/// Time iOS grants to finish work after the app went to the background.
@MainActor
final class BackgroundTask {
    private var id = UIBackgroundTaskIdentifier.invalid

    init(name: String) {
        id = UIApplication.shared.beginBackgroundTask(withName: name) { [weak self] in
            // Time is up: called on the main thread.
            MainActor.assumeIsolated { self?.end() }
        }
    }

    func end() {
        guard id != .invalid else { return }
        UIApplication.shared.endBackgroundTask(id)
        id = .invalid
    }
}
