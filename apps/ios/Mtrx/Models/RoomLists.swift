import Foundation
import MtrxFFI

/// An entry of the Spaces tab's spaces page: a top-level space, or the rooms
/// in no space.
enum SpaceEntry: Hashable {
    case space(String)
    case home

    /// Room IDs start with "!", so the saved form of Home cannot be one
    /// (from when it was called Other rooms).
    private static let homeSaved = "other-rooms"

    /// The form kept in the preferences.
    var saved: String {
        switch self {
        case .space(let id): return id
        case .home: return Self.homeSaved
        }
    }

    init(saved: String) {
        self = saved == Self.homeSaved ? .home : .space(saved)
    }
}

/// A space (or Home) with its rooms, as listed.
struct SpaceItem: Identifiable, Equatable {
    let entry: SpaceEntry
    /// The space's summary, for its name and picture; nil for Home.
    let space: RoomSummary?
    /// In the order of the room list, most recently active first.
    let rooms: [RoomSummary]
    /// Its rooms in the desktop's sections, in the account's order: every
    /// built-in section (even empty) and the space's categories.
    let sections: [RoomSection]

    init(entry: SpaceEntry, space: RoomSummary?, rooms: [RoomSummary], sections: [RoomSection] = []) {
        self.entry = entry
        self.space = space
        self.rooms = rooms
        self.sections = sections.isEmpty ? Self.sections(of: rooms) : sections
    }

    var id: SpaceEntry { entry }
    var name: String { space?.name ?? "Home" }
    /// Any of its rooms has unread messages.
    var unread: Bool { rooms.contains { $0.unreadMessages > 0 } }

    /// Sections for rooms the library did not arrange (Home): its
    /// invites, direct messages and rooms.
    private static func sections(of rooms: [RoomSummary]) -> [RoomSection] {
        let ids = { (rooms: [RoomSummary]) in rooms.map(\.id) }
        let joined = rooms.filter { !$0.invited }
        return [
            RoomSection(key: "invites", kind: .invites, name: "", roomIds: ids(rooms.filter(\.invited))),
            RoomSection(
                key: "direct_messages", kind: .directMessages, name: "",
                roomIds: ids(joined.filter { $0.kind == .direct })),
            RoomSection(key: "rooms", kind: .rooms, name: "", roomIds: ids(joined.filter { $0.kind != .direct })),
        ]
    }
}

/// The lists the app shows, derived from one `RoomListState` as the
/// library computed them (the desktop's rules).
struct RoomLists: Equatable {
    /// The Chats tab: direct chats, invites to them included; not the
    /// Notes to self while it is at the top.
    var chats: [RoomSummary] = []
    /// The Notes to self, while it stays at the top of Chats and of every
    /// space.
    var notes: RoomSummary?
    /// The Spaces tab's spaces page: the top-level spaces, then Home while
    /// it has any.
    var spaces: [SpaceItem] = []
    /// The joined rooms (not chats or spaces), in the list's order.
    var rooms: [RoomSummary] = []
    /// Every room, chat and space by ID.
    var byId: [String: RoomSummary] = [:]
    /// The rooms with a draft on this device.
    var drafts: Set<String> = []
    /// The names of the spaces each direct chat's person is in with us, by
    /// the chat's ID, in the Spaces tab's order: the spaces that list the
    /// chat. None for a chat with no space.
    var chatSpaces: [String: [String]] = [:]

    init() {}

    init(_ state: RoomListState) {
        let byId = Dictionary(state.rooms.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        self.byId = byId
        drafts = Set(state.drafts)
        let summaries = { (ids: [String]) in ids.compactMap { byId[$0] } }
        notes = state.notesAtTop ? state.notesRoomId.flatMap { byId[$0] } : nil
        chats = state.rooms.filter { $0.kind == .direct && $0.id != notes?.id }
        rooms = state.rooms.filter { $0.kind == .room && !$0.invited }
        spaces = state.spaces.compactMap { space in
            guard let summary = byId[space.spaceId] else { return nil }
            return SpaceItem(
                entry: .space(space.spaceId), space: summary, rooms: summaries(space.roomIds),
                sections: space.sections)
        }
        if !state.roomsOutsideSpaces.isEmpty {
            spaces.append(SpaceItem(entry: .home, space: nil, rooms: summaries(state.roomsOutsideSpaces)))
        }
        for item in spaces {
            guard let space = item.space else { continue }
            for chat in item.rooms where chat.kind == .direct {
                chatSpaces[chat.id, default: []].append(space.name)
            }
        }
    }

    /// The top-level spaces (not Home), in order: where someone can be
    /// invited.
    var topLevelSpaces: [RoomSummary] {
        spaces.compactMap(\.space)
    }

    func space(_ entry: SpaceEntry) -> SpaceItem? {
        spaces.first { $0.entry == entry }
    }

    /// The Rooms tab: Home (the rooms in no space), then each space's
    /// rooms, in the Spaces tab's order, a room in several spaces in each;
    /// in each, the rooms by their latest message, newest first. Not direct
    /// chats or spaces; groups without rooms left out. Only the rooms in
    /// `among`, if given (those shared with someone).
    func roomGroups(among ids: Set<String>? = nil) -> [ListSection] {
        let shown = { (rooms: [RoomSummary]) in
            RoomSort.mostRecent.apply(rooms.filter { $0.kind == .room && ids?.contains($0.id) ?? true })
        }
        let home = space(.home).map { ListSection(key: Self.homeKey, title: "Home", rooms: shown($0.rooms)) }
        let bySpace = spaces.compactMap { item in
            item.space.map { ListSection(key: $0.id, title: $0.name, rooms: shown(item.rooms), space: $0.id) }
        }
        return ([home].compactMap { $0 } + bySpace).filter { !$0.rooms.isEmpty }
    }

    /// The key of the Rooms tab's group of rooms in no space.
    static let homeKey = "home"

    /// The first space, in the Spaces tab's order, that lists room `id`
    /// (through nested spaces too); nil for a room in no space.
    func firstSpace(of id: String) -> RoomSummary? {
        spaces.first { $0.space != nil && $0.rooms.contains { $0.id == id } }?.space
    }

    // MARK: Unread messages

    /// The Chats tab's badge: unread messages in the direct chats (the
    /// Notes to self too).
    var chatsUnread: Int { Self.unread(directChats) }

    /// The Rooms tab's badge: unread messages in the rooms.
    var roomsUnread: Int { Self.unread(rooms) }

    /// The Spaces tab's badge: unread messages in the rooms in spaces.
    var spacesUnread: Int {
        let inSpaces = Set(spaces.filter { $0.space != nil }.flatMap { $0.rooms.map(\.id) })
        return Self.unread(rooms.filter { inSpaces.contains($0.id) })
    }

    /// The number on a room's or chat's back button: unread messages in
    /// the other rooms and direct chats.
    func unread(besides id: String) -> Int {
        Self.unread((rooms + directChats).filter { $0.id != id })
    }

    private var directChats: [RoomSummary] {
        chats + [notes].compactMap { $0 }
    }

    static func unread(_ rooms: [RoomSummary]) -> Int {
        rooms.reduce(0) { $0 + Int(clamping: $1.unreadMessages) }
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

    /// A space's list: its sections in the order `layout` puts the
    /// account's in, those shown on this phone and with rooms, folded as
    /// `layout` says; each section's rooms in `sort`'s order. While
    /// searching, the rooms found, in the search's order, none folded. The
    /// Notes to self at the top (`notes`) is left out of them.
    func sections(of item: SpaceItem, layout: SectionLayout, sort: RoomSort, search filter: String) -> [ListSection] {
        let searching = !filter.trimmingCharacters(in: .whitespaces).isEmpty
        let byKey = Dictionary(item.sections.map { ($0.key, $0) }, uniquingKeysWith: { first, _ in first })
        return layout.arranged(item.sections.map(\.key)).compactMap { key -> ListSection? in
            guard let section = byKey[key], layout.shows(key) else { return nil }
            let rooms = section.roomIds.compactMap { byId[$0] }.filter { $0.id != notes?.id }
            let shown = searching ? Self.search(rooms, filter) : sort.apply(rooms)
            guard !shown.isEmpty else { return nil }
            return ListSection(
                key: key, title: Self.title(section), rooms: shown, folded: !searching && layout.folds(key))
        }
    }

    /// What a section is called: a category its name, the others theirs.
    static func title(_ section: RoomSection) -> String {
        switch section.kind {
        case .invites: return "Invites"
        case .favourites: return "Favourites"
        case .category: return section.name
        case .directMessages: return "Direct messages"
        case .rooms: return "Rooms"
        case .lowPriority: return "Low priority"
        }
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

/// A titled part of a room list, folded or not.
struct ListSection: Identifiable, Equatable {
    /// The section's `RoomSection.key`.
    let key: String
    /// None for a list of one section (the chats).
    let title: String?
    let rooms: [RoomSummary]
    /// Only its heading shows, with its unread count.
    var folded = false
    /// The space whose rooms it holds, which its heading opens (the Rooms
    /// tab's groups).
    var space: String?

    var id: String { key }

    /// Unread messages in its rooms, and whether any mention us.
    var unread: (count: UInt64, mentions: Bool) {
        (rooms.reduce(0) { $0 + $1.unreadMessages }, rooms.contains { $0.unreadMentions > 0 })
    }
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
