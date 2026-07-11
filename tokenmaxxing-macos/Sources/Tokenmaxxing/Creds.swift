import Foundation

struct ClaudeCredentials {
    var accessToken: String
    var refreshToken: String
    var expiresAtMs: Int64
    var subscriptionType: String
    var rateLimitTier: String
}

/// Reads the same local credential files Claude Code and opencode write. Tokenmaxxing
/// must run un-sandboxed for these home-directory reads to succeed.
enum Creds {
    static var home: URL { FileManager.default.homeDirectoryForCurrentUser }
    static var claudePath: URL { home.appending(path: ".claude/.credentials.json") }
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
}

enum QuotaError: Error, CustomStringConvertible {
    case message(String)
    var description: String {
        switch self {
        case .message(let value): value
        }
    }
}
