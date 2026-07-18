import Foundation

/// Live Grok Build quota from the same billing endpoint the Grok CLI's `/usage`
/// command uses (`cli-chat-proxy.grok.com/v1/billing`).
enum GrokProvider {
    static let creditsURL = URL(string: "https://cli-chat-proxy.grok.com/v1/billing?format=credits")!
    static let billingURL = URL(string: "https://cli-chat-proxy.grok.com/v1/billing")!
    static let clientVersion = "0.2.101"
    static let userAgent = "tokenmaxxing/\(AppVersion.current) (+https://github.com/guitaripod/tokenmaxxing)"

    static func fetch() async -> Snapshot {
        do {
            return try await loadAndFetch()
        } catch {
            return unavailable("\(error)")
        }
    }

    private static func loadAndFetch() async throws -> Snapshot {
        var creds = try Creds.loadGrok()
        let nowMs = Int64(Date().timeIntervalSince1970 * 1000)
        if creds.expiresAtMs > 0, nowMs >= creds.expiresAtMs - 120_000 {
            if let fresh = try? await refresh(creds) { creds = fresh }
        }

        let (status, credits) = try await getJSON(creditsURL, token: creds.accessToken)
        if status == 401 || status == 403 {
            let fresh = try await refresh(creds)
            let (retryStatus, retryBody) = try await getJSON(creditsURL, token: fresh.accessToken)
            guard retryStatus == 200 else {
                throw QuotaError.message("billing endpoint returned \(retryStatus) after refresh")
            }
            let dollars = try? await getJSON(billingURL, token: fresh.accessToken)
            return parse(retryBody, dollars: dollars?.1, creds: fresh)
        }
        guard status == 200 else { throw QuotaError.message("billing endpoint returned \(status)") }
        let dollars = try? await getJSON(billingURL, token: creds.accessToken)
        return parse(credits, dollars: dollars?.1, creds: creds)
    }

    private static func getJSON(_ url: URL, token: String) async throws -> (Int, [String: Any]) {
        var request = URLRequest(url: url)
        request.setValue("Bearer \(token)", forHTTPHeaderField: "authorization")
        request.setValue("application/json", forHTTPHeaderField: "accept")
        request.setValue(clientVersion, forHTTPHeaderField: "x-grok-client-version")
        request.setValue("cli", forHTTPHeaderField: "x-grok-client-mode")
        request.setValue(userAgent, forHTTPHeaderField: "user-agent")
        let (data, response) = try await URLSession.shared.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        return (status, json)
    }

    private static func parse(_ credits: [String: Any], dollars: [String: Any]?, creds: GrokCredentials) -> Snapshot {
        let config = credits["config"] as? [String: Any] ?? [:]
        var gauges: [Gauge] = []

        let weeklyPct = (config["creditUsagePercent"] as? NSNumber)?.doubleValue ?? 0
        let weeklyReset = parseTimestamp(
            (config["currentPeriod"] as? [String: Any])?["end"] as? String
                ?? config["billingPeriodEnd"] as? String
        )
        gauges.append(Gauge(
            key: "weekly", label: "Weekly credits",
            fraction: min(1, max(0, weeklyPct / 100)),
            used: nil, limit: nil, unit: .percent,
            detail: nil, resetsAt: weeklyReset, trustedReset: true
        ))

        if let products = config["productUsage"] as? [[String: Any]] {
            for product in products {
                guard let name = product["product"] as? String,
                      let pct = (product["usagePercent"] as? NSNumber)?.doubleValue
                else { continue }
                gauges.append(Gauge(
                    key: "product_\(name.lowercased())",
                    label: prettyProduct(name),
                    fraction: min(1, max(0, pct / 100)),
                    used: nil, limit: nil, unit: .percent,
                    detail: nil, resetsAt: weeklyReset, trustedReset: true
                ))
            }
        }

        let onCap = moneyCents(config["onDemandCap"])
        let onUsed = moneyCents(config["onDemandUsed"]) ?? 0
        if let cap = onCap, cap > 0 {
            gauges.append(Gauge(
                key: "on_demand", label: "Pay-as-you-go",
                fraction: min(1, max(0, onUsed / cap)),
                used: onUsed, limit: cap, unit: .usd,
                detail: nil, resetsAt: nil, trustedReset: false
            ))
        }

        if let dollars,
           let dcfg = dollars["config"] as? [String: Any],
           let used = moneyCents(dcfg["used"]),
           let limit = moneyCents(dcfg["monthlyLimit"]), limit > 0
        {
            gauges.append(Gauge(
                key: "monthly", label: "Monthly spend",
                fraction: min(1, max(0, used / limit)),
                used: used, limit: limit, unit: .usd,
                detail: nil,
                resetsAt: parseTimestamp(dcfg["billingPeriodEnd"] as? String),
                trustedReset: false
            ))
        }

        markBinding(&gauges)

        let prepaid = moneyCents(config["prepaidBalance"])
        let spend: SpendInfo? = prepaid.map { balance in
            SpendInfo(
                enabled: balance > 0 || (onCap ?? 0) > 0,
                used: onUsed,
                limit: onCap,
                balance: balance,
                canPurchase: true,
                disclaimer: "Prepaid balance / on-demand from grok.com billing"
            )
        }

        return Snapshot(
            providerId: "xai",
            providerName: "Grok",
            subtitle: subtitle(creds),
            authority: .live,
            source: "cli-chat-proxy.grok.com · live",
            gauges: gauges,
            details: details(config, creds: creds, prepaid: prepaid),
            note: nil,
            error: nil,
            spend: spend
        )
    }

    private static func markBinding(_ gauges: inout [Gauge]) {
        guard let idx = gauges.indices.max(by: { gauges[$0].fraction < gauges[$1].fraction }) else { return }
        gauges[idx].isActive = true
    }

    private static func moneyCents(_ value: Any?) -> Double? {
        if let n = value as? NSNumber { return n.doubleValue / 100 }
        if let obj = value as? [String: Any], let n = obj["val"] as? NSNumber {
            return n.doubleValue / 100
        }
        return nil
    }

    private static func prettyProduct(_ name: String) -> String {
        switch name {
        case "GrokBuild": "Grok Build"
        case "Api", "API": "API"
        default: name
        }
    }

    private static func subtitle(_ creds: GrokCredentials) -> String {
        switch creds.tier {
        case 0: "Free · live"
        case 1: "Basic · live"
        case 2: "SuperGrok · live"
        case 3: "X Premium · live"
        case let n where n > 3: "Tier \(n) · live"
        default: "Grok · live"
        }
    }

    private static func details(_ config: [String: Any], creds: GrokCredentials, prepaid: Double?) -> [Detail] {
        var rows: [Detail] = []
        if !creds.email.isEmpty {
            rows.append(Detail(key: "Account", value: creds.email))
        }
        if let end = (config["currentPeriod"] as? [String: Any])?["end"] as? String
            ?? config["billingPeriodEnd"] as? String,
           let ts = parseTimestamp(end)
        {
            rows.append(Detail(key: "Weekly resets", value: Gauge.humanize(until: ts)))
        }
        if let prepaid {
            rows.append(Detail(key: "Prepaid balance", value: String(format: "$%.2f", prepaid)))
        }
        let period = ((config["currentPeriod"] as? [String: Any])?["type"] as? String
            ?? "weekly").replacingOccurrences(of: "USAGE_PERIOD_TYPE_", with: "").lowercased()
        rows.append(Detail(key: "Period", value: period))
        return rows
    }

    private static func parseTimestamp(_ raw: String?) -> Date? {
        guard let raw else { return nil }
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: raw) { return date }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: raw)
    }

    private static func refresh(_ creds: GrokCredentials) async throws -> GrokCredentials {
        guard !creds.refreshToken.isEmpty else {
            throw QuotaError.message("no refresh token — run `grok login`")
        }
        guard !creds.oidcClientId.isEmpty else {
            throw QuotaError.message("no OIDC client id in grok auth")
        }
        let issuer = creds.oidcIssuer.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        guard let url = URL(string: "\(issuer)/oauth2/token") else {
            throw QuotaError.message("bad OIDC issuer")
        }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "content-type")
        request.setValue("application/json", forHTTPHeaderField: "accept")
        let body = [
            "grant_type=refresh_token",
            "refresh_token=\(formEncode(creds.refreshToken))",
            "client_id=\(formEncode(creds.oidcClientId))",
        ].joined(separator: "&")
        request.httpBody = body.data(using: .utf8)
        let (data, response) = try await URLSession.shared.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard status >= 200, status < 300 else {
            throw QuotaError.message("token refresh returned \(status)")
        }
        let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        guard let access = json["access_token"] as? String else {
            throw QuotaError.message("no access_token in refresh response")
        }
        let newRefresh = json["refresh_token"] as? String ?? creds.refreshToken
        let expiresIn = (json["expires_in"] as? NSNumber)?.intValue ?? 21_600
        let expiresAt = ISO8601DateFormatter().string(from: Date().addingTimeInterval(TimeInterval(expiresIn)))
        try Creds.writeBackGrok(
            entryKey: creds.entryKey,
            accessToken: access,
            refreshToken: newRefresh,
            expiresAt: expiresAt
        )
        return try Creds.loadGrok()
    }

    private static func formEncode(_ value: String) -> String {
        var allowed = CharacterSet.alphanumerics
        allowed.insert(charactersIn: "-._~")
        return value.addingPercentEncoding(withAllowedCharacters: allowed)?
            .replacingOccurrences(of: " ", with: "+") ?? value
    }

    private static func unavailable(_ error: String) -> Snapshot {
        Snapshot(
            providerId: "xai",
            providerName: "Grok",
            subtitle: "Grok Build",
            authority: .unavailable,
            source: "cli-chat-proxy.grok.com · unreachable",
            gauges: [],
            details: [],
            note: nil,
            error: error
        )
    }
}
