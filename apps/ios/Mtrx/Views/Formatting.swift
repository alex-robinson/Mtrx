import Foundation

enum Formatting {
    /// A message's time, e.g. "14:05".
    static func time(_ date: Date) -> String {
        date.formatted(date: .omitted, time: .shortened)
    }

    /// A room's last activity in the list: the time today, "Yesterday",
    /// the weekday within a week, else the date.
    static func lastActivity(_ date: Date, now: Date = .now, calendar: Calendar = .current) -> String {
        if calendar.isDateInToday(date) {
            return time(date)
        }
        if calendar.isDateInYesterday(date) {
            return "Yesterday"
        }
        if let days = calendar.dateComponents([.day], from: date, to: now).day, days < 7 {
            return date.formatted(.dateTime.weekday(.wide))
        }
        return date.formatted(date: .numeric, time: .omitted)
    }

    /// A day divider: "Today", "Yesterday", "Monday 3 March", or with the
    /// year if not this one.
    static func day(_ date: Date, now: Date = .now, calendar: Calendar = .current) -> String {
        if calendar.isDateInToday(date) {
            return "Today"
        }
        if calendar.isDateInYesterday(date) {
            return "Yesterday"
        }
        if calendar.isDate(date, equalTo: now, toGranularity: .year) {
            return date.formatted(.dateTime.weekday(.wide).day().month(.wide))
        }
        return date.formatted(.dateTime.day().month(.wide).year())
    }

    static func bytes(_ count: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(clamping: count), countStyle: .file)
    }

    private static let links = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue)

    /// `text` with its web addresses as links.
    static func linked(_ text: String) -> AttributedString {
        var result = AttributedString(text)
        guard let links else { return result }
        let whole = NSRange(text.startIndex..., in: text)
        for match in links.matches(in: text, range: whole) {
            guard let url = match.url,
                  let range = Range(match.range, in: text),
                  let start = AttributedString.Index(range.lowerBound, within: result),
                  let end = AttributedString.Index(range.upperBound, within: result)
            else { continue }
            result[start..<end].link = url
        }
        return result
    }
}
