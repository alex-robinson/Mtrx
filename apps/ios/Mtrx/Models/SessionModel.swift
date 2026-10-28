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
    /// The Spaces tab's selected space, whose rooms it shows, remembered
    /// across launches (until sign-out).
    @Published private(set) var selectedSpace: SpaceEntry?
    /// The tab shown.
    @Published var tab = HomeTab.chats
    /// The chats opened over the Chats tab's list.
    @Published var chatsPath: [String] = []
    /// The rooms opened over the Rooms tab's Recent list: at launch, the
    /// last room.
    @Published var roomsPath: [String] = []
    /// The rooms opened lately, remembered across launches (until
    /// sign-out).
    @Published private(set) var recent: RecentRooms
    /// An invite to answer, while its sheet shows.
    @Published var invite: RoomRef?
    /// Rooms and spaces just made or joined here, not in the room list
    /// yet: shown as coming rather than gone.
    @Published private(set) var expecting: Set<String> = []
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
    /// What was typed in each room and not sent.
    let drafts = Drafts()
    /// The spaces' rooms as their servers last listed them, for the
    /// overview to show at once.
    let spaceChildren = SpaceChildrenCache()

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
    private static let recentKey = "recentRooms"

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
        }
        recent = RecentRooms(defaults.stringArray(forKey: Self.recentKey) ?? [])
        // The Rooms tab reopens the last room.
        roomsPath = recent.last.map { [$0] } ?? []
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
        expecting = expecting.filter { lists.byId[$0]?.invited ?? true }
        guard state.loaded else { return }
        // The first states may be a cache: rooms may come with sync.
        presence.follow(lists.chats)
        if let selectedSpace, lists.space(selectedSpace) == nil, !isExpected(selectedSpace) {
            selectSpace(nil)
        }
        var kept = recent
        kept.keep(Set(lists.byId.keys).union(expecting))
        setRecent(kept)
    }

    private func isExpected(_ entry: SpaceEntry) -> Bool {
        if case .space(let id) = entry { return expecting.contains(id) }
        return false
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
        if let entry {
            defaults.set(entry.saved, forKey: Self.selectedSpaceKey)
        } else {
            defaults.removeObject(forKey: Self.selectedSpaceKey)
        }
    }

    /// The Spaces tab's pages over its list: the selected space's rooms.
    var spacesPath: [SpaceEntry] {
        get { selectedSpace.map { [$0] } ?? [] }
        set { selectSpace(newValue.last) }
    }

    /// A tab was tapped; tapped again, it goes back to its list (the Rooms
    /// tab: to Recent).
    func select(_ tab: HomeTab) {
        guard tab == self.tab else {
            self.tab = tab
            return
        }
        switch tab {
        case .chats: chatsPath = []
        case .spaces: selectSpace(nil)
        case .rooms: roomsPath = []
        case .settings: break
        }
    }

    /// Open a room (or a direct chat) where it belongs: a room in the Rooms
    /// tab, over Recent; a chat in the Chats tab; a space's rooms in the
    /// Spaces tab. `kind` for one not in the room list yet.
    func open(_ id: String, kind: RoomKind? = nil) {
        switch kind ?? lists.byId[id]?.kind {
        case .direct:
            chatsPath = [id]
            tab = .chats
        case .space:
            selectSpace(.space(id))
            tab = .spaces
        case .room, nil:
            roomsPath = [id]
            tab = .rooms
        }
    }

    /// A room was opened (shown): it is the last room now, first in Recent.
    /// Not chats or spaces.
    func opened(_ room: RoomSummary) {
        guard room.kind == .room, !room.invited else { return }
        var opened = recent
        opened.opened(room.id)
        setRecent(opened)
    }

    /// We left a room, or declined an invite: it goes from where it shows.
    func left(_ id: String) {
        var kept = recent
        kept.remove(id)
        setRecent(kept)
        roomsPath.removeAll { $0 == id }
        chatsPath.removeAll { $0 == id }
        if selectedSpace == .space(id) {
            selectSpace(nil)
        }
        expecting.remove(id)
    }

    /// A room or space made or joined here: shown as coming until the room
    /// list has it.
    func expect(_ id: String) {
        expecting.insert(id)
    }

    private func setRecent(_ recent: RecentRooms) {
        guard recent != self.recent else { return }
        self.recent = recent
        defaults.set(recent.ids, forKey: Self.recentKey)
    }

    /// Accept an invite: join, then open what we joined.
    func accept(_ room: RoomSummary) async throws {
        _ = try await session.join(room: room.id, via: [])
        expect(room.id)
        open(room.id)
    }

    /// Decline an invite.
    func decline(_ room: RoomSummary) async throws {
        try await session.leave(roomId: room.id)
        left(room.id)
    }

    /// Our server's name, as in our user ID: "example.org".
    var serverName: String {
        userId.split(separator: ":", maxSplits: 1).last.map(String.init) ?? homeserver
    }

    /// The Rooms tab's badge: unread messages in the recent rooms.
    var roomsBadge: Int { recent.unread(in: lists) }

    /// The Spaces tab's badge: unread messages in the other rooms.
    var spacesBadge: Int { recent.unreadElsewhere(in: lists) }

    /// The number on a room's back button: unread messages in the other
    /// recent rooms.
    func backBadge(besides id: String) -> Int {
        recent.unread(in: lists, besides: id)
    }

    /// Order the spaces' rooms by `sort`, from now on.
    func setRoomSort(_ sort: RoomSort) {
        roomSort = sort
        defaults.set(sort.rawValue, forKey: Self.roomSortKey)
    }

    func timeline(for room: RoomSummary) -> TimelineModel {
        TimelineModel(session: session, room: room, keysArriving: keysArriving, drafts: drafts)
    }

    func info(for room: RoomSummary) -> RoomInfoModel {
        RoomInfoModel(session: session, roomId: room.id, userId: userId)
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
        forgetPlaces()
    }

    /// Delete the data of a session the server no longer accepts.
    func forget() async throws {
        subscriptions.cancelAll()
        verification = nil
        // The server no longer accepts the session: nothing to set.
        presence.forget()
        try await session.forget()
        forgetPlaces()
    }

    /// The selected space and the recent rooms are the account's.
    private func forgetPlaces() {
        defaults.removeObject(forKey: Self.selectedSpaceKey)
        defaults.removeObject(forKey: Self.recentKey)
    }
}

/// The tabs at the bottom.
enum HomeTab: Hashable {
    /// Direct chats.
    case chats
    /// The spaces and their rooms.
    case spaces
    /// The last room, over the rooms opened lately.
    case rooms
    case settings
}

/// A room by ID, to present a sheet for.
struct RoomRef: Identifiable, Hashable {
    let id: String
}

/// The spaces' rooms as their servers last listed them, by space ID.
@MainActor
final class SpaceChildrenCache {
    private var rooms: [String: [DirectoryRoom]] = [:]

    subscript(spaceId: String) -> [DirectoryRoom]? {
        get { rooms[spaceId] }
        set { rooms[spaceId] = newValue }
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
