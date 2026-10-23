import MtrxFFI
import SwiftUI

/// Three tabs: Chats and Spaces, each a list that opens rooms over it (the
/// tab bar hidden then), and Settings. A verification shows over any of
/// them.
struct HomeView: View {
    @EnvironmentObject private var app: AppModel
    @EnvironmentObject private var session: SessionModel
    @EnvironmentObject private var presence: PresenceModel
    @State private var tab = HomeTab.chats

    var body: some View {
        TabView(selection: $tab) {
            ChatsTab(openSettings: openSettings)
                .tabItem { Label("Chats", systemImage: "bubble.left.and.bubble.right") }
                .tag(HomeTab.chats)
            SpacesTab(openSettings: openSettings)
                .tabItem { Label("Spaces", systemImage: "square.stack.3d.up") }
                .tag(HomeTab.spaces)
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
        tab = .settings
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

enum HomeTab: Hashable {
    case chats
    case spaces
    case settings
}

/// The direct chats, most recently active first, with their people's
/// presence.
struct ChatsTab: View {
    @EnvironmentObject private var session: SessionModel
    @EnvironmentObject private var presence: PresenceModel
    let openSettings: () -> Void
    @State private var search = ""

    var body: some View {
        NavigationStack {
            RoomList(sections: [RoomSection(title: nil, rooms: RoomLists.search(session.lists.chats, search))],
                     searching: !search.isEmpty, openSettings: openSettings,
                     presence: presence.presence(of:)) {
                ListPlaceholder(loaded: session.loaded, empty: "No chats yet")
            }
            .navigationTitle("Chats")
            .searchable(text: $search, prompt: "Search chats")
            .opensRooms()
        }
    }
}

/// The spaces on the left page, the selected space's rooms on the right,
/// like the desktop's two columns.
struct SpacesTab: View {
    @EnvironmentObject private var session: SessionModel
    let openSettings: () -> Void
    @State private var search = ""

    private var space: SpaceItem? {
        session.selectedSpace.flatMap(session.lists.space)
    }

    var body: some View {
        NavigationStack {
            TabView(selection: $session.roomsPage) {
                SpacesPage(search: search, openSettings: openSettings)
                    .tag(RoomsPage.spaces)
                SpaceRoomsPage(space: space, search: search, sort: session.roomSort, openSettings: openSettings)
                    .tag(RoomsPage.rooms)
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            .navigationTitle(session.roomsPage == .spaces ? "Spaces" : space?.name ?? "Rooms")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                if session.roomsPage == .rooms {
                    ToolbarItem(placement: .navigationBarLeading) {
                        Button {
                            withAnimation { session.roomsPage = .spaces }
                        } label: {
                            Label("Spaces", systemImage: "chevron.left")
                                .labelStyle(.titleAndIcon)
                        }
                    }
                }
                if session.roomsPage == .rooms {
                    ToolbarItem(placement: .navigationBarTrailing) {
                        SortMenu()
                    }
                }
            }
            .searchable(text: $search, prompt: session.roomsPage == .spaces ? "Search spaces" : "Search rooms")
            .onChange(of: session.roomsPage) { _ in search = "" }
            .opensRooms()
        }
    }
}

/// The top-level spaces, then Other rooms; a tap shows the entry's rooms.
private struct SpacesPage: View {
    @EnvironmentObject private var session: SessionModel
    let search: String
    let openSettings: () -> Void

    var body: some View {
        List {
            if search.isEmpty {
                EncryptionBannerRow(open: openSettings)
            }
            ForEach(RoomLists.search(session.lists.spaces, search)) { item in
                Button {
                    withAnimation { session.selectSpace(item.entry) }
                } label: {
                    SpaceRow(item: item)
                }
                .listRowBackground(item.entry == session.selectedSpace ? Color.accentColor.opacity(0.12) : nil)
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

/// The selected space's rooms, then its direct chats, in the chosen order.
private struct SpaceRoomsPage: View {
    @EnvironmentObject private var session: SessionModel
    let space: SpaceItem?
    let search: String
    let sort: RoomSort
    let openSettings: () -> Void

    var body: some View {
        RoomList(sections: RoomLists.sections(space?.rooms ?? [], sort: sort, search: search),
                 searching: !search.isEmpty, openSettings: openSettings) {
            if space != nil {
                Text("No rooms in this space").foregroundStyle(.secondary)
            } else if session.selectedSpace != nil && !session.loaded {
                // The remembered space may come with sync.
                ProgressView("Loading…")
            } else {
                Text("Choose a space").foregroundStyle(.secondary)
            }
        }
    }
}

/// A space's picture and name, with a dot while any of its rooms has unread
/// messages.
private struct SpaceRow: View {
    let item: SpaceItem

    var body: some View {
        HStack(spacing: 12) {
            if let space = item.space {
                AvatarView(name: space.name, uri: space.avatarUrl, size: 44, shape: .square)
            } else {
                Image(systemName: "tray.full")
                    .font(.title3)
                    .foregroundStyle(.secondary)
                    .frame(width: 44, height: 44)
                    .background(Color.secondary.opacity(0.15), in: RoundedRectangle(cornerRadius: 10))
            }
            Text(item.name)
                .fontWeight(item.unread ? .semibold : .regular)
                .foregroundColor(.primary)
                .lineLimit(1)
            Spacer(minLength: 8)
            if item.unread {
                Circle()
                    .fill(Color.accentColor)
                    .frame(width: 10, height: 10)
                    .accessibilityLabel("Unread")
            }
        }
        .padding(.vertical, 4)
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

/// Rooms or chats, in sections with their titles, that open on a tap;
/// invites, answered on the desktop, do not. The encryption banner above
/// them, unless searching.
struct RoomList<Empty: View>: View {
    let sections: [RoomSection]
    let searching: Bool
    let openSettings: () -> Void
    /// The presence to show on a room's picture (Chats).
    var presence: (RoomSummary) -> Presence? = { _ in nil }
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
            if room.invited {
                RoomRow(room: room, presence: presence(room))
            } else {
                NavigationLink(value: room.id) {
                    RoomRow(room: room, presence: presence(room))
                }
            }
        }
    }
}

/// Before the room list loads, or when a loaded list is empty.
private struct ListPlaceholder: View {
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
    /// Rooms open over the list, by ID, without the tab bar; the back
    /// gesture returns to the list as it was.
    func opensRooms() -> some View {
        navigationDestination(for: String.self) { id in
            OpenRoom(id: id)
                .toolbar(.hidden, for: .tabBar)
        }
    }
}

private struct OpenRoom: View {
    @EnvironmentObject private var session: SessionModel
    let id: String

    var body: some View {
        if let room = session.room(id) {
            RoomView(room: room, model: session.timeline(for: room))
        } else {
            Text("The room is no longer in the room list.")
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
