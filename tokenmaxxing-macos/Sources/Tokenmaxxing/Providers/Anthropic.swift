import Foundation

/// Live Claude Max/Pro quota from the same OAuth endpoint Claude Code's
/// `/usage` command uses. Survives HTTP 429 by serving the last good body from
/// disk and imposing a multi-minute cooldown so we stop hammering the endpoint.
enum AnthropicProvider {
    static let usageURL = URL(string: "https://api.anthropic.com/api/oauth/usage")!
    static let tokenURL = URL(string: "https://platform.claude.com/v1/oauth/token")!
    static let clientId = "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
    static let oauthBeta = "oauth-2025-04-20"
    static let liveInterval: TimeInterval = 120
    static let rateLimitFloor: TimeInterval = 5 * 60
    static let failCooldown: TimeInterval = 90

    struct FetchResult {
        var snapshot: Snapshot
        var cooldown: TimeInterval
        var fresh: Bool
    }

    static func fetch() async -> FetchResult {
        do {
            let (body, creds) = try await loadAndFetch()
            saveUsageCache(body)
            return FetchResult(snapshot: parse(body, creds: creds), cooldown: liveInterval, fresh: true)
        } catch let err as RateLimitedError {
            let cooldown = max(err.retryAfter ?? rateLimitFloor, rateLimitFloor)
            return FetchResult(
                snapshot: fallbackSnapshot(message: err.message, reason: "rate limited"),
                cooldown: cooldown,
                fresh: false
            )
        } catch {
            return FetchResult(
                snapshot: fallbackSnapshot(message: "\(error)", reason: "cached"),
                cooldown: failCooldown,
                fresh: false
            )
        }
    }

    private struct RateLimitedError: Error {
        var message: String
        var retryAfter: TimeInterval?
    }

    private static func loadAndFetch() async throws -> ([String: Any], ClaudeCredentials) {
        var creds = try Creds.loadClaude()
        let nowMs = Int64(Date().timeIntervalSince1970 * 1000)
        if creds.expiresAtMs > 0, nowMs >= creds.expiresAtMs - 120_000 {
            if let fresh = try? await refresh(creds.refreshToken) { creds = fresh }
        }

        let first = try await getUsage(creds.accessToken)
        if first.status == 401 || first.status == 403 {
            let fresh = try await refresh(creds.refreshToken)
            let retry = try await getUsage(fresh.accessToken)
            try throwIfBad(retry.status, retryAfter: retry.retryAfter)
            return (retry.json, fresh)
        }
        try throwIfBad(first.status, retryAfter: first.retryAfter)
        return (first.json, creds)
    }

    private static func throwIfBad(_ status: Int, retryAfter: TimeInterval?) throws {
        if status == 200 { return }
        if status == 429 {
            throw RateLimitedError(message: "usage endpoint rate limited (HTTP 429)", retryAfter: retryAfter)
        }
        throw QuotaError.message("usage endpoint returned \(status)")
    }

    private static func getUsage(_ token: String) async throws -> (status: Int, json: [String: Any], retryAfter: TimeInterval?) {
        var request = URLRequest(url: usageURL)
        request.setValue("Bearer \(token)", forHTTPHeaderField: "authorization")
        request.setValue(oauthBeta, forHTTPHeaderField: "anthropic-beta")
        request.setValue("application/json", forHTTPHeaderField: "accept")
        request.setValue("tokenmaxxing/0.2.0 (+https://github.com/guitaripod/tokenmaxxing)", forHTTPHeaderField: "user-agent")
        let (data, response) = try await URLSession.shared.data(for: request)
        let http = response as? HTTPURLResponse
        let status = http?.statusCode ?? 0
        let retryAfter: TimeInterval? = {
            guard let raw = http?.value(forHTTPHeaderField: "Retry-After")?.trimmingCharacters(in: .whitespaces),
                  let seconds = TimeInterval(raw)
            else { return nil }
            return max(1, seconds)
        }()
        let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        return (status, json, retryAfter)
    }

    private static func fallbackSnapshot(message: String, reason: String) -> Snapshot {
        if let body = loadUsageCache(), let creds = try? Creds.loadClaude() {
            var snap = parse(body, creds: creds)
            snap.source = "api.anthropic.com · \(reason)"
            snap.note = "Showing last good reading — \(message)"
            snap.error = nil
            return snap
        }
        return unavailable(message)
    }

    private static var cacheURL: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appending(path: "Library/Application Support/tokenmaxxing/claude_usage_cache.json")
    }

    private static func saveUsageCache(_ body: [String: Any]) {
        let payload: [String: Any] = [
            "saved_at_ms": Int64(Date().timeIntervalSince1970 * 1000),
            "body": body,
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted]) else { return }
        let url = cacheURL
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: url, options: [.atomic])
    }

    private static func loadUsageCache() -> [String: Any]? {
        guard let data = try? Data(contentsOf: cacheURL),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let body = root["body"] as? [String: Any]
        else { return nil }
        return body
    }

    private static func parse(_ json: [String: Any], creds: ClaudeCredentials) -> Snapshot {
        var gauges: [Gauge] = []
        if let limits = json["limits"] as? [[String: Any]] {
            gauges = limits.compactMap(gaugeFromLimit)
        }
        if gauges.isEmpty { gauges = gaugesFromTopLevel(json) }
        if let extra = gaugeFromExtra(json) { gauges.append(extra) }

        return Snapshot(
            providerId: "anthropic",
            providerName: "Claude",
            subtitle: subtitle(creds),
            authority: .live,
            source: "api.anthropic.com · live",
            gauges: gauges,
            details: details(json),
            note: nil,
            error: nil,
            spend: spendInfo(json)
        )
    }

    private static func gaugeFromLimit(_ item: [String: Any]) -> Gauge? {
        guard let kind = item["kind"] as? String,
              let percent = (item["percent"] as? NSNumber)?.doubleValue
        else { return nil }
        let model = ((item["scope"] as? [String: Any])?["model"] as? [String: Any])?["display_name"] as? String
        let label: String
        switch (kind, model) {
        case ("session", _): label = "5-hour session"
        case ("weekly_all", _): label = "Weekly · all models"
        case ("weekly_scoped", .some(let name)): label = "Weekly · \(name)"
        case ("weekly_scoped", .none): label = "Weekly · scoped"
        case (let other, .some(let name)): label = "\(pretty(other)) · \(name)"
        case (let other, .none): label = pretty(other)
        }
        return Gauge(
            key: kind, label: label,
            fraction: min(1, max(0, percent / 100)),
            used: nil, limit: nil, unit: .percent,
            detail: nil, resetsAt: parseTimestamp(item["resets_at"]),
            trustedReset: kind == "session",
            apiSeverity: severity(from: item["severity"] as? String),
            isActive: (item["is_active"] as? NSNumber)?.boolValue ?? false
        )
    }

    /// Prefer the server's own severity over the fraction-derived threshold.
    private static func severity(from value: String?) -> Severity? {
        switch value {
        case "critical": return .critical
        case "warn", "warning": return .warn
        case "normal", "ok": return .nominal
        default: return nil
        }
    }

    /// The prepaid / pay-as-you-go credit block, when the plan exposes one.
    private static func spendInfo(_ json: [String: Any]) -> SpendInfo? {
        guard let spend = json["spend"] as? [String: Any] else { return nil }
        return SpendInfo(
            enabled: (spend["enabled"] as? NSNumber)?.boolValue ?? false,
            used: money(spend["used"]) ?? 0,
            limit: money(spend["limit"]),
            balance: money(spend["balance"]),
            canPurchase: (spend["can_purchase_credits"] as? NSNumber)?.boolValue ?? false,
            disclaimer: (spend["disclaimer"] as? String).map(stripMarkdownLinks)
        )
    }

    /// Accepts a bare dollar number or a `{amount_minor, exponent}` object → dollars.
    private static func money(_ value: Any?) -> Double? {
        if let n = value as? NSNumber, !(value is NSNull) { return n.doubleValue }
        guard let dict = value as? [String: Any],
              let minor = (dict["amount_minor"] as? NSNumber)?.doubleValue
        else { return nil }
        let exponent = (dict["exponent"] as? NSNumber)?.intValue ?? 2
        return minor / pow(10.0, Double(exponent))
    }

    /// Reduce `[label](url)` markdown links to their label for plain rendering.
    private static func stripMarkdownLinks(_ text: String) -> String {
        var out = ""
        let chars = Array(text)
        var i = 0
        while i < chars.count {
            if chars[i] == "[" {
                i += 1
                var label = ""
                while i < chars.count, chars[i] != "]" { label.append(chars[i]); i += 1 }
                if i < chars.count { i += 1 } // skip ']'
                if i < chars.count, chars[i] == "(" {
                    while i < chars.count, chars[i] != ")" { i += 1 }
                    if i < chars.count { i += 1 } // skip ')'
                }
                out += label
            } else {
                out.append(chars[i])
                i += 1
            }
        }
        return out
    }

    private static func gaugesFromTopLevel(_ json: [String: Any]) -> [Gauge] {
        [("five_hour", "5-hour session", true), ("seven_day", "Weekly · all models", false)]
            .compactMap { key, label, trusted in
                guard let obj = json[key] as? [String: Any],
                      let utilization = (obj["utilization"] as? NSNumber)?.doubleValue
                else { return nil }
                return Gauge(
                    key: key, label: label,
                    fraction: min(1, max(0, utilization / 100)),
                    used: nil, limit: nil, unit: .percent,
                    detail: nil, resetsAt: parseTimestamp(obj["resets_at"]),
                    trustedReset: trusted
                )
            }
    }

    private static func gaugeFromExtra(_ json: [String: Any]) -> Gauge? {
        guard let extra = json["extra_usage"] as? [String: Any],
              (extra["is_enabled"] as? NSNumber)?.boolValue == true,
              let utilization = (extra["utilization"] as? NSNumber)?.doubleValue
        else { return nil }
        return Gauge(
            key: "extra_usage", label: "Extra usage credits",
            fraction: min(1, max(0, utilization / 100)),
            used: (extra["used_credits"] as? NSNumber)?.doubleValue,
            limit: (extra["monthly_limit"] as? NSNumber)?.doubleValue,
            unit: .usd, detail: nil, resetsAt: nil, trustedReset: false
        )
    }

    private static func details(_ json: [String: Any]) -> [Detail] {
        var details: [Detail] = []
        if let ts = parseTimestamp((json["five_hour"] as? [String: Any])?["resets_at"]) {
            details.append(Detail(key: "Session resets", value: "in \(Gauge.humanize(until: ts))"))
        }
        if let ts = parseTimestamp((json["seven_day"] as? [String: Any])?["resets_at"]) {
            details.append(Detail(key: "Weekly resets", value: "in \(Gauge.humanize(until: ts))"))
        }
        let extraEnabled = ((json["extra_usage"] as? [String: Any])?["is_enabled"] as? NSNumber)?.boolValue ?? false
        details.append(Detail(key: "Extra usage credits", value: extraEnabled ? "enabled" : "disabled"))
        return details
    }

    private static func subtitle(_ creds: ClaudeCredentials) -> String {
        let plan: String
        switch creds.subscriptionType {
        case "max": plan = "Max"
        case "pro": plan = "Pro"
        default: plan = creds.subscriptionType
        }
        if let multiplier = creds.rateLimitTier.split(separator: "_").last, multiplier.hasSuffix("x") {
            return "\(plan) · \(multiplier.dropLast())×"
        }
        return plan
    }

    private static func pretty(_ kind: String) -> String {
        kind.replacingOccurrences(of: "_", with: " ").capitalized
    }

    private static func parseTimestamp(_ value: Any?) -> Date? {
        guard let raw = value as? String else { return nil }
        let iso = ISO8601DateFormatter()
        iso.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = iso.date(from: raw) { return date }
        iso.formatOptions = [.withInternetDateTime]
        if let range = raw.range(of: #"\.\d+"#, options: .regularExpression) {
            var trimmed = raw
            trimmed.removeSubrange(range)
            return iso.date(from: trimmed)
        }
        return iso.date(from: raw)
    }

    private static func refresh(_ refreshToken: String) async throws -> ClaudeCredentials {
        guard !refreshToken.isEmpty else {
            throw QuotaError.message("no refresh token — run `claude` to sign in")
        }
        var request = URLRequest(url: tokenURL)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "content-type")
        request.httpBody = try JSONSerialization.data(withJSONObject: [
            "grant_type": "refresh_token",
            "refresh_token": refreshToken,
            "client_id": clientId,
        ])
        let (data, response) = try await URLSession.shared.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(status) else {
            throw QuotaError.message("token refresh returned \(status)")
        }
        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let access = json["access_token"] as? String
        else { throw QuotaError.message("no access_token in refresh response") }
        let newRefresh = json["refresh_token"] as? String ?? refreshToken
        let expiresIn = (json["expires_in"] as? NSNumber)?.int64Value ?? 28_800
        let expiresAtMs = Int64(Date().timeIntervalSince1970 * 1000) + expiresIn * 1000
        try Creds.writeBackClaude(accessToken: access, refreshToken: newRefresh, expiresAtMs: expiresAtMs)
        return try Creds.loadClaude()
    }

    private static func unavailable(_ error: String) -> Snapshot {
        Snapshot(
            providerId: "anthropic",
            providerName: "Claude",
            subtitle: "Claude Max",
            authority: .unavailable,
            source: "api.anthropic.com · unreachable",
            gauges: [],
            details: [],
            note: nil,
            error: error
        )
    }
}
