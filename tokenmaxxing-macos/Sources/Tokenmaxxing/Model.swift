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
    /// The server's own severity for this window, when it reports one. Trusted
    /// over the fraction-derived threshold.
    var apiSeverity: Severity? = nil
    /// True for the limit the server marks as the current binding constraint.
    var isActive: Bool = false

    var severity: Severity { apiSeverity ?? .from(fraction) }
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
    var spend: SpendInfo? = nil

    /// The gauge the server marks active, else the most-utilized one.
    var bindingGauge: Gauge? {
        gauges.first(where: { $0.isActive }) ?? gauges.max(by: { $0.fraction < $1.fraction })
    }
}

struct Detail: Identifiable {
    let id = UUID()
    var key: String
    var value: String
}

/// Overflow-credit state from the Claude usage endpoint's `spend` block.
struct SpendInfo {
    var enabled: Bool = false
    var used: Double = 0
    var limit: Double?
    var balance: Double?
    var canPurchase: Bool = false
    var disclaimer: String?
}

/// The whole dashboard for one refresh.
struct Dashboard {
    var claudeQuota: Snapshot
    var claudeUsage: Usage
    var opencodeQuota: Snapshot
    var opencodeUsage: Usage
    var generatedAt: Date
}

/// Aggregated usage history for one provider, from local files. Dollar figures
/// are API-equivalent estimates; token counts are exact.
struct Usage {
    var scope: String = ""
    var authority: Authority = .unavailable
    var source: String = ""
    var totals = Totals()
    var windows = Windows()
    var daily: [DayPoint] = []
    var byModel: [Segment] = []
    var byProject: [Segment] = []
    var byProvider: [Segment] = []
    var tokens = TokenBreakdown()
    var heatmap = Heatmap()
    var error: String?

    var isEmpty: Bool { totals.messages == 0 }

    var avgDailyCost: Double {
        totals.activeDays == 0 ? 0 : totals.costUSD / Double(totals.activeDays)
    }

    var cacheHitRate: Double {
        let base = tokens.input + tokens.cacheRead
        return base == 0 ? 0 : Double(tokens.cacheRead) / Double(base)
    }
}

struct Totals {
    var costUSD: Double = 0
    var input: Int64 = 0
    var output: Int64 = 0
    var cacheWrite: Int64 = 0
    var cacheRead: Int64 = 0
    var messages: Int64 = 0
    var sessions: Int64 = 0
    var activeDays: Int64 = 0
    var webSearch: Int64 = 0
    var webFetch: Int64 = 0
    var firstDay: Date?
    var lastDay: Date?

    var totalTokens: Int64 { input + output + cacheWrite + cacheRead }
}

struct DayPoint: Identifiable {
    let id = UUID()
    var date: Date
    var cost: Double
    var tokens: Int64
    var messages: Int64
}

struct Segment: Identifiable {
    let id = UUID()
    var label: String
    var cost: Double
    var tokens: Int64
    var messages: Int64
}

/// The token tiers that price differently, for the composition breakdown.
struct TokenBreakdown {
    var input: Int64 = 0
    var output: Int64 = 0
    var cacheWrite: Int64 = 0
    var cacheRead: Int64 = 0
    var reasoning: Int64 = 0

    var total: Int64 { input + output + cacheWrite + cacheRead + reasoning }
}

struct WinStat {
    var cost: Double = 0
    var tokens: Int64 = 0
    var messages: Int64 = 0
}

struct Windows {
    var today = WinStat()
    var seven = WinStat()
    var thirty = WinStat()
}

/// Activity by local weekday (0 = Monday) and hour, for the punch-card heatmap.
struct Heatmap {
    var counts: [[Int]] = Array(repeating: Array(repeating: 0, count: 24), count: 7)
    var max: Int = 0
}
