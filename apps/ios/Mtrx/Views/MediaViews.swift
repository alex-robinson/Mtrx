import MtrxFFI
import SwiftUI

/// A message's image, or a video's preview, laid out at its size before it
/// loads.
struct MediaImageView: View {
    let media: Media

    @EnvironmentObject private var images: ImageCache
    @State private var loaded: UIImage?
    @State private var failed = false

    static let maxSize = CGSize(width: 260, height: 320)

    var body: some View {
        let image = loaded ?? images.cached(.media(media))
        let size = Self.fitted(media)
        ZStack {
            Color(.secondarySystemBackground)
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
            } else if failed {
                Image(systemName: "photo")
                    .font(.title)
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("Could not load the image")
            } else {
                ProgressView()
            }
            if media.kind == .video {
                Image(systemName: "play.circle.fill")
                    .font(.system(size: 44))
                    .foregroundStyle(.white, .black.opacity(0.4))
                    .accessibilityLabel("Video")
            }
        }
        .frame(width: size.width, height: size.height)
        .clipShape(RoundedRectangle(cornerRadius: 16))
        .accessibilityLabel(media.filename)
        .task(id: media.source) {
            guard images.cached(.media(media)) == nil else { return }
            loaded = await images.load(.media(media))
            failed = loaded == nil
        }
    }

    /// The image's size scaled down to fit `maxSize`; a square if unknown.
    static func fitted(_ media: Media) -> CGSize {
        guard let width = media.width, let height = media.height, width > 0, height > 0 else {
            return CGSize(width: 200, height: 200)
        }
        let size = CGSize(width: CGFloat(width), height: CGFloat(height))
        let scale = min(1, maxSize.width / size.width, maxSize.height / size.height)
        return CGSize(width: max(size.width * scale, 60), height: max(size.height * scale, 60))
    }
}

/// A file, audio or other attachment: its name and size.
struct FileView: View {
    let media: Media

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: symbol)
                .font(.title2)
            VStack(alignment: .leading, spacing: 2) {
                Text(media.filename)
                    .lineLimit(2)
                if let size = media.size {
                    Text(Formatting.bytes(size))
                        .font(.caption)
                        .opacity(0.7)
                }
            }
        }
    }

    private var symbol: String {
        switch media.kind {
        case .audio: return "waveform"
        case .video: return "film"
        case .image: return "photo"
        case .file: return "doc"
        }
    }
}
