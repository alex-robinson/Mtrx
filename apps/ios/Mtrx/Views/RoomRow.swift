import MtrxFFI
import SwiftUI

/// A room in the list: picture (with a direct chat's person's presence),
/// name, last activity and unread count.
struct RoomRow: View {
    let room: RoomSummary
    var presence: Presence?

    private var unread: Bool { room.unreadMessages > 0 || room.unreadMentions > 0 }

    var body: some View {
        HStack(spacing: 12) {
            AvatarView(name: room.name, uri: room.avatarUrl, size: 44, shape: room.kind == .space ? .square : .circle)
                .presenceDot(presence, size: 12, avatar: 44)
            VStack(alignment: .leading, spacing: 2) {
                Text(room.name)
                    .fontWeight(unread ? .semibold : .regular)
                    .lineLimit(1)
                if room.invited {
                    Text(room.kind == .space ? "Invited to the space" : "Invited – answer on your computer")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 4) {
                if let last = room.lastActivity {
                    Text(Formatting.lastActivity(last))
                        .font(.caption)
                        .foregroundStyle(unread ? Color.accentColor : .secondary)
                }
                UnreadBadge(room: room)
            }
        }
        .padding(.vertical, 4)
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
                .padding(.vertical, 2)
                .frame(minWidth: 20)
                .background(mentions ? Color.red : Color.accentColor, in: Capsule())
                .accessibilityLabel(mentions ? "\(count) mentions" : "\(count) unread")
        }
    }
}
