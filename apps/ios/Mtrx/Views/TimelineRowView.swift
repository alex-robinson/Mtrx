import MtrxFFI
import SwiftUI

struct TimelineRowView: View {
    let row: TimelineRow

    var body: some View {
        switch row.content {
        case .start:
            Text("This is the start of the room.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 16)
        case .day(let date):
            HStack(spacing: 10) {
                line
                Text(Formatting.day(date))
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .fixedSize()
                line
            }
            .padding(.vertical, 12)
        case .readMarker:
            HStack(spacing: 8) {
                Rectangle().fill(Color.red.opacity(0.6)).frame(height: 1)
                Text("New")
                    .font(.caption2.weight(.semibold))
                    .foregroundColor(.red)
            }
            .padding(.vertical, 6)
            .accessibilityLabel("New messages below")
        case .summary(let text):
            Text(text)
                .font(.footnote)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 6)
        case .message(let message):
            MessageView(row: message)
        }
    }

    private var line: some View {
        Rectangle().fill(Color.secondary.opacity(0.3)).frame(height: 1)
    }
}

/// A message: in a bubble on the right if ours, else on the left, with the
/// sender's name over the first of a group.
struct MessageView: View {
    let row: MessageRow

    private var event: TimelineEvent { row.event }
    private var own: Bool { event.own }

    var body: some View {
        VStack(alignment: own ? .trailing : .leading, spacing: 3) {
            if row.showSender {
                Text(event.displayName)
                    .font(.caption.weight(.semibold))
                    .foregroundColor(AvatarColor.of(event.senderId))
                    .padding(.horizontal, 12)
            }
            HStack(spacing: 0) {
                if own { Spacer(minLength: 48) }
                content
                if !own { Spacer(minLength: 48) }
            }
            if case .failed(let error) = event.sendState {
                Text("Not sent: \(error)")
                    .font(.caption2)
                    .foregroundColor(.red)
                    .padding(.horizontal, 12)
            }
        }
        .padding(.top, row.firstInGroup ? 10 : 2)
    }

    @ViewBuilder
    private var content: some View {
        switch event.content {
        case .message(let message):
            if let media = message.media, media.kind == .image || media.kind == .video {
                VStack(alignment: own ? .trailing : .leading, spacing: 4) {
                    MediaImageView(media: media)
                        .overlay(alignment: .bottomTrailing) {
                            footer.padding(6).background(.ultraThinMaterial, in: Capsule()).padding(6)
                        }
                    if message.body != media.filename && !message.body.isEmpty {
                        bubble(Text(Formatting.linked(message.body)))
                    }
                }
            } else if let media = message.media {
                bubble(FileView(media: media))
            } else {
                bubble(text(message))
            }
        case .redacted:
            bubble(Text("Message deleted").italic().foregroundColor(.secondary))
        case .unableToDecrypt:
            bubble(Text("Unable to decrypt this message (waiting for keys)").italic().foregroundColor(.secondary))
        case .other(let description):
            bubble(Text("sent a \(description)").italic().foregroundColor(.secondary))
        case .membership, .displayNameChange, .otherState:
            // Folded into summaries by `TimelineRows`.
            EmptyView()
        }
    }

    private func text(_ message: Message) -> Text {
        switch message.kind {
        case .emote:
            return Text("\(event.displayName) ").italic() + Text(Formatting.linked(message.body)).italic()
        case .notice:
            return Text(Formatting.linked(message.body)).foregroundColor(own ? .white.opacity(0.85) : .secondary)
        default:
            return Text(Formatting.linked(message.body))
        }
    }

    private func bubble<Content: View>(_ content: Content) -> some View {
        VStack(alignment: .trailing, spacing: 2) {
            content
                .textSelection(.enabled)
                .frame(minWidth: 0, alignment: .leading)
            footer
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .foregroundColor(own ? .white : .primary)
        .tint(own ? .white : .accentColor)
        .background(own ? Color.accentColor : Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 18))
    }

    /// Edited, time and send state.
    private var footer: some View {
        HStack(spacing: 4) {
            if case .message(let message) = event.content, message.edited {
                Text("edited")
            }
            Text(Formatting.time(event.timestamp))
            switch event.sendState {
            case .sent:
                EmptyView()
            case .sending:
                Image(systemName: "clock").accessibilityLabel("Sending")
            case .waiting:
                Image(systemName: "wifi.slash").accessibilityLabel("Waiting for connection")
            case .failed:
                Image(systemName: "exclamationmark.circle").foregroundColor(.red).accessibilityLabel("Not sent")
            }
        }
        .font(.caption2)
        .foregroundStyle(own ? Color.white.opacity(0.75) : Color.secondary)
    }
}
