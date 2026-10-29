import Foundation

/// How this phone shows a space's sections, by `RoomSection.key`: which are
/// hidden and folded, and their order once reordered here. Kept on the
/// phone only: the desktop's arrangement is not changed. A section new to
/// the phone takes its place in the account's order, shown and unfolded.
struct SectionLayout: Codable, Equatable {
    /// The keys in the order chosen here; nil: the account's order.
    var order: [String]?
    var hidden: Set<String> = []
    var folded: Set<String> = []

    func shows(_ key: String) -> Bool { !hidden.contains(key) }

    func folds(_ key: String) -> Bool { folded.contains(key) }

    /// The account's sections (`account`, in its order) in this phone's
    /// order: those reordered here as they were put, each new one after the
    /// section it follows in the account's order (first if none does).
    func arranged(_ account: [String]) -> [String] {
        guard let order else { return account }
        let present = Set(account)
        var arranged = order.filter(present.contains)
        for (at, key) in account.enumerated() where !arranged.contains(key) {
            if let before = account[..<at].last(where: arranged.contains),
               let index = arranged.firstIndex(of: before)
            {
                arranged.insert(key, at: index + 1)
            } else {
                arranged.insert(key, at: 0)
            }
        }
        return arranged
    }

    mutating func toggleFold(_ key: String) {
        if folded.contains(key) {
            folded.remove(key)
        } else {
            folded.insert(key)
        }
    }

    mutating func setShown(_ key: String, _ shown: Bool) {
        if shown {
            hidden.remove(key)
        } else {
            hidden.insert(key)
        }
    }

    /// Show (or hide) every one of `keys`.
    mutating func setAllShown(_ keys: [String], _ shown: Bool) {
        hidden = shown ? hidden.subtracting(keys) : hidden.union(keys)
    }

    /// Move sections of the account's (`account`) as arranged here, as a
    /// list's `onMove` says: from `offsets` to before `destination`.
    mutating func move(_ account: [String], from offsets: IndexSet, to destination: Int) {
        var arranged = arranged(account)
        let moving = offsets.map { arranged[$0] }
        let before = offsets.filter { $0 < destination }.count
        for offset in offsets.sorted(by: >) {
            arranged.remove(at: offset)
        }
        arranged.insert(contentsOf: moving, at: destination - before)
        order = arranged
    }
}

/// Each space's `SectionLayout` on this phone, in the preferences.
@MainActor
final class SectionLayouts: ObservableObject {
    @Published private var layouts: [String: SectionLayout] = [:]
    private let defaults: UserDefaults
    private static let prefix = "sectionLayout."

    init(defaults: UserDefaults) {
        self.defaults = defaults
    }

    subscript(entry: SpaceEntry) -> SectionLayout {
        if let layout = layouts[entry.saved] {
            return layout
        }
        guard let data = defaults.data(forKey: Self.prefix + entry.saved),
              let saved = try? JSONDecoder().decode(SectionLayout.self, from: data)
        else { return SectionLayout() }
        return saved
    }

    /// Change `entry`'s layout, and keep it.
    func change(_ entry: SpaceEntry, _ change: (inout SectionLayout) -> Void) {
        var layout = self[entry]
        change(&layout)
        layouts[entry.saved] = layout
        if let data = try? JSONEncoder().encode(layout) {
            defaults.set(data, forKey: Self.prefix + entry.saved)
        }
    }

    /// Forget every space's (at sign-out).
    func forget() {
        layouts = [:]
        for key in defaults.dictionaryRepresentation().keys where key.hasPrefix(Self.prefix) {
            defaults.removeObject(forKey: key)
        }
    }
}
