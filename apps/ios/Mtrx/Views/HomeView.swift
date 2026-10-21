import MtrxFFI
import SwiftUI

/// The room list and the open room: side by side on iPad, one after the
/// other on iPhone.
struct HomeView: View {
    @EnvironmentObject private var app: AppModel
    @EnvironmentObject private var session: SessionModel
    @State private var selection: String?
    @State private var showSettings = false

    var body: some View {
        NavigationSplitView {
            RoomListView(selection: $selection)
                .navigationTitle("Mtrx")
                .toolbar {
                    ToolbarItem(placement: .navigationBarTrailing) {
                        Button {
                            showSettings = true
                        } label: {
                            Image(systemName: "person.crop.circle")
                        }
                        .accessibilityLabel("Settings")
                    }
                }
        } detail: {
            if let id = selection, let room = session.room(id) {
                RoomView(room: room, model: session.timeline(for: room))
                    .id(id)
            } else {
                Text("Choose a room")
                    .foregroundStyle(.secondary)
            }
        }
        .sheet(isPresented: $showSettings) {
            SettingsView()
                .environmentObject(app)
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

    private var signedOutElsewhere: Binding<Bool> {
        Binding(get: { session.connection == .signedOut }, set: { _ in })
    }

    private var hasError: Binding<Bool> {
        Binding(get: { session.error != nil }, set: { if !$0 { session.error = nil } })
    }
}

/// Rooms by section, or the rooms found by a search.
struct RoomListView: View {
    @EnvironmentObject private var session: SessionModel
    @Binding var selection: String?
    @State private var search = ""

    var body: some View {
        List(selection: $selection) {
            if search.isEmpty {
                ForEach(RoomSections.group(session.rooms)) { group in
                    Section(group.section.title) {
                        ForEach(group.rooms, id: \.id, content: entry)
                    }
                }
            } else {
                ForEach(RoomSections.search(session.rooms, search), id: \.id, content: entry)
            }
        }
        .listStyle(.plain)
        .searchable(text: $search, prompt: "Search rooms")
        .overlay {
            if session.rooms.isEmpty {
                if session.loaded {
                    Text("No rooms yet").foregroundStyle(.secondary)
                } else {
                    ProgressView("Loading…")
                }
            }
        }
        .safeAreaInset(edge: .bottom) {
            OfflineBanner()
        }
    }

    /// Invites cannot be opened here: they are answered on the desktop.
    @ViewBuilder
    private func entry(_ room: RoomSummary) -> some View {
        if room.invited {
            RoomRow(room: room)
        } else {
            NavigationLink(value: room.id) {
                RoomRow(room: room)
            }
        }
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
