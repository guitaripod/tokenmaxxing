import Foundation

enum Authority {
    case live
    case estimated
    case unavailable

    var badge: String {
        switch self {
        case .live: "LIVE"
        case .estimated: "EST"
        case .unavailable: "OFFLINE"
        }
    }
}

enum Severity {
    case nominal
    case warn
    case critical

    static func from(_ fraction: Double) -> Severity {
        if fraction >= 0.85 { .critical } else if fraction >= 0.60 { .warn } else { .nominal }
    }
}

enum Unit {
    case percent
    case usd
}

struct Gauge: Identifiable {
    let id = UUID()
    var key: String
    var label: String
    var fraction: Double
    var used: Double?
    var limit: Double?
    var unit: Unit
    var detail: String?
    var resetsAt: Date?
    var trustedReset: Bool

    var severity: Severity { .from(fraction) }
    var percentText: String { "\(Int((fraction * 100).rounded()))%" }

    /// The line beneath a ring: dollar caps, request counts, and reset time.
    var subline: String? {
        var parts: [String] = []
        if unit == .usd, let used, let limit {
            parts.append(String(format: "$%.2f / $%.0f", used, limit))
        }
        if let detail { parts.append(detail) }
        if let resetsAt {
            let human = Self.humanize(until: resetsAt)
            parts.append(trustedReset ? "resets \(human)" : "~resets \(human)")
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    static func humanize(until date: Date) -> String {
        let seconds = max(0, Int(date.timeIntervalSinceNow))
        if seconds < 3600 { return "\(seconds / 60)m" }
        if seconds < 86_400 { return "\(seconds / 3600)h \((seconds % 3600) / 60)m" }
        return "\(seconds / 86_400)d \((seconds % 86_400) / 3600)h"
    }
}

struct Snapshot: Identifiable {
    let id = UUID()
    var providerId: String
    var providerName: String
    var subtitle: String
    var authority: Authority
    var source: String
    var gauges: [Gauge]
    var details: [Detail]
    var note: String?
    var error: String?
}

struct Detail: Identifiable {
    let id = UUID()
    var key: String
    var value: String
}
