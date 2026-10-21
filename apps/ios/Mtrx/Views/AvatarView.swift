import SwiftUI

/// A room's or person's picture, or their initial on a colour of their
/// name until it loads (or if there is none).
struct AvatarView: View {
    enum Shape {
        case circle
        /// Spaces.
        case square
    }

    let name: String
    /// `mxc://` URI.
    let uri: String?
    let size: CGFloat
    var shape = Shape.circle

    @EnvironmentObject private var images: ImageCache
    @Environment(\.displayScale) private var scale
    @State private var loaded: UIImage?

    private var key: ImageKey? {
        uri.map { .avatar($0, pixels: UInt32((size * scale).rounded())) }
    }

    var body: some View {
        let image = loaded ?? key.flatMap(images.cached)
        ZStack {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
            } else {
                AvatarColor.of(name)
                Text(initial)
                    .font(.system(size: size * 0.45, weight: .semibold))
                    .foregroundColor(.white)
            }
        }
        .frame(width: size, height: size)
        .clipShape(shape == .circle ? AnyShape(Circle()) : AnyShape(RoundedRectangle(cornerRadius: size * 0.25)))
        .accessibilityHidden(true)
        .task(id: key) {
            loaded = nil
            guard let key else { return }
            loaded = await images.load(key)
        }
    }

    private var initial: String {
        let letter = name.first { $0.isLetter || $0.isNumber }
        return letter.map { String($0).uppercased() } ?? "#"
    }
}

enum AvatarColor {
    private static let palette: [Color] = [
        Color(red: 0.36, green: 0.42, blue: 0.94),
        Color(red: 0.17, green: 0.69, blue: 0.79),
        Color(red: 0.85, green: 0.38, blue: 0.47),
        Color(red: 0.93, green: 0.58, blue: 0.22),
        Color(red: 0.31, green: 0.67, blue: 0.43),
        Color(red: 0.62, green: 0.40, blue: 0.85),
        Color(red: 0.80, green: 0.47, blue: 0.75),
        Color(red: 0.45, green: 0.55, blue: 0.62),
    ]

    /// The same colour for a name on every launch (unlike `hashValue`).
    static func of(_ name: String) -> Color {
        let sum = name.unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) & 0xFFFF }
        return palette[sum % palette.count]
    }
}
