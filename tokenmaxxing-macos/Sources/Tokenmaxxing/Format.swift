import Foundation

/// Compact number formatting shared across the dashboard and export.
enum Fmt {
    static func count(_ n: Int64) -> String {
        let value = Double(n)
        if value >= 1e12 { return String(format: "%.1fT", value / 1e12) }
        if value >= 1e9 { return String(format: "%.1fB", value / 1e9) }
        if value >= 1e6 { return String(format: "%.1fM", value / 1e6) }
        if value >= 1e3 { return String(format: "%.1fK", value / 1e3) }
        return "\(n)"
    }

    static func usd(_ v: Double) -> String {
        let a = abs(v)
        if a >= 1e6 { return String(format: "$%.2fM", v / 1e6) }
        if a >= 10_000 { return String(format: "$%.1fK", v / 1e3) }
        if a >= 100 { return String(format: "$%.0f", v) }
        return String(format: "$%.2f", v)
    }

    static func usdCents(_ v: Double) -> String { String(format: "$%.2f", v) }

    static func until(_ seconds: Int) -> String {
        let s = max(0, seconds)
        let (days, hours, minutes) = (s / 86_400, (s % 86_400) / 3_600, (s % 3_600) / 60)
        if days > 0 { return "\(days)d \(hours)h" }
        if hours > 0 { return "\(hours)h \(minutes)m" }
        return "\(minutes)m"
    }

    static func until(_ date: Date) -> String {
        until(Int(date.timeIntervalSinceNow))
    }

    static func percent(_ fraction: Double) -> String {
        "\(Int((fraction * 100).rounded()))%"
    }
}
