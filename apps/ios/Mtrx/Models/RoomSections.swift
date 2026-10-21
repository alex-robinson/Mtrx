import Foundation
import MtrxFFI

/// The room list's sections, in display order, as on the desktop.
enum RoomSection: Hashable, CaseIterable {
    case invites
    case people
    case rooms

    var title: String {
        switch self {
        case .invites: return "Invites"
        case .people: return "Direct messages"
        case .rooms: return "Rooms"
        }
    }
}

struct RoomGroup: Identifiable, Equatable {
    let section: RoomSection
    let rooms: [RoomSummary]
    var id: RoomSection { section }
}

enum RoomSections {
    /// `rooms` (most recently active first) by section, keeping their
    /// order; empty sections are left out. Joined spaces are not listed
    /// (their rooms are), invites to them are, to be answered.
    static func group(_ rooms: [RoomSummary]) -> [RoomGroup] {
        let listed = rooms.filter { $0.kind != .space || $0.invited }
        return RoomSection.allCases.compactMap { section in
            let members = listed.filter { self.section(of: $0) == section }
            return members.isEmpty ? nil : RoomGroup(section: section, rooms: members)
        }
    }

    static func section(of room: RoomSummary) -> RoomSection {
        if room.invited { return .invites }
        return room.kind == .direct ? .people : .rooms
    }

    /// Rooms and chats whose name contains `filter`, those starting with
    /// it first, then by name.
    static func search(_ rooms: [RoomSummary], _ filter: String) -> [RoomSummary] {
        let filter = filter.trimmingCharacters(in: .whitespaces).lowercased()
        guard !filter.isEmpty else { return [] }
        return rooms
            .filter { $0.kind != .space }
            .compactMap { room -> (later: Bool, name: String, room: RoomSummary)? in
                let name = room.name.lowercased()
                guard let range = name.range(of: filter) else { return nil }
                return (range.lowerBound != name.startIndex, name, room)
            }
            .sorted { ($0.later ? 1 : 0, $0.name) < ($1.later ? 1 : 0, $1.name) }
            .map(\.room)
    }
}
