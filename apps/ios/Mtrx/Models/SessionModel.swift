import MtrxFFI
import UIKit

/// A signed-in account: its room list, connection and pictures, and sync,
/// which runs while the app is in the foreground.
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
    @Published private(set) var connection: ConnectionState = .online
    @Published var error: String?

    private let subscriptions = Subscriptions()
    private var syncing = false
    /// The last start or stop of sync; the next waits for it.
    private var syncChange: Task<Void, Never>?

    init(session: Session) {
        self.session = session
        images = ImageCache(session: session)
        userId = session.userId()
        deviceId = session.deviceId()
        homeserver = session.homeserver()
    }

    /// Follow the connection and the room list.
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
                    self?.rooms = state.rooms
                    self?.loaded = state.loaded
                }
            } catch {
                self?.error = errorMessage(error)
            }
        })
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
        } else {
            let background = BackgroundTask(name: "Stop sync")
            syncChange = Task {
                await previous?.value
                await session.stopSync()
                background.end()
            }
        }
    }

    func timeline(for room: RoomSummary) -> TimelineModel {
        TimelineModel(session: session, room: room)
    }

    /// Sign out on the server and delete the account's data here.
    func logout() async throws {
        subscriptions.cancelAll()
        try await session.logout()
    }

    /// Delete the data of a session the server no longer accepts.
    func forget() async throws {
        subscriptions.cancelAll()
        try await session.forget()
    }
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
