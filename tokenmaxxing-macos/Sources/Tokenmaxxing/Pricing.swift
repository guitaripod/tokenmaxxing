import Foundation

/// Per-model API pricing, used to value local token history in API-equivalent
/// dollars. The Claude Max/Pro subscription bills a flat fee, so this is not
/// what the user pays — it is the retail cost the same tokens would incur on the
/// metered API, i.e. the value the subscription returns. Always an estimate.
struct Rate {
    let input: Double
    let output: Double

    static let cacheWrite5m = 1.25
    static let cacheWrite1h = 2.0
    static let cacheRead = 0.1

    /// API-equivalent dollar cost of one message's token usage.
    func cost(_ t: TokenCounts) -> Double {
        func per(_ tokens: Int64, _ rate: Double) -> Double { Double(tokens) / 1_000_000 * rate }
        return per(t.input, input)
            + per(t.output, output)
            + per(t.cacheWrite5m, input * Rate.cacheWrite5m)
            + per(t.cacheWrite1h, input * Rate.cacheWrite1h)
            + per(t.cacheRead, input * Rate.cacheRead)
    }
}

/// The token tiers that price differently. `cacheWrite5m` / `cacheWrite1h` split
/// the single `cache_creation_input_tokens` field when the finer breakdown is
/// present; otherwise all cache-write tokens fall into the 5-minute tier.
struct TokenCounts {
    var input: Int64 = 0
    var output: Int64 = 0
    var cacheWrite5m: Int64 = 0
    var cacheWrite1h: Int64 = 0
    var cacheRead: Int64 = 0
}

enum Pricing {
    /// Resolve a model id to its rate, matching Claude ids loosely so future
    /// point releases keep pricing.
    static func rate(for model: String) -> Rate {
        let m = model.lowercased()
        if m.contains("fable") || m.contains("mythos") {
            return Rate(input: 10.0, output: 50.0)
        } else if m.contains("haiku") {
            return Rate(input: 1.0, output: 5.0)
        } else if m.contains("sonnet") {
            return Rate(input: 3.0, output: 15.0)
        } else {
            return Rate(input: 5.0, output: 25.0)
        }
    }

    /// A friendly display name for a Claude model id, for legends and tables.
    static func shortName(_ model: String) -> String {
        let m = model.lowercased()
        if m.contains("opus-4-8") { return "Opus 4.8" }
        if m.contains("opus") { return "Opus" }
        if m.contains("fable") { return "Fable 5" }
        if m.contains("mythos") { return "Mythos 5" }
        if m.contains("sonnet") { return "Sonnet" }
        if m.contains("haiku") { return "Haiku" }
        if model == "<synthetic>" { return "synthetic" }
        return model
    }
}
