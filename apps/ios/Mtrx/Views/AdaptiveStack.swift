import SwiftUI

/// Its views side by side, or one under the other (leading) at the
/// accessibility text sizes, where side by side would leave each too
/// little width.
struct AdaptiveStack<Content: View>: View {
    var spacing: CGFloat?
    @ViewBuilder let content: () -> Content
    @Environment(\.dynamicTypeSize) private var size

    var body: some View {
        let layout = size.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: spacing))
            : AnyLayout(HStackLayout(spacing: spacing))
        layout(content)
    }
}

extension View {
    /// At most `lines` lines, or `accessibility` at the accessibility text
    /// sizes, where a name cut to one line shows only a few letters.
    func lineLimit(_ lines: Int, accessibility: Int) -> some View {
        modifier(AdaptiveLineLimit(lines: lines, accessibility: accessibility))
    }
}

private struct AdaptiveLineLimit: ViewModifier {
    let lines: Int
    let accessibility: Int
    @Environment(\.dynamicTypeSize) private var size

    func body(content: Content) -> some View {
        content.lineLimit(size.isAccessibilitySize ? accessibility : lines)
    }
}

/// The size of a list row's picture, growing with the text up to 1.75
/// times `base`.
@propertyWrapper
struct RowAvatarSize: DynamicProperty {
    private let scaled: ScaledMetric<CGFloat>
    private let base: CGFloat

    init(_ base: CGFloat = RoomRow.avatarSize) {
        self.base = base
        scaled = ScaledMetric(wrappedValue: base, relativeTo: .body)
    }

    var wrappedValue: CGFloat { min(scaled.wrappedValue, base * 1.75) }
}
