import Foundation

/// KakaoTalk's list and message views expose time as Korean locale labels
/// (`"오전 11:17"`, `"오후 12:12"`, `"어제"`, `"2026년 8월 1일"`), never ISO.
/// This does a best-effort conversion to an ISO 8601 UTC string; when a label
/// carries no usable time it returns `""` (the contract allows an empty `at`
/// for list previews — see `docs/adapter-contract.md`).
enum KoreanTime {
    private static var cal: Calendar {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone.current
        return c
    }

    private static let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        f.timeZone = TimeZone(identifier: "UTC")
        return f
    }()

    /// `label` is a KakaoTalk time string. `now` is injectable for tests.
    static func toISO(_ label: String, now: Date = Date()) -> String {
        let s = label.trimmingCharacters(in: .whitespaces)
        guard !s.isEmpty else { return "" }

        // "오전/오후 H:MM"  ->  today (or yesterday if `dayOffset` given elsewhere)
        if let hm = parseHourMinute(s) {
            return iso.string(from: combine(day: now, hour: hm.0, minute: hm.1))
        }
        // "어제"
        if s == "어제" {
            let y = cal.date(byAdding: .day, value: -1, to: now) ?? now
            return iso.string(from: cal.startOfDay(for: y))
        }
        // "YYYY년 M월 D일"
        if let d = parseYMD(s) {
            return iso.string(from: d)
        }
        return ""
    }

    /// Parse "오전 11:17" / "오후 12:12" -> 24h (hour, minute).
    static func parseHourMinute(_ s: String) -> (Int, Int)? {
        let ampm: Int
        var rest = s
        if s.hasPrefix("오전") { ampm = 0; rest = String(s.dropFirst(2)) }
        else if s.hasPrefix("오후") { ampm = 12; rest = String(s.dropFirst(2)) }
        else { return nil }

        let parts = rest.trimmingCharacters(in: .whitespaces).split(separator: ":")
        guard parts.count == 2, var h = Int(parts[0]), let m = Int(parts[1]) else { return nil }
        // 오전 12:xx -> 00:xx ; 오후 12:xx -> 12:xx
        if h == 12 { h = 0 }
        return (h + ampm, m)
    }

    private static func parseYMD(_ s: String) -> Date? {
        let digits = s.split(whereSeparator: { !$0.isNumber }).compactMap { Int($0) }
        guard digits.count == 3 else { return nil }
        var dc = DateComponents()
        dc.year = digits[0]; dc.month = digits[1]; dc.day = digits[2]
        return cal.date(from: dc)
    }

    private static func combine(day: Date, hour: Int, minute: Int) -> Date {
        var dc = cal.dateComponents([.year, .month, .day], from: day)
        dc.hour = hour; dc.minute = minute; dc.second = 0
        return cal.date(from: dc) ?? day
    }
}
