import CoreGraphics

/// A message swiped to the left, to reply to it: a drag that starts mostly
/// leftward (not a scroll, nor the back gesture, which goes right) moves
/// the bubble with the finger, up to a limit, and replies if let go past
/// the threshold.
struct SwipeToReply: Equatable {
    /// Let go this far left, the message is replied to.
    static let threshold: CGFloat = 50
    /// The bubble follows the finger at most this far.
    static let limit: CGFloat = 80

    /// The bubble's horizontal offset, zero or negative.
    private(set) var offset: CGFloat = 0

    /// Whether a drag starting at `velocity` is a swipe to reply: leftward,
    /// more than twice as fast as up or down.
    static func begins(velocity: CGPoint) -> Bool {
        -velocity.x > 2 * abs(velocity.y)
    }

    /// The finger moved by `translation` since it touched.
    mutating func update(_ translation: CGPoint) {
        offset = max(-Self.limit, min(0, translation.x))
    }

    /// Letting go now replies.
    var triggers: Bool {
        offset <= -Self.threshold
    }

    /// How much of the reply icon shows, from 0 to 1.
    var progress: CGFloat {
        min(1, -offset / Self.threshold)
    }
}
