import Foundation

struct OpenCodeRemoteWindow: Sendable {
    var spend = 0.0
    var requests: Int64 = 0
}

struct OpenCodeRemoteStats: Sendable {
    var windows: [String: OpenCodeRemoteWindow] = [:]
    var allTimeSpend = 0.0
    var sessions: Int64 = 0
    var tokensIn: Int64 = 0
    var tokensOut: Int64 = 0
    var cacheRead: Int64 = 0

    mutating func add(_ other: OpenCodeRemoteStats) {
        for (key, win) in other.windows {
            var merged = windows[key] ?? OpenCodeRemoteWindow()
            merged.spend += win.spend
            merged.requests += win.requests
            windows[key] = merged
        }
        allTimeSpend += other.allTimeSpend
        sessions += other.sessions
        tokensIn += other.tokensIn
        tokensOut += other.tokensOut
        cacheRead += other.cacheRead
    }
}

struct OpenCodeRemoteReport: Sendable {
    var configured: [String] = []
    var reached: [String] = []
    var stale: [String] = []
    var unreachable: [String] = []
    var stats = OpenCodeRemoteStats()

    var included: [String] { reached + stale }
}

/// Sums opencode-go spend from other machines' opencode.db over SSH, so the
/// rolling-cap estimate covers the whole account instead of just this machine.
/// Hosts come from `opencode_remote_hosts` in ~/.config/tokenmaxxing/config.json
/// and must be reachable via non-interactive `ssh <host>` (e.g. Tailscale peers).
enum OpenCodeRemote {
    static let remoteDbPath = ".local/share/opencode/opencode.db"
    private static let freshTTL: TimeInterval = 60
    private static let staleTTL: TimeInterval = 15 * 60
    private static let sshTimeout: TimeInterval = 15

    static func report() -> OpenCodeRemoteReport {
        var report = OpenCodeRemoteReport()
        report.configured = configuredHosts()
        let nowMs = Int64(Date().timeIntervalSince1970 * 1000)
        for host in report.configured {
            if let entry = Cache.shared.entry(for: host), entry.age < freshTTL {
                report.stats.add(entry.stats)
                report.reached.append(host)
                continue
            }
            do {
                let stats = try query(host: host, nowMs: nowMs)
                Cache.shared.store(stats, for: host)
                report.stats.add(stats)
                report.reached.append(host)
            } catch {
                if let entry = Cache.shared.entry(for: host), entry.age < staleTTL {
                    report.stats.add(entry.stats)
                    report.stale.append(host)
                } else {
                    report.unreachable.append(host)
                }
            }
        }
        return report
    }

    static func configuredHosts() -> [String] {
        let base = ProcessInfo.processInfo.environment["XDG_CONFIG_HOME"]
            .map { URL(fileURLWithPath: $0) } ?? Creds.home.appending(path: ".config")
        let url = base.appending(path: "tokenmaxxing/config.json")
        guard let data = try? Data(contentsOf: url),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let hosts = root["opencode_remote_hosts"] as? [String]
        else { return [] }
        return hosts.filter { !$0.isEmpty }
    }

    private static func query(host: String, nowMs: Int64) throws -> OpenCodeRemoteStats {
        let data = try runSSH(host: host, sql: aggregateSQL(nowMs: nowMs))
        guard let rows = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]],
              let row = rows.first
        else {
            throw QuotaError.message("\(host): unparseable sqlite output")
        }
        func d(_ key: String) -> Double { (row[key] as? NSNumber)?.doubleValue ?? 0 }
        func i(_ key: String) -> Int64 { (row[key] as? NSNumber)?.int64Value ?? 0 }
        var stats = OpenCodeRemoteStats()
        stats.windows["5h"] = OpenCodeRemoteWindow(spend: d("s5"), requests: i("r5"))
        stats.windows["7d"] = OpenCodeRemoteWindow(spend: d("s7"), requests: i("r7"))
        stats.windows["30d"] = OpenCodeRemoteWindow(spend: d("s30"), requests: i("r30"))
        stats.allTimeSpend = d("sall")
        stats.sessions = i("sess")
        stats.tokensIn = i("tin")
        stats.tokensOut = i("tout")
        stats.cacheRead = i("cr")
        return stats
    }

    private static func aggregateSQL(nowMs: Int64) -> String {
        let c5 = nowMs - 5 * 3_600_000
        let c7 = nowMs - 7 * 24 * 3_600_000
        let c30 = nowMs - 30 * 24 * 3_600_000
        return """
        SELECT
         COALESCE(SUM(CASE WHEN time_created>=\(c5) THEN json_extract(data,'$.cost') END),0.0) AS s5,
         COUNT(CASE WHEN time_created>=\(c5) AND json_extract(data,'$.cost') IS NOT NULL THEN 1 END) AS r5,
         COALESCE(SUM(CASE WHEN time_created>=\(c7) THEN json_extract(data,'$.cost') END),0.0) AS s7,
         COUNT(CASE WHEN time_created>=\(c7) AND json_extract(data,'$.cost') IS NOT NULL THEN 1 END) AS r7,
         COALESCE(SUM(CASE WHEN time_created>=\(c30) THEN json_extract(data,'$.cost') END),0.0) AS s30,
         COUNT(CASE WHEN time_created>=\(c30) AND json_extract(data,'$.cost') IS NOT NULL THEN 1 END) AS r30,
         COALESCE(SUM(json_extract(data,'$.cost')),0.0) AS sall,
         COUNT(DISTINCT session_id) AS sess,
         COALESCE(SUM(json_extract(data,'$.tokens.input')),0) AS tin,
         COALESCE(SUM(json_extract(data,'$.tokens.output')),0) AS tout,
         COALESCE(SUM(json_extract(data,'$.tokens.cache.read')),0) AS cr
        FROM message
        WHERE json_extract(data,'$.providerID')='\(OpenCodeProvider.provider)';
        """
    }

    /// Streams the SQL over stdin so no shell quoting of the statement is needed;
    /// the wait-then-read order guarantees the timeout fires even if ssh hangs.
    private static func runSSH(host: String, sql: String) throws -> Data {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/ssh")
        process.arguments = [
            "-o", "BatchMode=yes",
            "-o", "ConnectTimeout=4",
            host,
            "sqlite3 -readonly -json \(remoteDbPath)",
        ]
        let stdin = Pipe()
        let stdout = Pipe()
        process.standardInput = stdin
        process.standardOutput = stdout
        process.standardError = Pipe()
        let done = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in done.signal() }
        try process.run()
        stdin.fileHandleForWriting.write(Data(sql.utf8))
        stdin.fileHandleForWriting.closeFile()
        if done.wait(timeout: .now() + sshTimeout) == .timedOut {
            process.terminate()
            throw QuotaError.message("ssh \(host) timed out")
        }
        guard process.terminationStatus == 0 else {
            throw QuotaError.message("ssh \(host) exited \(process.terminationStatus)")
        }
        return stdout.fileHandleForReading.readDataToEndOfFile()
    }

    private struct CacheEntry {
        var stats: OpenCodeRemoteStats
        var fetchedAt: Date
        var age: TimeInterval { Date().timeIntervalSince(fetchedAt) }
    }

    private final class Cache: @unchecked Sendable {
        static let shared = Cache()
        private let lock = NSLock()
        private var entries: [String: CacheEntry] = [:]

        func entry(for host: String) -> CacheEntry? {
            lock.withLock { entries[host] }
        }

        func store(_ stats: OpenCodeRemoteStats, for host: String) {
            lock.withLock { entries[host] = CacheEntry(stats: stats, fetchedAt: Date()) }
        }
    }
}
