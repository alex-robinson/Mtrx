import MtrxFFI
import SwiftUI

/// Four tabs: Chats (direct chats), Spaces (the spaces, each with its
/// rooms), Rooms (the last room, over the rooms opened lately) and Settings.
/// Rooms open over their list, without the tab bar. A verification and an
/// invite to answer show over any of them.
struct HomeView: View {
    @EnvironmentObject private var app: AppModel
    @EnvironmentObject private var session: SessionModel
    @EnvironmentObject private var presence: PresenceModel

    var body: some View {
        TabView(selection: Binding(get: { session.tab }, set: session.select)) {
            ChatsTab(openSettings: openSettings)
                .tabItem { Label("Chats", systemImage: "bubble.left.and.bubble.right") }
                .tag(HomeTab.chats)
            SpacesTab(openSettings: openSettings)
                .tabItem { Label("Spaces", systemImage: "square.stack.3d.up") }
                .badge(session.spacesBadge)
                .tag(HomeTab.spaces)
            RoomsTab()
                .tabItem { Label("Rooms", systemImage: "number") }
                .badge(session.roomsBadge)
                .tag(HomeTab.rooms)
            SettingsView()
                .tabItem { Label("Settings", systemImage: "gearshape") }
                .tag(HomeTab.settings)
        }
        // Input anywhere in the app: not idle.
        .background(InputWatcher(onInput: presence.input))
        // Closed by its own buttons, which end the verification.
        .sheet(item: verification) { model in
            VerificationView(model: model)
                .environmentObject(session)
        }
        .sheet(item: $session.invite) { invite in
            if let room = session.room(invite.id) {
                InviteView(room: room)
                    .environmentObject(session)
                    .environmentObject(session.images)
            }
        }
        .alert("Signed out", isPresented: signedOutElsewhere) {
            Button("Sign in again") {
                Task { await app.forgetSession() }
            }
        } message: {
            Text("The server no longer accepts this session, e.g. because it was signed out on another device.")
        }
        .alert("Something went wrong", isPresented: hasError) {
            Button("OK") { session.error = nil }
        } message: {
            Text(session.error ?? "")
        }
    }

    /// The encryption banner leads to Settings.
    private func openSettings() {
        session.tab = .settings
    }

    private var verification: Binding<VerificationModel?> {
        Binding(get: { session.verification }, set: { _ in })
    }

    private var signedOutElsewhere: Binding<Bool> {
        Binding(get: { session.connection == .signedOut }, set: { _ in })
    }

    private var hasError: Binding<Bool> {
        Binding(get: { session.error != nil }, set: { if !$0 { session.error = nil } })
    }
}

/// The direct chats, most recently active first, with their people's
/// presence.
struct ChatsTab: View {
    @EnvironmentObject private var session: SessionModel
    @EnvironmentObject private var presence: PresenceModel
    let openSettings: () -> Void
    @State private var search = ""

    var body: some View {
        NavigationStack(path: $session.chatsPath) {
            RoomList(sections: [RoomSection(title: nil, rooms: RoomLists.search(session.lists.chats, search))],
                     searching: !search.isEmpty, openSettings: openSettings,
                     presence: presence.presence(of:)) {
                ListPlaceholder(loaded: session.loaded, empty: "No chats yet")
            }
            .navigationTitle("Chats")
            .searchable(text: $search, prompt: "Search chats")
            .navigationDestination(for: String.self) { id in
                OpenRoom(id: id)
                    .toolbar(.hidden, for: .tabBar)
            }
        }
    }
}

/// What the Spaces and Rooms tabs open over themselves.
enum SpacesSheet: Identifiable {
    case newSpace
    /// A new room in the space (ID and name), if any.
    case newRoom(parent: String?, name: String?)
    case join

    var id: String {
        switch self {
        case .newSpace: return "new space"
        case .newRoom(let parent, _): return "new room \(parent ?? "")"
        case .join: return "join"
        }
    }
}

/// The spaces, then Other rooms; a tap shows the entry's rooms, with the
/// space's name on top (a tap on it shows its overview). A room opens in
/// the Rooms tab.
struct SpacesTab: View {
    @EnvironmentObject private var session: SessionModel
    let openSettings: () -> Void
    @State private var search = ""
    @State private var sheet: SpacesSheet?

    var body: some View {
        NavigationStack(path: $session.spacesPath) {
            SpacesList(search: search, openSettings: openSettings)
                .navigationTitle("Spaces")
                .searchable(text: $search, prompt: "Search spaces")
                .toolbar {
                    ToolbarItem(placement: .navigationBarTrailing) {
                        AddMenu(space: nil, sheet: $sheet)
                    }
                }
                .navigationDestination(for: SpaceEntry.self) { entry in
                    SpaceRoomsPage(entry: entry, sheet: $sheet, openSettings: openSettings)
                }
        }
        .sheet(item: $sheet) { sheet in
            SpacesSheetView(sheet: sheet)
                .environmentObject(session)
                .environmentObject(session.images)
        }
    }
}

/// The `+` of the Spaces tab: New space…, Join a space or room…, and in a
/// space New room in it….
private struct AddMenu: View {
    /// The space shown, if any.
    let space: RoomSummary?
    @Binding var sheet: SpacesSheet?

    var body: some View {
        Menu {
            if let space {
                Button {
                    sheet = .newRoom(parent: space.id, name: space.name)
                } label: {
                    Label("New room in \(space.name)…", systemImage: "plus.bubble")
                }
            }
            Button {
                sheet = .newSpace
            } label: {
                Label("New space…", systemImage: "square.stack.3d.up")
            }
            Button {
                sheet = .join
            } label: {
                Label("Join a space or room…", systemImage: "magnifyingglass")
            }
        } label: {
            Image(systemName: "plus")
        }
        .accessibilityLabel("Add")
    }
}

/// A sheet of the Spaces tab, with what it made or joined opened after.
private struct SpacesSheetView: View {
    @EnvironmentObject private var session: SessionModel
    let sheet: SpacesSheet

    var body: some View {
        switch sheet {
        case .newSpace:
            NewRoomView(model: NewRoomModel(session: session.session, space: true))
        case .newRoom(let parent, let name):
            NewRoomView(
                model: NewRoomModel(
                    session: session.session, space: false,
                    parent: parent.flatMap { id in name.map { (id: id, name: $0) } }))
        case .join:
            JoinView(model: JoinModel(session: session.session, ownServer: session.serverName))
        }
    }
}

/// The top-level spaces, then Other rooms; a tap shows the entry's rooms.
struct SpacesList: View {
    @EnvironmentObject private var session: SessionModel
    let search: String
    let openSettings: () -> Void

    var body: some View {
        List {
            if search.isEmpty {
                EncryptionBannerRow(open: openSettings)
            }
            ForEach(RoomLists.search(session.lists.spaces, search)) { item in
                NavigationLink(value: item.entry) {
                    SpaceRow(item: item)
                }
                .compactRow()
            }
        }
        .listStyle(.plain)
        .overlay {
            if session.lists.spaces.isEmpty {
                ListPlaceholder(loaded: session.loaded, empty: "No rooms yet")
            }
        }
        .safeAreaInset(edge: .bottom) { OfflineBanner() }
    }
}

/// A space's rooms, then its direct chats, in the chosen order, with the
/// space's name on top: a tap on it shows the space's overview.
struct SpaceRoomsPage: View {
    @EnvironmentObject private var session: SessionModel
    let entry: SpaceEntry
    @Binding var sheet: SpacesSheet?
    let openSettings: () -> Void
    @State private var search = ""
    @State private var overview = false

    private var item: SpaceItem? { session.lists.space(entry) }

    var body: some View {
        RoomList(sections: RoomLists.sections(item?.rooms ?? [], sort: session.roomSort, search: search),
                 searching: !search.isEmpty, openSettings: openSettings, opensInRooms: true) {
            if item != nil {
                Text("No rooms in this space").foregroundStyle(.secondary)
            } else {
                // A remembered or new space may come with sync.
                ProgressView("Loading…")
            }
        }
        .searchable(text: $search, prompt: "Search rooms")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                if let space = item?.space {
                    Button {
                        overview = true
                    } label: {
                        HStack(spacing: 6) {
                            AvatarView(name: space.name, uri: space.avatarUrl, size: 22, shape: .square)
                            Text(space.name)
                                .font(.headline)
                                .foregroundColor(.primary)
                                .lineLimit(1)
                            Image(systemName: "chevron.right")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                        }
                    }
                    .accessibilityHint("Shows the space's overview")
                } else {
                    Text(item?.name ?? "").font(.headline)
                }
            }
            ToolbarItemGroup(placement: .navigationBarTrailing) {
                SortMenu()
                AddMenu(space: item?.space, sheet: $sheet)
            }
        }
        .fullScreenCover(isPresented: $overview) {
            if let space = item?.space {
                SpaceOverviewView(space: space)
                    .environmentObject(session)
                    .environmentObject(session.images)
            }
        }
    }
}

/// A space's picture and name, with a dot while any of its rooms has unread
/// messages.
private struct SpaceRow: View {
    let item: SpaceItem

    var body: some View {
        HStack(spacing: 10) {
            if let space = item.space {
                AvatarView(name: space.name, uri: space.avatarUrl, size: RoomRow.avatarSize, shape: .square)
            } else {
                Image(systemName: "tray.full")
                    .font(.body)
                    .foregroundStyle(.secondary)
                    .frame(width: RoomRow.avatarSize, height: RoomRow.avatarSize)
                    .background(Color.secondary.opacity(0.15), in: RoundedRectangle(cornerRadius: 8))
            }
            Text(item.name)
                .fontWeight(item.unread ? .semibold : .regular)
                .foregroundColor(.primary)
                .lineLimit(1)
            Spacer(minLength: 8)
            if item.unread {
                Circle()
                    .fill(Color.accentColor)
                    .frame(width: 9, height: 9)
                    .accessibilityLabel("Unread")
            }
        }
        .contentShape(Rectangle())
    }
}

/// The order of the spaces' rooms: a menu of the choices, the current one
/// checked.
private struct SortMenu: View {
    @EnvironmentObject private var session: SessionModel

    var body: some View {
        Menu {
            Picker("Sort", selection: Binding(get: { session.roomSort }, set: session.setRoomSort)) {
                ForEach(RoomSort.allCases) { sort in
                    Text(sort.title).tag(sort)
                }
            }
        } label: {
            Image(systemName: "arrow.up.arrow.down")
        }
        .accessibilityLabel("Sort")
    }
}

/// The Rooms tab: the last room, over Recent; back (or a tap on the tab)
/// goes to Recent.
struct RoomsTab: View {
    @EnvironmentObject private var session: SessionModel

    var body: some View {
        NavigationStack(path: $session.roomsPath) {
            RecentView()
                .navigationTitle("Recent")
                .navigationDestination(for: String.self) { id in
                    OpenRoom(id: id)
                        .backWithUnread(session.backBadge(besides: id)) {
                            session.roomsPath.removeAll { $0 == id }
                        }
                        .toolbar(.hidden, for: .tabBar)
                }
        }
    }
}

/// The rooms opened lately, across all spaces, newest first: each with its
/// space and unread count.
struct RecentView: View {
    @EnvironmentObject private var session: SessionModel

    var body: some View {
        let rooms = session.recent.rooms(in: session.lists)
        List {
            ForEach(rooms, id: \.id) { room in
                NavigationLink(value: room.id) {
                    RoomRow(room: room, detail: session.lists.spaceName(of: room))
                }
                .compactRow()
            }
        }
        .listStyle(.plain)
        .overlay {
            if rooms.isEmpty {
                VStack(spacing: 12) {
                    Text("Rooms you open show here")
                        .foregroundStyle(.secondary)
                    Button("Browse spaces") {
                        session.tab = .spaces
                    }
                    .buttonStyle(.bordered)
                }
            }
        }
        .safeAreaInset(edge: .bottom) { OfflineBanner() }
    }
}

/// Rooms or chats, in sections with their titles, that open on a tap (in
/// their tabs if `opensInRooms`: rooms in Rooms, chats in Chats; else over
/// the list); an invite opens its answer. The encryption banner above
/// them, unless searching.
struct RoomList<Empty: View>: View {
    @EnvironmentObject private var session: SessionModel
    let sections: [RoomSection]
    let searching: Bool
    let openSettings: () -> Void
    /// The presence to show on a room's picture (Chats).
    var presence: (RoomSummary) -> Presence? = { _ in nil }
    /// Rooms open in their tabs (from a space's list).
    var opensInRooms = false
    @ViewBuilder let empty: () -> Empty

    var body: some View {
        List {
            if !searching {
                EncryptionBannerRow(open: openSettings)
            }
            ForEach(sections) { section in
                if let title = section.title {
                    Section(title) { rows(section.rooms) }
                } else {
                    rows(section.rooms)
                }
            }
        }
        .listStyle(.plain)
        .overlay {
            if sections.allSatisfy(\.rooms.isEmpty) && !searching {
                empty()
            }
        }
        .safeAreaInset(edge: .bottom) { OfflineBanner() }
    }

    private func rows(_ rooms: [RoomSummary]) -> some View {
        ForEach(rooms, id: \.id) { room in
            Group {
                if room.invited {
                    Button {
                        session.invite = RoomRef(id: room.id)
                    } label: {
                        RoomRow(room: room, presence: presence(room))
                    }
                    .foregroundColor(.primary)
                } else if opensInRooms {
                    Button {
                        session.open(room.id)
                    } label: {
                        RoomRow(room: room, presence: presence(room))
                    }
                    .foregroundColor(.primary)
                } else {
                    NavigationLink(value: room.id) {
                        RoomRow(room: room, presence: presence(room))
                    }
                }
            }
            .compactRow()
        }
    }
}

/// Before the room list loads, or when a loaded list is empty.
struct ListPlaceholder: View {
    let loaded: Bool
    let empty: String

    var body: some View {
        if loaded {
            Text(empty).foregroundStyle(.secondary)
        } else {
            ProgressView("Loading…")
        }
    }
}

extension View {
    /// A list row with less space around it than the default.
    func compactRow() -> some View {
        listRowInsets(EdgeInsets(top: 5, leading: 14, bottom: 5, trailing: 14))
    }

    /// A back button with a number: unread messages elsewhere.
    func backWithUnread(_ count: Int, back: @escaping () -> Void) -> some View {
        modifier(BackWithUnread(count: count, back: back))
    }
}

/// The back button of a room in the Rooms tab: a chevron and the unread
/// messages in the other recent rooms. Swiping from the edge still goes
/// back (`UINavigationController` extension).
private struct BackWithUnread: ViewModifier {
    let count: Int
    let back: () -> Void

    func body(content: Content) -> some View {
        content
            .navigationBarBackButtonHidden(true)
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Button(action: back) {
                        HStack(spacing: 4) {
                            Image(systemName: "chevron.left")
                                .font(.body.weight(.semibold))
                            if count > 0 {
                                Text(count > 999 ? "999+" : String(count))
                                    .font(.caption.weight(.semibold))
                                    .monospacedDigit()
                                    .foregroundColor(.white)
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 1)
                                    .background(Color.accentColor, in: Capsule())
                            }
                        }
                    }
                    .accessibilityLabel(count > 0 ? "Recent, \(count) unread" : "Recent")
                }
            }
    }
}

/// A room by ID: its invite to answer, its timeline, or a word while it is
/// coming (just made or joined) or gone.
struct OpenRoom: View {
    @EnvironmentObject private var session: SessionModel
    let id: String

    var body: some View {
        if let room = session.room(id), !room.invited {
            RoomView(room: room, model: session.timeline(for: room), info: session.info(for: room))
        } else if let room = session.room(id) {
            InviteView(room: room)
        } else if session.expecting.contains(id) || !session.loaded {
            ProgressView("Loading…")
        } else {
            Text("This room is not in your list.")
                .foregroundStyle(.secondary)
        }
    }
}

/// The encryption banner as the first row of a list, while it shows.
private struct EncryptionBannerRow: View {
    @EnvironmentObject private var session: SessionModel
    let open: () -> Void

    var body: some View {
        if session.needsEncryptionSetup {
            EncryptionBanner(open: open)
                .listRowSeparator(.hidden)
                .listRowInsets(EdgeInsets(top: 6, leading: 12, bottom: 6, trailing: 12))
        }
    }
}

/// While this device cannot read all encrypted messages; a tap opens the
/// encryption settings.
struct EncryptionBanner: View {
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 10) {
                Image(systemName: "lock.trianglebadge.exclamationmark")
                    .font(.title3)
                Text("Verify this device or enter your recovery key to read encrypted messages.")
                    .font(.footnote)
                    .multilineTextAlignment(.leading)
                Spacer(minLength: 0)
                Image(systemName: "chevron.right")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            .padding(12)
            .background(Color.orange.opacity(0.15), in: RoundedRectangle(cornerRadius: 12))
        }
        .buttonStyle(.plain)
    }
}

/// While the server is unreachable.
struct OfflineBanner: View {
    @EnvironmentObject private var session: SessionModel

    var body: some View {
        if session.connection == .offline {
            Label("Offline – reconnecting…", systemImage: "wifi.slash")
                .font(.footnote)
                .padding(.horizontal, 14)
                .padding(.vertical, 8)
                .background(.regularMaterial, in: Capsule())
                .padding(.bottom, 8)
        }
    }
}
