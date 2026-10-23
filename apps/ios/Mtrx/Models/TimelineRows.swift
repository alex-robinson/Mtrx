import Foundation
import MtrxFFI

/// One row of the timeline as shown: the library's items, with runs of
/// state events (joins, renames, ...) folded into one line and messages
/// grouped by sender.
struct TimelineRow: Identifiable, Equatable {
    enum Content: Equatable {
        /// The room's beginning.
        case start
        case day(Date)
        /// Everything below was not read yet.
        case readMarker
        case message(MessageRow)
        /// A run of state events in one line, e.g. "alice created and
        /// configured the room · bob joined".
        case summary(String)
    }

    /// The library's item ID (for a summary, its first event's): stable
    /// while the item stays, so the list's identity.
    let id: String
    let content: Content
}

struct MessageRow: Equatable {
    let event: TimelineEvent
    /// Starts a group of messages by one sender.
    let firstInGroup: Bool
    /// Show the sender's name above it and their avatar beside it (the
    /// first of a group by someone else, outside direct chats).
    let showSender: Bool
    /// Someone else's message outside a direct chat: the avatars' column
    /// is on its left (empty but for the first of a group).
    let avatarColumn: Bool
    /// An undecryptable message while message keys arrive from the backup:
    /// it may decrypt meanwhile.
    let decrypting: Bool

    var message: Message? {
        if case .message(let message) = event.content { return message }
        return nil
    }

    /// In the order first given.
    var reactions: [Reaction] { message?.reactions ?? [] }
    /// The message it answers.
    var quote: ReplyQuote? { message?.replyTo }
    var edited: Bool { message?.edited ?? false }
    /// The others who read up to here.
    var readers: [Reader] { event.readBy }

    /// What an undecryptable message says instead of its text; nil for
    /// others.
    var undecryptableText: String? {
        guard event.content == .unableToDecrypt else { return nil }
        return decrypting ? "Decrypting…" : "Could not decrypt this message."
    }

    /// Swiping it to the left answers it, as Reply in its menu.
    var canSwipeToReply: Bool {
        actions.contains(.reply)
    }

    /// What holding the message offers, in the menu's order.
    var actions: [MessageAction] {
        var actions: [MessageAction] = []
        let sent = event.eventId != nil
        if let message {
            if sent {
                actions += [.react, .reply]
            }
            if !message.body.isEmpty {
                actions += [.copy, .selectText]
            }
        }
        if event.editable {
            actions.append(.edit)
        }
        if event.own && sent && event.content != .redacted {
            actions.append(.delete)
        }
        if case .failed = event.sendState {
            actions += [.retry, .discard]
        }
        return actions
    }
}

/// What can be done with a message.
enum MessageAction: Equatable {
    /// Add (or remove) a reaction.
    case react
    /// Answer it, quoting it.
    case reply
    /// Copy its text.
    case copy
    /// Show its text to select from.
    case selectText
    /// Change its text (ours).
    case edit
    /// Delete it for everyone (ours).
    case delete
    /// Send it again after the server refused it.
    case retry
    /// Drop it after the server refused it.
    case discard
}

enum TimelineRows {
    /// Messages further apart start a new group.
    static let groupGap: TimeInterval = 5 * 60

    /// The rows for `items` (oldest first); `showSenders` is false in
    /// direct chats, where who wrote what is clear. While `keysArriving`,
    /// undecryptable messages say they are being decrypted.
    static func build(_ items: [TimelineItem], showSenders: Bool, keysArriving: Bool = false) -> [TimelineRow] {
        var rows: [TimelineRow] = []
        var stateRun: [(id: String, event: TimelineEvent)] = []
        var previous: TimelineEvent?

        func endStateRun() {
            guard let first = stateRun.first else { return }
            let summary = stateSummary(stateRun.map(\.event))
            if !summary.isEmpty {
                rows.append(TimelineRow(id: first.id, content: .summary(summary)))
                previous = nil
            }
            stateRun = []
        }

        for item in items {
            switch item.content {
            case .start:
                endStateRun()
                rows.append(TimelineRow(id: item.id, content: .start))
                previous = nil
            case .dayDivider(let date):
                endStateRun()
                rows.append(TimelineRow(id: item.id, content: .day(date)))
                previous = nil
            case .readMarker:
                endStateRun()
                rows.append(TimelineRow(id: item.id, content: .readMarker))
                previous = nil
            case .event(let event):
                if event.isState {
                    stateRun.append((item.id, event))
                    continue
                }
                endStateRun()
                let first = previous.map { !continuesGroup($0, with: event) } ?? true
                let others = showSenders && !event.own
                let row = MessageRow(
                    event: event, firstInGroup: first, showSender: first && others, avatarColumn: others,
                    decrypting: keysArriving && event.content == .unableToDecrypt)
                rows.append(TimelineRow(id: item.id, content: .message(row)))
                previous = event
            }
        }
        endStateRun()
        return rows
    }

    static func continuesGroup(_ previous: TimelineEvent, with event: TimelineEvent) -> Bool {
        previous.senderId == event.senderId
            && event.timestamp.timeIntervalSince(previous.timestamp) < groupGap
    }

    /// One line for a run of state events, as the desktop app writes it.
    static func stateSummary(_ events: [TimelineEvent]) -> String {
        let creators = events.filter { $0.isRoomCreation }.map(\.senderId)
        var parts: [String] = []
        var joined: [String] = []
        var left: [String] = []
        for event in events {
            let name = event.displayName
            let part: String
            switch event.content {
            case .otherState(let type) where type == "m.room.create":
                part = "\(name) created and configured the room"
            case .otherState where creators.contains(event.senderId):
                // Initial settings belong to "created and configured".
                continue
            case .otherState(let type):
                switch type {
                case "m.room.name": part = "\(name) renamed the room"
                case "m.room.topic": part = "\(name) changed the topic"
                case "m.room.avatar": part = "\(name) changed the room picture"
                case "m.room.pinned_events": part = "\(name) changed the pinned messages"
                default: part = "\(name) changed the room settings"
                }
            case .membership(let userId, let targetName, let change):
                let target = targetName ?? localpart(userId)
                switch change {
                case .joined, .invitationAccepted:
                    if !creators.contains(userId) && !joined.contains(target) {
                        joined.append(target)
                    }
                    continue
                case .left:
                    left.append(target)
                    continue
                case .invited: part = "\(name) invited \(target)"
                case .invitationRejected: part = "\(target) declined the invite"
                case .invitationRevoked: part = "\(name) withdrew the invite for \(target)"
                case .kicked: part = "\(name) removed \(target)"
                case .banned: part = "\(name) banned \(target)"
                case .unbanned: part = "\(name) unbanned \(target)"
                case .knocked: part = "\(target) asked to join"
                case .other: continue
                }
            case .displayNameChange(let userId, let newName):
                if let newName {
                    part = "\(localpart(userId)) is now \(newName)"
                } else {
                    part = "\(localpart(userId)) removed their display name"
                }
            default:
                continue
            }
            if parts.last != part {
                parts.append(part)
            }
        }
        if !joined.isEmpty {
            parts.append("\(joined.joined(separator: ", ")) joined")
        }
        if !left.isEmpty {
            parts.append("\(left.joined(separator: ", ")) left")
        }
        return parts.joined(separator: " · ")
    }
}

/// `alice` of `@alice:example.org`.
func localpart(_ userId: String) -> String {
    let name = userId.hasPrefix("@") ? userId.dropFirst() : Substring(userId)
    return String(name.prefix { $0 != ":" })
}

/// Who read up to a message, as its row shows them: at most `shown`
/// avatars, then "+N".
enum ReaderAvatars {
    static let shown = 3

    static func split(_ readers: [Reader]) -> (shown: [Reader], more: Int) {
        (Array(readers.prefix(shown)), max(0, readers.count - shown))
    }
}

extension Reader {
    /// The display name, else the user ID's local part (until the name is
    /// known).
    var displayName: String {
        name ?? localpart(userId)
    }
}

extension ReplyQuote {
    /// The original's sender, once loaded.
    var senderLabel: String? {
        senderName ?? senderId.map(localpart)
    }

    /// The original in a few words: its text, or for a file the kind and
    /// its name; "Loading…" while it is fetched.
    var preview: String {
        switch state {
        case .loading:
            return "Loading…"
        case .failed:
            return "Could not load the message"
        case .loaded:
            let body = body ?? ""
            guard let label = kind?.fileLabel else { return body }
            return body.isEmpty ? label : "\(label): \(body)"
        }
    }

    /// Tapping it goes to the original.
    var canGoTo: Bool {
        state == .loaded
    }
}

extension MessageKind {
    /// "Image", "Video", "Audio" or "File" for a message with a file.
    var fileLabel: String? {
        switch self {
        case .image: return "Image"
        case .video: return "Video"
        case .audio: return "Audio"
        case .file: return "File"
        default: return nil
        }
    }
}

extension TimelineEvent {
    /// The sender's display name, else their user ID's local part.
    var displayName: String {
        senderName ?? localpart(senderId)
    }

    var isState: Bool {
        switch content {
        case .membership, .displayNameChange, .otherState: return true
        default: return false
        }
    }

    var isRoomCreation: Bool {
        if case .otherState("m.room.create") = content { return true }
        return false
    }
}
