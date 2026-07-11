import Foundation

/// Live Claude Max/Pro quota from the same OAuth endpoint Claude Code's
/// `/usage` command uses.
enum AnthropicProvider {
    static let usageURL = URL(string: "https://api.anthropic.com/api/oauth/usage")!
    static let tokenURL = URL(string: "https://platform.claude.com/v1/oauth/token")!
    static let clientId = "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
    static let oauthBeta = "oauth-2025-04-20"

    static func fetch() async -> Snapshot {
        do {
            return try await loadAndFetch()
        } catch {
            return unavailable("\(error)")
        }
    }

    private static func loadAndFetch() async throws -> Snapshot {
        var creds = try Creds.loadClaude()
        let nowMs = Int64(Date().timeIntervalSince1970 * 1000)
        if creds.expiresAtMs > 0, nowMs >= creds.expiresAtMs - 120_000 {
            if let fresh = try? await refresh(creds.refreshToken) { creds = fresh }
        }

        let (status, body) = try await getUsage(creds.accessToken)
        if status == 401 || status == 403 {
            let fresh = try await refresh(creds.refreshToken)
            let (retryStatus, retryBody) = try await getUsage(fresh.accessToken)
            guard retryStatus == 200 else {
                throw QuotaError.message("usage endpoint returned \(retryStatus) after refresh")
            }
            return parse(retryBody, creds: fresh)
        }
        guard status == 200 else { throw QuotaError.message("usage endpoint returned \(status)") }
        return parse(body, creds: creds)
    }

    private static func getUsage(_ token: String) async throws -> (Int, [String: Any]) {
        var request = URLRequest(url: usageURL)
        request.setValue("Bearer \(token)", forHTTPHeaderField: "authorization")
        request.setValue(oauthBeta, forHTTPHeaderField: "anthropic-beta")
        request.setValue("application/json", forHTTPHeaderField: "accept")
        request.setValue("prism/0.1.0 (+https://github.com/guitaripod/quota)", forHTTPHeaderField: "user-agent")
        let (data, response) = try await URLSession.shared.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        return (status, json)
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
            error: nil
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
            trustedReset: kind == "session"
        )
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
