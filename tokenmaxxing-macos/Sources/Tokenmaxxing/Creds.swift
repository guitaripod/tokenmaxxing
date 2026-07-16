import Foundation

struct ClaudeCredentials {
    var accessToken: String
    var refreshToken: String
    var expiresAtMs: Int64
    var subscriptionType: String
    var rateLimitTier: String
}

struct GrokCredentials {
    var accessToken: String
    var refreshToken: String
    var expiresAtMs: Int64
    var oidcIssuer: String
    var oidcClientId: String
    var email: String
    var tier: Int64
    var entryKey: String
}

/// Reads the same local credential files Claude Code, Grok, and opencode write.
/// Tokenmaxxing must run un-sandboxed for these home-directory reads to succeed.
enum Creds {
    static var home: URL { FileManager.default.homeDirectoryForCurrentUser }
    static var claudePath: URL { home.appending(path: ".claude/.credentials.json") }
    static var grokAuthPath: URL { home.appending(path: ".grok/auth.json") }
    static var grokSessionsPath: URL { home.appending(path: ".grok/sessions") }
    static var opencodeAuthPath: URL { home.appending(path: ".local/share/opencode/auth.json") }
    static var opencodeDbPath: URL { home.appending(path: ".local/share/opencode/opencode.db") }

    static func loadClaude() throws -> ClaudeCredentials {
        let data = try Data(contentsOf: claudePath)
        guard
            let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            let oauth = root["claudeAiOauth"] as? [String: Any],
            let accessToken = oauth["accessToken"] as? String
        else {
            throw QuotaError.message("no claudeAiOauth block — run `claude` to sign in")
        }
        return ClaudeCredentials(
            accessToken: accessToken,
            refreshToken: oauth["refreshToken"] as? String ?? "",
            expiresAtMs: (oauth["expiresAt"] as? NSNumber)?.int64Value ?? 0,
            subscriptionType: oauth["subscriptionType"] as? String ?? "unknown",
            rateLimitTier: oauth["rateLimitTier"] as? String ?? ""
        )
    }

    /// Rewrite the OAuth tokens without disturbing sibling keys, atomically.
    static func writeBackClaude(accessToken: String, refreshToken: String, expiresAtMs: Int64) throws {
        let data = try Data(contentsOf: claudePath)
        guard var root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              var oauth = root["claudeAiOauth"] as? [String: Any]
        else {
            throw QuotaError.message("credentials file missing claudeAiOauth")
        }
        oauth["accessToken"] = accessToken
        oauth["refreshToken"] = refreshToken
        oauth["expiresAt"] = expiresAtMs
        root["claudeAiOauth"] = oauth
        let out = try JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted])
        try out.write(to: claudePath, options: [.atomic])
        try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: claudePath.path)
    }

    static func opencodeGoConfigured() -> Bool {
        guard let data = try? Data(contentsOf: opencodeAuthPath),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return false }
        return root["opencode-go"] != nil
    }

    static func loadGrok() throws -> GrokCredentials {
        let data = try Data(contentsOf: grokAuthPath)
        guard let root = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw QuotaError.message("grok auth.json is not an object — run `grok login`")
        }
        let preferred = root.first { key, value in
            key.contains("auth.x.ai")
                && ((value as? [String: Any])?["key"] as? String).map { !$0.isEmpty } == true
        } ?? root.first { _, value in
            ((value as? [String: Any])?["key"] as? String).map { !$0.isEmpty } == true
        }
        guard let (entryKey, rawEntry) = preferred,
              let entry = rawEntry as? [String: Any],
              let accessToken = entry["key"] as? String
        else {
            throw QuotaError.message("no grok session — run `grok login`")
        }
        let expiresAt = entry["expires_at"] as? String ?? ""
        return GrokCredentials(
            accessToken: accessToken,
            refreshToken: entry["refresh_token"] as? String ?? "",
            expiresAtMs: parseExpiresMs(expiresAt),
            oidcIssuer: entry["oidc_issuer"] as? String ?? "https://auth.x.ai",
            oidcClientId: entry["oidc_client_id"] as? String ?? "",
            email: entry["email"] as? String ?? "",
            tier: jwtClaimInt64(accessToken, claim: "tier") ?? 0,
            entryKey: entryKey
        )
    }

    static func writeBackGrok(entryKey: String, accessToken: String, refreshToken: String, expiresAt: String) throws {
        let data = try Data(contentsOf: grokAuthPath)
        guard var root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              var entry = root[entryKey] as? [String: Any]
        else {
            throw QuotaError.message("auth entry missing after refresh")
        }
        entry["key"] = accessToken
        entry["refresh_token"] = refreshToken
        entry["expires_at"] = expiresAt
        root[entryKey] = entry
        let out = try JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted])
        try out.write(to: grokAuthPath, options: [.atomic])
        try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: grokAuthPath.path)
    }

    private static func parseExpiresMs(_ raw: String) -> Int64 {
        guard !raw.isEmpty else { return 0 }
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: raw) {
            return Int64(date.timeIntervalSince1970 * 1000)
        }
        formatter.formatOptions = [.withInternetDateTime]
        if let date = formatter.date(from: raw) {
            return Int64(date.timeIntervalSince1970 * 1000)
        }
        return 0
    }

    private static func jwtClaimInt64(_ token: String, claim: String) -> Int64? {
        let parts = token.split(separator: ".")
        guard parts.count >= 2 else { return nil }
        var payload = String(parts[1])
        let pad = (4 - payload.count % 4) % 4
        if pad > 0 { payload += String(repeating: "=", count: pad) }
        payload = payload.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        guard let data = Data(base64Encoded: payload),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return nil }
        if let n = json[claim] as? NSNumber { return n.int64Value }
        return nil
    }
}

enum QuotaError: Error, CustomStringConvertible {
    case message(String)
    var description: String {
        switch self {
        case .message(let value): value
        }
    }
}
