import Foundation
import MtrxFFI

/// Members' roles as the desktop shows and changes them: a label by each,
/// the levels we may give in a member's menu, and what to ask first where a
/// change can't be undone.
enum MemberRoles {
    /// "Owner" (a creator, above every level), "Admin", "Mod", "Level 20",
    /// or nil for a plain member.
    static func label(_ member: Member) -> String? {
        switch member.level {
        case nil: return "Owner"
        case 100: return "Admin"
        case 50: return "Mod"
        case let level? where level == member.manage.defaultLevel: return nil
        case let level?: return "Level \(level)"
        }
    }

    /// A level in a member's menu.
    struct Choice: Identifiable, Equatable {
        let title: String
        let level: Int64
        /// Theirs: ticked.
        let current: Bool

        var id: Int64 { level }
    }

    /// The levels we may give `member`, theirs ticked: Admin 100, Moderator
    /// 50, Member (the room's default), each if theirs or at most `upTo`;
    /// theirs first if none of those. Empty if we may not change theirs.
    static func choices(for member: Member) -> [Choice] {
        guard let upTo = member.manage.upTo else { return [] }
        var levels: [(String, Int64)] = [("Admin", 100), ("Moderator", 50)]
        if !levels.contains(where: { $0.1 == member.manage.defaultLevel }) {
            levels.append(("Member", member.manage.defaultLevel))
        }
        var choices: [Choice] = []
        if let level = member.level, !levels.contains(where: { $0.1 == level }) {
            choices.append(Choice(title: "Level \(level)", level: level, current: true))
        }
        for (title, level) in levels where member.level == level || level <= upTo {
            choices.append(Choice(title: title, level: level, current: member.level == level))
        }
        return choices
    }

    /// What `level` makes someone: "admin", "moderator", "member" or
    /// "level 20".
    static func role(_ level: Int64, defaultLevel: Int64) -> String {
        switch level {
        case 100: return "admin"
        case 50: return "moderator"
        case defaultLevel: return "member"
        default: return "level \(level)"
        }
    }

    /// The role with its article: "an admin", "a member", "level 20".
    static func aRole(_ level: Int64, defaultLevel: Int64) -> String {
        let role = role(level, defaultLevel: defaultLevel)
        if role.hasPrefix("level") { return role }
        return role.hasPrefix("a") ? "an \(role)" : "a \(role)"
    }

    /// A question before a change that can't be undone.
    struct Confirmation: Equatable {
        let title: String
        let message: String
        let button: String
    }

    /// What to ask before giving `member` (named `name`) `level`, or nil to
    /// just give it: any change to our own (`own`; `alone`: nobody else has
    /// our level), or a level as high as ours (`upTo`).
    static func confirmation(level: Int64, for member: Member, name: String, own: Bool, alone: Bool)
        -> Confirmation?
    {
        guard let upTo = member.manage.upTo else { return nil }
        let defaultLevel = member.manage.defaultLevel
        if own {
            var message = "You can't undo this: only someone with a higher level can raise you again."
            if alone {
                message += " Nobody else here has your level, so nobody will be able to do all you can now."
            }
            return Confirmation(
                title: "Step down to \(role(level, defaultLevel: defaultLevel))?", message: message,
                button: "Step down")
        }
        guard level >= upTo else { return nil }
        let message =
            level == 100
            ? "Admins can do everything you can, including changing the room and its people. "
                + "You can't take it back: an admin can't demote another admin."
            : "They'll have the same level as you. You can't take it back: nobody can lower "
                + "someone with their own level."
        return Confirmation(
            title: "Make \(name) \(aRole(level, defaultLevel: defaultLevel))?", message: message, button: "Change")
    }

    /// A level typed for a custom level: a number, at most `upTo`.
    static func level(typed: String, upTo: Int64) -> Int64? {
        guard let level = Int64(typed.trimmingCharacters(in: .whitespaces)), level <= upTo else { return nil }
        return level
    }

    /// Nobody joined but us has our level or more (a creator's is above
    /// all).
    static func alone(_ members: [Member], ours: String) -> Bool {
        let level = { (member: Member) in member.level ?? .max }
        let joined = members.filter { !$0.invited }
        guard let own = joined.first(where: { $0.userId == ours }) else { return false }
        return !joined.contains { $0.userId != ours && level($0) >= level(own) }
    }

    /// We may do something to `member`: change their level, remove or ban
    /// them.
    static func manageable(_ member: Member) -> Bool {
        member.manage.upTo != nil || member.manage.remove || member.manage.ban
    }
}
