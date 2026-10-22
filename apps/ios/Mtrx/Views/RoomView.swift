import MtrxFFI
import SwiftUI

/// A room: its timeline, newest at the bottom, who types, and the
/// composer.
struct RoomView: View {
    let room: RoomSummary
    @StateObject private var model: TimelineModel
    @EnvironmentObject private var images: ImageCache
    @State private var sheet: RoomSheet?
    /// The message to delete, once confirmed.
    @State private var deleting: String?

    init(room: RoomSummary, model: @autoclosure @escaping () -> TimelineModel) {
        self.room = room
        _model = StateObject(wrappedValue: model())
    }

    var body: some View {
        VStack(spacing: 0) {
            TimelineList(model: model, actions: actions)
                .safeAreaInset(edge: .bottom) { OfflineBanner() }
            TypingLine(text: TimelineModel.typingLine(model.typing))
            Divider()
            Composer(draft: $model.draft, compose: model.compose, cancel: model.cancelCompose) {
                Task { await model.send() }
            }
        }
        .navigationTitle(room.name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if room.kind == .direct {
                ToolbarItem(placement: .principal) {
                    PersonHeader(room: room, open: model.showPersonInfo)
                }
            }
        }
        .task { await model.open() }
        .onAppear { model.setVisible(true) }
        .onDisappear { model.setVisible(false) }
        .sheet(item: $sheet) { sheet in
            RoomSheetView(sheet: sheet, model: model)
                .environmentObject(images)
        }
        .confirmationDialog("Delete this message for everyone?", isPresented: isDeleting, titleVisibility: .visible) {
            Button("Delete", role: .destructive) {
                if let deleting { model.delete(deleting) }
            }
        }
        .alert("Something went wrong", isPresented: hasError) {
            Button("OK") { model.error = nil }
        } message: {
            Text(model.error ?? "")
        }
    }

    private var actions: RowActions {
        RowActions(
            perform: perform,
            react: { id, key in model.react(id, key: key) },
            goTo: { quote in Task { await model.goTo(eventId: quote.eventId) } },
            showReactors: { id, key in sheet = .reactors(itemId: id, key: key) },
            showReaders: { id in sheet = .readers(itemId: id) })
    }

    private func perform(_ action: MessageAction, on id: String) {
        switch action {
        case .react: sheet = .emoji(itemId: id)
        case .reply: model.startReply(to: id)
        case .copy: UIPasteboard.general.string = model.messageRow(id)?.message?.body
        case .selectText:
            if let body = model.messageRow(id)?.message?.body {
                sheet = .text(body)
            }
        case .edit: model.startEdit(id)
        case .delete: deleting = id
        case .retry: model.retry(id)
        case .discard: model.discard(id)
        }
    }

    private var isDeleting: Binding<Bool> {
        Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })
    }

    private var hasError: Binding<Bool> {
        Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })
    }
}

/// A direct chat's title: the person's picture and name; a tap shows who
/// they are.
private struct PersonHeader: View {
    let room: RoomSummary
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 8) {
                AvatarView(name: room.name, uri: room.avatarUrl, size: 30)
                Text(room.name)
                    .font(.headline)
                    .foregroundColor(.primary)
                    .lineLimit(1)
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(room.name)
        .accessibilityHint("Shows who this is")
    }
}

/// The sheets a room opens over itself.
enum RoomSheet: Identifiable {
    /// Who reacted to a message with a key.
    case reactors(itemId: String, key: String)
    /// Who read up to a message.
    case readers(itemId: String)
    /// All emoji, to react to a message with.
    case emoji(itemId: String)
    /// A message's text, to select from.
    case text(String)

    var id: String {
        switch self {
        case .reactors(let itemId, let key): return "reactors \(itemId) \(key)"
        case .readers(let itemId): return "readers \(itemId)"
        case .emoji(let itemId): return "emoji \(itemId)"
        case .text(let text): return "text \(text)"
        }
    }
}

/// A room's sheet, from the newest rows (names arrive in later updates).
private struct RoomSheetView: View {
    let sheet: RoomSheet
    @ObservedObject var model: TimelineModel

    var body: some View {
        switch sheet {
        case .reactors(let itemId, let key):
            let reaction = model.messageRow(itemId)?.reactions.first { $0.key == key }
            PeopleSheet(title: "Reacted with \(key)", people: reaction?.senders ?? [])
        case .readers(let itemId):
            PeopleSheet(title: "Read by", people: model.messageRow(itemId)?.readers ?? [])
        case .emoji(let itemId):
            let ours = model.messageRow(itemId)?.reactions.filter(\.byUs).map(\.key) ?? []
            EmojiPicker(ours: Set(ours)) { key in
                model.react(itemId, key: key)
            }
        case .text(let text):
            SelectTextSheet(text: text)
        }
    }
}

/// The timeline, newest message at the bottom, older ones loading when its
/// top comes into view. The list and its rows are flipped upside down: the
/// newest row is then the scroll view's first, where it starts and where a
/// lazy stack keeps its place while rows come and change. (Anchored at the
/// bottom instead, the lazy stack's estimates of the rows not drawn yet
/// change with every change of the rows, and a reopened room landed short
/// of its bottom, or past it with nothing drawn.) Flipped, the scroll
/// view's edge effects would be drawn at the wrong edges: they are hidden,
/// and the list is cut at the bars instead. Scrolled up, a button leads
/// back to the bottom.
struct TimelineList: View {
    @ObservedObject var model: TimelineModel
    let actions: RowActions

    /// Below the newest row (flipped: the list's first).
    private static let bottomId = "bottom"

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                rows
            }
            .modifier(TracksBottom(model: model))
            .modifier(HidesScrollEdgeEffects())
            .upsideDown(true)
            .clipped()
            .onChange(of: model.scrollTarget) { target in
                guard let target else { return }
                withAnimation {
                    proxy.scrollTo(target.rowId, anchor: .center)
                }
            }
            .overlay(alignment: .bottomTrailing) {
                if model.showsScrollToBottom {
                    ScrollToBottomButton {
                        withAnimation {
                            // The flipped list's top.
                            proxy.scrollTo(Self.bottomId, anchor: .top)
                        }
                    }
                    .transition(.scale.combined(with: .opacity))
                }
            }
            .animation(.easeOut(duration: 0.2), value: model.showsScrollToBottom)
        }
        .scrollDismissesKeyboard(.interactively)
        .overlay {
            if model.opening {
                ProgressView()
            }
        }
        .overlay(alignment: .top) {
            if model.seeking {
                ProgressView()
                    .padding(10)
                    .background(.regularMaterial, in: Circle())
                    .padding(.top, 8)
                    .accessibilityLabel("Looking for the original message")
            }
        }
    }

    /// Newest first, each the right way up; read oldest first.
    private var rows: some View {
        LazyVStack(spacing: 0) {
            bottom
            ForEach(Array(model.rows.enumerated().reversed()), id: \.element.id) { index, row in
                TimelineRowView(row: row, highlighted: model.highlighted == row.id, actions: actions)
                    .id(row.id)
                    .upsideDown(true)
                    .accessibilitySortPriority(-Double(index))
            }
            top.upsideDown(true)
        }
        .accessibilityElement(children: .contain)
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

    /// Below the newest row, to scroll to; before iOS 18, whether the lazy
    /// stack draws it says whether the timeline is at its bottom.
    private var bottom: some View {
        Color.clear
            .frame(height: 1)
            .id(Self.bottomId)
            .onAppear { model.setBottomDrawn(true) }
            .onDisappear { model.setBottomDrawn(false) }
    }
}

/// From iOS 18, how far the (flipped) timeline is scrolled up from its
/// bottom, as the scroll view says.
private struct TracksBottom: ViewModifier {
    @ObservedObject var model: TimelineModel

    func body(content: Content) -> some View {
        if #available(iOS 18, *) {
            content.onScrollGeometryChange(for: CGFloat.self) { geometry in
                // Flipped: the bottom is the scroll view's top.
                geometry.contentOffset.y + geometry.contentInsets.top
            } action: { _, distance in
                model.scrolled(distanceFromBottom: distance)
            }
        } else {
            content
        }
    }
}

/// The scroll view's soft edges (iOS 26), which a flipped one draws at the
/// wrong edges, over its content.
private struct HidesScrollEdgeEffects: ViewModifier {
    func body(content: Content) -> some View {
        if #available(iOS 26, *) {
            content.scrollEdgeEffectHidden(true, for: .all)
        } else {
            content
        }
    }
}

/// A round button with a down arrow, over the timeline's lower right
/// corner: to the newest message.
struct ScrollToBottomButton: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: "arrow.down")
                .font(.body.weight(.semibold))
                .foregroundColor(.accentColor)
                .frame(width: 40, height: 40)
                .background(.regularMaterial, in: Circle())
                .overlay(Circle().strokeBorder(Color.secondary.opacity(0.25), lineWidth: 0.5))
                .shadow(color: .black.opacity(0.15), radius: 3, y: 1)
        }
        .padding(12)
        .accessibilityLabel("Scroll to the newest message")
    }
}

/// Who types, in one line above the composer; it keeps its height when
/// nobody does, so the timeline does not jump.
struct TypingLine: View {
    let text: String?

    var body: some View {
        Text(text ?? " ")
            .font(.footnote)
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 16)
            .padding(.vertical, 3)
            .accessibilityHidden(text == nil)
    }
}

/// Writing a message (Markdown), or a reply or an edit, with a bar saying
/// which and a button to cancel it.
struct Composer: View {
    @Binding var draft: String
    let compose: TimelineModel.Compose?
    let cancel: () -> Void
    let send: () -> Void
    @FocusState private var focused: Bool

    private var blank: Bool {
        draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        VStack(spacing: 0) {
            if let compose {
                bar(compose)
            }
            HStack(alignment: .bottom, spacing: 8) {
                TextField("Message", text: $draft, axis: .vertical)
                    .lineLimit(1...6)
                    .focused($focused)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 18))
                Button(action: send) {
                    Image(systemName: compose.isEdit ? "checkmark.circle.fill" : "arrow.up.circle.fill")
                        .font(.system(size: 32))
                }
                .disabled(blank)
                .accessibilityLabel(compose.isEdit ? "Save" : "Send")
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
        }
        .background(.bar)
        .onChange(of: compose) { compose in
            if compose != nil {
                focused = true
            }
        }
    }

    private func bar(_ compose: TimelineModel.Compose) -> some View {
        let reply: Bool
        if case .reply = compose { reply = true } else { reply = false }
        return HStack(spacing: 10) {
            Image(systemName: reply ? "arrowshape.turn.up.left" : "pencil")
                .foregroundColor(.accentColor)
            VStack(alignment: .leading, spacing: 1) {
                Text(reply ? "Replying to \(compose.target.sender)" : "Editing")
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
                Text(compose.target.preview)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            Button(action: cancel) {
                Image(systemName: "xmark.circle.fill")
                    .font(.title3)
                    .foregroundStyle(.secondary)
            }
            .accessibilityLabel(reply ? "Cancel the reply" : "Cancel editing")
        }
        .padding(.horizontal, 16)
        .padding(.top, 8)
    }
}

private extension Optional where Wrapped == TimelineModel.Compose {
    var isEdit: Bool {
        if case .edit = self { return true }
        return false
    }
}

extension View {
    /// Flipped vertically if `flipped`: a list and its rows both flipped
    /// show the rows in reverse order, the right way up.
    func upsideDown(_ flipped: Bool) -> some View {
        scaleEffect(x: 1, y: flipped ? -1 : 1, anchor: .center)
    }
}
