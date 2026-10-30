import MtrxFFI
import SwiftUI

/// A room in a list, compact: picture (with a direct chat's person's
/// presence), name, a detail line (its space, or an invite), last activity,
/// a pencil if it has a draft, and unread count.
struct RoomRow: View {
    static let avatarSize: CGFloat = 34

    let room: RoomSummary
    var presence: Presence?
    /// Small and grey under the name, e.g. the room's space.
    var detail: String?
    /// Something was written in it and not sent.
    var draft = false

    private var unread: Bool { room.unreadMessages > 0 || room.unreadMentions > 0 }

    private var subtitle: String? {
        if room.invited {
            switch room.kind {
            case .space: return "Invited to the space"
            case .direct: return "Wants to chat"
            case .room: return "Invited"
            }
        }
        return detail
    }

    var body: some View {
        HStack(spacing: 10) {
            AvatarView(
                name: room.name, uri: room.avatarUrl, size: Self.avatarSize,
                shape: room.kind == .space ? .square : .circle
            )
            .presenceDot(presence, size: 10, avatar: Self.avatarSize)
            VStack(alignment: .leading, spacing: 0) {
                Text(room.name)
                    .fontWeight(unread ? .semibold : .regular)
                    .lineLimit(1)
                if let subtitle {
                    Text(subtitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                if let last = room.lastActivity {
                    Text(Formatting.lastActivity(last))
                        .font(.caption2)
                        .foregroundStyle(unread ? Color.accentColor : .secondary)
                }
                HStack(spacing: 4) {
                    if draft {
                        Image(systemName: "pencil")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .accessibilityLabel("Draft")
                    }
                    UnreadBadge(room: room)
                }
            }
        }
    }
}

/// Unread messages, in red when they mention the user.
struct UnreadBadge: View {
    let room: RoomSummary

    var body: some View {
        let mentions = room.unreadMentions > 0
        let count = mentions ? room.unreadMentions : room.unreadMessages
        if count > 0 {
            Text(count > 999 ? "999+" : String(count))
                .font(.caption2.weight(.semibold))
                .monospacedDigit()
                .foregroundColor(.white)
                .padding(.horizontal, 6)
                .padding(.vertical, 1)
                .frame(minWidth: 18)
                .background(mentions ? Color.red : Color.accentColor, in: Capsule())
                .accessibilityLabel(mentions ? "\(count) mentions" : "\(count) unread")
        }
    }
}
