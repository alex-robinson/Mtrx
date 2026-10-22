import Foundation

/// Emoji to react with: the common ones offered first, and all the others
/// for the picker, searchable by their Unicode names.
enum Emoji {
    /// Offered when holding a message.
    static let common = ["👍", "❤️", "😂", "😮", "😢", "🎉"]

    struct Entry: Identifiable, Hashable {
        let symbol: String
        /// Lowercased, e.g. "face with tears of joy".
        let name: String
        var id: String { symbol }
    }

    /// The emoji drawn as emoji by default, each one character, faces and
    /// people first; no single regional indicators or skin tones.
    static let all: [Entry] = {
        let blocks: [ClosedRange<UInt32>] = [
            0x1F600...0x1F64F, // faces
            0x1F900...0x1F9FF, // more faces, people, animals, food
            0x1FA70...0x1FAFF, // objects, newer faces
            0x1F300...0x1F5FF, // nature, food, places, objects
            0x1F680...0x1F6FF, // transport
            0x1F780...0x1F7FF, // shapes
            0x2600...0x27BF,   // symbols
            0x2300...0x23FF,   // clocks
            0x2B00...0x2BFF,   // stars, squares
            0x1F000...0x1F2FF, // cards, letters
        ]
        let excluded: [ClosedRange<UInt32>] = [0x1F1E6...0x1F1FF, 0x1F3FB...0x1F3FF]
        return blocks.flatMap { block in
            block.compactMap { value -> Entry? in
                guard !excluded.contains(where: { $0.contains(value) }),
                      let scalar = Unicode.Scalar(value),
                      scalar.properties.isEmojiPresentation
                else { return nil }
                let name = scalar.properties.name?.lowercased() ?? ""
                return Entry(symbol: String(Character(scalar)), name: name)
            }
        }
    }()

    /// The emoji whose name has every word of `query` (all if it is blank).
    static func search(_ query: String, in entries: [Entry] = all) -> [Entry] {
        let words = query.lowercased().split(whereSeparator: \.isWhitespace)
        guard !words.isEmpty else { return entries }
        return entries.filter { entry in
            words.allSatisfy { entry.name.contains($0) || entry.symbol == $0 }
        }
    }
}
