import Foundation
import MtrxFFI

/// An entry of the Spaces tab's spaces page: a top-level space, or the rooms
/// in no space.
enum SpaceEntry: Hashable {
    case space(String)
    case otherRooms

    /// Room IDs start with "!", so the saved form of Other rooms cannot be
    /// one.
    private static let otherRoomsSaved = "other-rooms"

    /// The form kept in the preferences.
    var saved: String {
        switch self {
        case .space(let id): return id
        case .otherRooms: return Self.otherRoomsSaved
        }
    }

    init(saved: String) {
        self = saved == Self.otherRoomsSaved ? .otherRooms : .space(saved)
    }
}

/// A space (or Other rooms) with its rooms, as listed.
struct SpaceItem: Identifiable, Equatable {
    let entry: SpaceEntry
    /// The space's summary, for its name and picture; nil for Other rooms.
    let space: RoomSummary?
    /// In the order of the room list, most recently active first.
    let rooms: [RoomSummary]

    var id: SpaceEntry { entry }
    var name: String { space?.name ?? "Other rooms" }
    /// Any of its rooms has unread messages.
    var unread: Bool { rooms.contains { $0.unreadMessages > 0 } }
}

/// The lists the app shows, derived from one `RoomListState` as the
/// library computed them (the desktop's rules).
struct RoomLists: Equatable {
    /// The Chats tab: direct chats, invites to them included.
    var chats: [RoomSummary] = []
    /// The Spaces tab's spaces page: the top-level spaces, then Other rooms
    /// while it has any.
    var spaces: [SpaceItem] = []

    init() {}

    init(_ state: RoomListState) {
        let byId = Dictionary(state.rooms.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let summaries = { (ids: [String]) in ids.compactMap { byId[$0] } }
        chats = state.rooms.filter { $0.kind == .direct }
        spaces = state.spaces.compactMap { space in
            guard let summary = byId[space.spaceId] else { return nil }
            return SpaceItem(entry: .space(space.spaceId), space: summary, rooms: summaries(space.roomIds))
        }
        if !state.roomsOutsideSpaces.isEmpty {
            spaces.append(SpaceItem(entry: .otherRooms, space: nil, rooms: summaries(state.roomsOutsideSpaces)))
        }
    }

    /// The top-level spaces (not Other rooms), in order: where someone
    /// can be invited.
    var topLevelSpaces: [RoomSummary] {
        spaces.compactMap(\.space)
    }

    func space(_ entry: SpaceEntry) -> SpaceItem? {
        spaces.first { $0.entry == entry }
    }

    /// The rooms whose name contains `filter`, those starting with it
    /// first, then by name; all of them, in order, without a filter.
    static func search(_ rooms: [RoomSummary], _ filter: String) -> [RoomSummary] {
        search(rooms, filter, name: \.name)
    }

    /// The spaces whose name contains `filter`, as `search(_:_:)`.
    static func search(_ spaces: [SpaceItem], _ filter: String) -> [SpaceItem] {
        search(spaces, filter, name: \.name)
    }

    /// A space's rooms in two sections, Rooms and People (its direct
    /// chats), each in `sort`'s order, or while searching in the search's;
    /// empty sections left out.
    static func sections(_ rooms: [RoomSummary], sort: RoomSort, search filter: String) -> [RoomSection] {
        let shown = filter.trimmingCharacters(in: .whitespaces).isEmpty ? sort.apply(rooms) : search(rooms, filter)
        return [
            RoomSection(title: "Rooms", rooms: shown.filter { $0.kind != .direct }),
            RoomSection(title: "People", rooms: shown.filter { $0.kind == .direct }),
        ].filter { !$0.rooms.isEmpty }
    }

    private static func search<T>(_ items: [T], _ filter: String, name: (T) -> String) -> [T] {
        let filter = filter.trimmingCharacters(in: .whitespaces).lowercased()
        guard !filter.isEmpty else { return items }
        return items
            .compactMap { item -> (later: Bool, name: String, item: T)? in
                let name = name(item).lowercased()
                guard let range = name.range(of: filter) else { return nil }
                return (range.lowerBound != name.startIndex, name, item)
            }
            .sorted { ($0.later ? 1 : 0, $0.name) < ($1.later ? 1 : 0, $1.name) }
            .map(\.item)
    }
}

/// A titled part of a room list.
struct RoomSection: Identifiable, Equatable {
    /// None for a list of one section (the chats).
    let title: String?
    let rooms: [RoomSummary]

    var id: String { title ?? "" }
}

/// The order of a space's rooms, chosen on its page; remembered.
enum RoomSort: String, CaseIterable, Identifiable {
    /// The room list's: most recently active first, as the library
    /// orders it.
    case listOrder = "default"
    case aToZ
    case zToA
    /// By the last activity, newest first; rooms without one last.
    case mostRecent

    var id: Self { self }

    var title: String {
        switch self {
        case .listOrder: return "Default"
        case .aToZ: return "A to Z"
        case .zToA: return "Z to A"
        case .mostRecent: return "Most recent"
        }
    }

    /// `rooms` in this order; ties keep the list's order.
    func apply(_ rooms: [RoomSummary]) -> [RoomSummary] {
        let ordered: (RoomSummary, RoomSummary) -> ComparisonResult
        switch self {
        case .listOrder:
            return rooms
        case .aToZ:
            ordered = { $0.name.localizedStandardCompare($1.name) }
        case .zToA:
            ordered = { $1.name.localizedStandardCompare($0.name) }
        case .mostRecent:
            ordered = { a, b in
                switch (a.lastActivity, b.lastActivity) {
                case let (a?, b?): return a == b ? .orderedSame : (a > b ? .orderedAscending : .orderedDescending)
                case (_?, nil): return .orderedAscending
                case (nil, _?): return .orderedDescending
                case (nil, nil): return .orderedSame
                }
            }
        }
        return rooms.enumerated()
            .sorted { a, b in
                let order = ordered(a.element, b.element)
                return order == .orderedSame ? a.offset < b.offset : order == .orderedAscending
            }
            .map(\.element)
    }
}
