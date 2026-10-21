import MtrxFFI
import SwiftUI

/// A room: its timeline, newest at the bottom, and the composer.
struct RoomView: View {
    let room: RoomSummary
    @StateObject private var model: TimelineModel

    init(room: RoomSummary, model: @autoclosure @escaping () -> TimelineModel) {
        self.room = room
        _model = StateObject(wrappedValue: model())
    }

    var body: some View {
        VStack(spacing: 0) {
            TimelineList(model: model)
                .safeAreaInset(edge: .bottom) { OfflineBanner() }
            Divider()
            Composer(draft: $model.draft) {
                Task { await model.send() }
            }
        }
        .navigationTitle(room.name)
        .navigationBarTitleDisplayMode(.inline)
        .task { await model.open() }
        .onAppear { model.setVisible(true) }
        .onDisappear { model.setVisible(false) }
        .alert("Something went wrong", isPresented: hasError) {
            Button("OK") { model.error = nil }
        } message: {
            Text(model.error ?? "")
        }
    }

    private var hasError: Binding<Bool> {
        Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })
    }
}

/// The timeline, starting at the bottom (the newest message) and keeping
/// its place there as messages arrive; older ones load when its top comes
/// into view. iOS 17 anchors a scroll view at the bottom; on iOS 16 the
/// list and its rows are flipped upside down instead (which newer systems
/// draw wrongly, with their scroll-edge effects).
struct TimelineList: View {
    @ObservedObject var model: TimelineModel

    var body: some View {
        Group {
            if #available(iOS 17, *) {
                ScrollView {
                    rows(flipped: false)
                }
                .defaultScrollAnchor(.bottom)
            } else {
                ScrollView {
                    rows(flipped: true)
                }
                .upsideDown(true)
            }
        }
        .scrollDismissesKeyboard(.interactively)
        .overlay {
            if model.opening {
                ProgressView()
            }
        }
    }

    private func rows(flipped: Bool) -> some View {
        LazyVStack(spacing: 0) {
            if !flipped {
                top
            }
            ForEach(flipped ? Array(model.rows.reversed()) : model.rows) { row in
                TimelineRowView(row: row)
                    .upsideDown(flipped)
            }
            if flipped {
                top.upsideDown(true)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }

    /// Above the oldest row: a spinner until the room's start.
    private var top: some View {
        Group {
            if model.reachedStart || model.opening {
                Color.clear.frame(height: 1)
            } else {
                ProgressView()
                    .frame(maxWidth: .infinity, minHeight: 44)
            }
        }
        .onAppear { Task { await model.setTopVisible(true) } }
        .onDisappear { Task { await model.setTopVisible(false) } }
    }
}

/// Writing a message (Markdown).
struct Composer: View {
    @Binding var draft: String
    let send: () -> Void

    private var blank: Bool {
        draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField("Message", text: $draft, axis: .vertical)
                .lineLimit(1...6)
                .padding(.horizontal, 14)
                .padding(.vertical, 8)
                .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 18))
            Button(action: send) {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.system(size: 32))
            }
            .disabled(blank)
            .accessibilityLabel("Send")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.bar)
    }
}

extension View {
    /// Flipped vertically if `flipped`: a list and its rows both flipped
    /// show the rows in reverse order, the right way up.
    func upsideDown(_ flipped: Bool) -> some View {
        scaleEffect(x: 1, y: flipped ? -1 : 1, anchor: .center)
    }
}
