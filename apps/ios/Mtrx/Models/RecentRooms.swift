import Foundation
import MtrxFFI

/// The rooms opened lately, across all spaces, newest first: the Rooms
/// tab's Recent list, whose first is the last room, which the tab reopens.
/// Direct chats (the Chats tab's) and spaces are not among them.
struct RecentRooms: Equatable {
    /// At most this many are kept.
    static let limit = 20

    private(set) var ids: [String]

    init(_ ids: [String] = []) {
        self.ids = Array(ids.prefix(Self.limit))
    }

    /// The room the Rooms tab reopens.
    var last: String? { ids.first }

    /// `id` was opened: first now.
    mutating func opened(_ id: String) {
        ids.removeAll { $0 == id }
        ids.insert(id, at: 0)
        if ids.count > Self.limit {
            ids.removeLast(ids.count - Self.limit)
        }
    }

    /// Only the rooms still in the list (`rooms`): left ones go.
    mutating func keep(_ rooms: Set<String>) {
        ids.removeAll { !rooms.contains($0) }
    }

    mutating func remove(_ id: String) {
        ids.removeAll { $0 == id }
    }

    /// The recent rooms' summaries, in order.
    func rooms(in lists: RoomLists) -> [RoomSummary] {
        ids.compactMap { lists.byId[$0] }
    }

    /// The Rooms tab's badge: unread messages in the recent rooms.
    func unread(in lists: RoomLists) -> Int {
        Self.unread(rooms(in: lists))
    }

    /// The back button's number in a room: unread messages in the other
    /// recent rooms.
    func unread(in lists: RoomLists, besides id: String) -> Int {
        Self.unread(rooms(in: lists).filter { $0.id != id })
    }

    /// The Spaces tab's badge: unread messages in rooms not among the
    /// recent ones, so activity elsewhere shows there.
    func unreadElsewhere(in lists: RoomLists) -> Int {
        let recent = Set(ids)
        return Self.unread(lists.rooms.filter { !recent.contains($0.id) })
    }

    private static func unread(_ rooms: [RoomSummary]) -> Int {
        rooms.reduce(0) { $0 + Int(clamping: $1.unreadMessages) }
    }
}
