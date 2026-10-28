import MtrxFFI
import SwiftUI

/// A room: its header (name and topic line), its timeline, newest at the
/// bottom, who types, and the composer. The name opens the room's info,
/// the topic line its topic panel.
struct RoomView: View {
    let room: RoomSummary
    @StateObject private var model: TimelineModel
    @StateObject private var info: RoomInfoModel
    @EnvironmentObject private var images: ImageCache
    @EnvironmentObject private var session: SessionModel
    @EnvironmentObject private var presence: PresenceModel
    @State private var sheet: RoomSheet?
    @State private var showsInfo = false
    /// The message to delete, once confirmed.
    @State private var deleting: String?

    init(
        room: RoomSummary, model: @autoclosure @escaping () -> TimelineModel,
        info: @autoclosure @escaping () -> RoomInfoModel
    ) {
        self.room = room
        _model = StateObject(wrappedValue: model())
        _info = StateObject(wrappedValue: info())
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
            ToolbarItem(placement: .principal) {
                if let person = model.person {
                    PersonHeader(room: room, presence: presence.presence(of: person), open: model.showPersonInfo)
                } else {
                    RoomHeader(
                        room: room, topic: topic, canEdit: info.settings?.canChangeTopic ?? false,
                        openInfo: { showsInfo = true }, openTopic: { sheet = .topic })
                }
            }
        }
        .task { await model.open() }
        .task { await info.loadSettings() }
        .onAppear {
            model.setVisible(true)
            session.opened(room)
        }
        .onDisappear { model.setVisible(false) }
        .onChange(of: room.pinned) { model.setPinned($0) }
        .onChange(of: room.topic) { info.topicListed($0) }
        .sheet(item: $sheet) { sheet in
            RoomSheetView(sheet: sheet, room: room, model: model, info: info, topic: topic)
                .environmentObject(images)
                .environmentObject(session)
        }
        .sheet(item: $model.personInfo) { info in
            PersonInfoView(model: info)
                .environmentObject(images)
                .environmentObject(session)
                .environmentObject(presence)
        }
        .fullScreenCover(isPresented: $showsInfo) {
            RoomInfoView(room: room, model: info)
                .environmentObject(images)
                .environmentObject(session)
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

    /// The topic as shown: one just saved until the room list has it.
    private var topic: Topic {
        parseTopic(topic: info.shownTopic(room.topic))
    }

    private var actions: RowActions {
        RowActions(
            entries: model.actions(for:),
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
        case .pin, .unpin: model.togglePin(id)
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

/// A room's title: its name, which opens its info, over its topic line, a
/// caret and the topic's first line (else its links' names, else "Add a
/// topic" where we may edit it), which opens the topic panel.
struct RoomHeader: View {
    let room: RoomSummary
    let topic: Topic
    let canEdit: Bool
    let openInfo: () -> Void
    let openTopic: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            Button(action: openInfo) {
                Text(room.name)
                    .font(.headline)
                    .foregroundColor(.primary)
                    .lineLimit(1)
            }
            .buttonStyle(.plain)
            .accessibilityHint("Shows the room's info and settings")
            if let line = Topics.preview(topic, canEdit: canEdit) {
                Button(action: openTopic) {
                    HStack(spacing: 3) {
                        Image(systemName: "chevron.down")
                            .font(.system(size: 9, weight: .bold))
                        Text(line)
                            .font(.caption)
                            .lineLimit(1)
                    }
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: 240)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Topic: \(line)")
                .accessibilityHint("Shows the topic, its links and the pinned messages")
            }
        }
    }
}

/// A direct chat's title: the person's picture and name, with their
/// presence; a tap opens their info sheet.
private struct PersonHeader: View {
    let room: RoomSummary
    let presence: Presence?
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 8) {
                AvatarView(name: room.name, uri: room.avatarUrl, size: 28)
                    .presenceDot(presence, size: 9, avatar: 28)
                VStack(alignment: .leading, spacing: 0) {
                    Text(room.name)
                        .font(.headline)
                        .foregroundColor(.primary)
                        .lineLimit(1)
                    PresenceLine(presence: presence, font: .caption)
                }
            }
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
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
    /// The topic in full, its links and the pinned messages.
    case topic

    var id: String {
        switch self {
        case .reactors(let itemId, let key): return "reactors \(itemId) \(key)"
        case .readers(let itemId): return "readers \(itemId)"
        case .emoji(let itemId): return "emoji \(itemId)"
        case .text(let text): return "text \(text)"
        case .topic: return "topic"
        }
    }
}

/// A room's sheet, from the newest rows (names arrive in later updates).
private struct RoomSheetView: View {
    let sheet: RoomSheet
    let room: RoomSummary
    @ObservedObject var model: TimelineModel
    @ObservedObject var info: RoomInfoModel
    let topic: Topic
    @EnvironmentObject private var session: SessionModel

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
        case .topic:
            TopicPanel(
                topic: topic, canEdit: info.settings?.canChangeTopic ?? false, info: info,
                pinned: PinnedModel(session: session.session, roomId: room.id)
            ) { eventId in
                Task { await model.goTo(eventId: eventId) }
            }
        }
    }
}

/// The timeline, newest message at the bottom, older ones loading when its
/// top comes into view. The scroll view is anchored at its bottom: it opens
/// there, and rows added above (older ones) or below (new ones, while at
/// the bottom) keep what is in view in place. Scrolled up, a button leads
/// back to the bottom.
struct TimelineList: View {
    @ObservedObject var model: TimelineModel
    let actions: RowActions

    /// Below the newest row.
    private static let bottomId = "bottom"

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                rows
            }
            .modifier(AnchoredAtBottom(model: model, proxy: proxy, bottomId: Self.bottomId))
            .modifier(TracksBottom(model: model))
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
                            proxy.scrollTo(Self.bottomId, anchor: .bottom)
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

    /// Oldest first.
    private var rows: some View {
        LazyVStack(spacing: 0) {
            top
            ForEach(model.rows) { row in
                TimelineRowView(row: row, highlighted: model.highlighted == row.id, actions: actions)
                    .id(row.id)
            }
            bottom
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

/// Keeps the timeline at its bottom as it opens and while new messages
/// come, and what is in view in place while older ones load above it. From
/// iOS 18 the scroll view's bottom anchor does all of it; before, the rows'
/// changes are followed here too (iOS 17's anchor places it at the start).
private struct AnchoredAtBottom: ViewModifier {
    @ObservedObject var model: TimelineModel
    let proxy: ScrollViewProxy
    let bottomId: String
    /// The oldest row, as last drawn.
    @State private var oldest: String?

    func body(content: Content) -> some View {
        if #available(iOS 18, *) {
            content.defaultScrollAnchor(.bottom)
        } else if #available(iOS 17, *) {
            following(content.defaultScrollAnchor(.bottom))
        } else {
            following(content)
        }
    }

    private func following(_ content: some View) -> some View {
        content
            .onChange(of: model.rows.last?.id) { _ in
                if model.atBottom {
                    proxy.scrollTo(bottomId, anchor: .bottom)
                }
            }
            .onChange(of: model.rows.first?.id) { first in
                // Older rows came above: the row that was the oldest stays
                // at the top.
                if let oldest, oldest != first, !model.atBottom {
                    proxy.scrollTo(oldest, anchor: .top)
                }
                oldest = first
            }
    }
}

/// From iOS 18, how far the timeline is scrolled up from its bottom, as the
/// scroll view says.
private struct TracksBottom: ViewModifier {
    @ObservedObject var model: TimelineModel

    func body(content: Content) -> some View {
        if #available(iOS 18, *) {
            content.onScrollGeometryChange(for: CGFloat.self) { geometry in
                // Short content sits at the bottom: nothing below is hidden.
                geometry.contentSize.height - geometry.visibleRect.maxY
            } action: { _, distance in
                model.scrolled(distanceFromBottom: distance)
            }
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
