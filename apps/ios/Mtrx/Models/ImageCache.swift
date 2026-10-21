import MtrxFFI
import UIKit

/// A picture to show: an avatar at a size, or a message's image.
enum ImageKey: Hashable {
    /// `mxc://` URI and size in pixels.
    case avatar(String, pixels: UInt32)
    case media(Media)

    fileprivate var name: NSString {
        switch self {
        case .avatar(let uri, let pixels): return "avatar \(pixels) \(uri)" as NSString
        case .media(let media): return "media \(media.source)" as NSString
        }
    }
}

/// Decoded avatars and message images in memory, on top of the library's
/// download cache. One download per picture at a time; failures are not
/// remembered, so a picture is tried again when shown again.
@MainActor
final class ImageCache: ObservableObject {
    private let session: Session
    private let images = NSCache<NSString, UIImage>()
    private var loading: [ImageKey: Task<UIImage?, Never>] = [:]

    init(session: Session) {
        self.session = session
        images.totalCostLimit = 64 << 20
    }

    func cached(_ key: ImageKey) -> UIImage? {
        images.object(forKey: key.name)
    }

    func load(_ key: ImageKey) async -> UIImage? {
        if let image = cached(key) {
            return image
        }
        if let task = loading[key] {
            return await task.value
        }
        let session = self.session
        let task = Task.detached(priority: .userInitiated) { () -> UIImage? in
            let data: Data
            do {
                switch key {
                case .avatar(let uri, let pixels):
                    data = try await session.avatar(mxcUri: uri, size: pixels)
                case .media(let media):
                    data = try await session.mediaPreview(media: media)
                }
            } catch {
                return nil
            }
            return UIImage(data: data)?.preparingForDisplay()
        }
        loading[key] = task
        let image = await task.value
        loading[key] = nil
        if let image {
            let cost = Int(image.size.width * image.size.height * image.scale * image.scale) * 4
            images.setObject(image, forKey: key.name, cost: cost)
        }
        return image
    }
}
