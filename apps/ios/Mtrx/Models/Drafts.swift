import Foundation

/// What was typed in each room's composer and not sent yet, kept while the
/// account is signed in: leaving a room for another and coming back finds
/// it as it was. Used from the main actor only.
final class Drafts {
    private var texts: [String: String] = [:]

    subscript(roomId: String) -> String {
        get { texts[roomId] ?? "" }
        set { texts[roomId] = newValue.isEmpty ? nil : newValue }
    }
}
