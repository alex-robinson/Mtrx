import Combine
import MtrxFFI
import UIKit

/// A signed-in account: its room lists, connection, pictures and presence,
/// sync, which runs while the app is in the foreground, and whether this
/// device can read encrypted messages.
@MainActor
final class SessionModel: ObservableObject {
    let session: Session
    let images: ImageCache
    let presence: PresenceModel
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
    /// The Spaces tab's selected space, remembered across launches (until
    /// sign-out).
    @Published private(set) var selectedSpace: SpaceEntry?
    /// The Spaces tab's page: the spaces, or the selected space's rooms.
    @Published var roomsPage = RoomsPage.spaces
    /// The order of every space's rooms (not of the chats), remembered
    /// across launches.
    @Published private(set) var roomSort: RoomSort
    @Published private(set) var connection: ConnectionState = .online
    @Published var error: String?

    /// Whether this device is verified, the account's recovery, and
    /// whether message keys are arriving, as the library pushes it; nil
    /// until its first update.
    @Published private(set) var encryption: EncryptionState?
    /// Whether another of our devices is signed in (to verify with); nil
    /// until known.
    @Published private(set) var hasOtherDevice: Bool?
    /// The verification on screen, asked for here or by another device.
    @Published private(set) var verification: VerificationModel?
    @Published private(set) var requestingVerification = false
    /// Why asking another device to verify failed.
    @Published var verificationError: String?
    private(set) lazy var recovery = RecoveryModel(session: session)

    private let subscriptions = Subscriptions()
    /// The app is in the foreground: sync runs.
    private var foreground = false
    /// Sync has started once.
    private var started = false
    /// The last start or stop of sync; the next waits for it.
    private var syncChange: Task<Void, Never>?
    private var verificationRequest: Task<Void, Never>?
    private let defaults: UserDefaults
    private static let selectedSpaceKey = "selectedSpace"
    private static let roomSortKey = "roomSort"

    init(session: Session, defaults: UserDefaults = .standard, idleTime: TimeInterval = PresenceModel.idleTime) {
        self.session = session
        self.defaults = defaults
        images = ImageCache(session: session)
        presence = PresenceModel(session: session, defaults: defaults, idleTime: idleTime)
        userId = session.userId()
        deviceId = session.deviceId()
        homeserver = session.homeserver()
        roomSort = defaults.string(forKey: Self.roomSortKey).flatMap(RoomSort.init(rawValue:)) ?? .listOrder
        // Opens on the space's rooms, until the room list says it is gone.
        if let saved = defaults.string(forKey: Self.selectedSpaceKey) {
            selectedSpace = SpaceEntry(saved: saved)
            roomsPage = .rooms
        }
    }

    /// Follow the connection, the room list, the encryption state and
    /// verification requests, for the session (sync starts right after).
    /// The encryption state is pushed now and on every change: its recovery
    /// is `unknown` until sync has run.
    func start() {
        let connection = Feed<ConnectionState>()
        subscriptions.add(session.subscribeConnection(listener: connection), connection) { [weak self] in
            self?.connection = $0
        }
        let encryption = Feed<EncryptionState>()
        subscriptions.add(session.subscribeEncryptionState(listener: encryption), encryption) { [weak self] in
            self?.encryption = $0
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
        if state.loaded {
            presence.follow(lists.chats)
        }
        // The first states may be a cache: the space may come with sync.
        if let selectedSpace, state.loaded, lists.space(selectedSpace) == nil {
            selectSpace(nil)
        }
    }

    func room(_ id: String) -> RoomSummary? {
        rooms.first { $0.id == id }
    }

    /// Sync while the app is in the foreground, and set our presence;
    /// when it goes to the background, go offline, then stop and save the
    /// sync position, before iOS suspends it. Not sharing our presence,
    /// offline before sync first starts.
    func setForeground(_ on: Bool) {
        guard on != foreground else { return }
        foreground = on
        let previous = syncChange
        let session = self.session
        if on {
            let offline = presence.enterForeground(first: !started)
            started = true
            syncChange = Task {
                await previous?.value
                await offline?.value
                await session.startSync()
            }
        } else {
            let background = BackgroundTask(name: "Stop sync")
            let offline = presence.enterBackground()
            syncChange = Task {
                await previous?.value
                await offline?.value
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
        self.verification = VerificationModel(verification: verification, incoming: incoming)
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

    /// Order the spaces' rooms by `sort`, from now on.
    func setRoomSort(_ sort: RoomSort) {
        roomSort = sort
        defaults.set(sort.rawValue, forKey: Self.roomSortKey)
    }

    func timeline(for room: RoomSummary) -> TimelineModel {
        TimelineModel(session: session, room: room, keysArriving: keysArriving)
    }

    /// Message keys are arriving from the backup, now and on every change:
    /// undecryptable messages say they are being decrypted meanwhile.
    var keysArriving: AnyPublisher<Bool, Never> {
        $encryption.map { $0?.keysArriving ?? false }.removeDuplicates().eraseToAnyPublisher()
    }

    /// Sign out on the server and delete the account's data here.
    func logout() async throws {
        subscriptions.cancelAll()
        verification = nil
        await presence.signOut()?.value
        try await session.logout()
        defaults.removeObject(forKey: Self.selectedSpaceKey)
    }

    /// Delete the data of a session the server no longer accepts.
    func forget() async throws {
        subscriptions.cancelAll()
        verification = nil
        // The server no longer accepts the session: nothing to set.
        presence.forget()
        try await session.forget()
        defaults.removeObject(forKey: Self.selectedSpaceKey)
    }
}

/// The pages of the Spaces tab, left to right.
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
