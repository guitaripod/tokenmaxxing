import Foundation
import SQLite3

private let SQLITE_TRANSIENT = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

struct OpenCodeAllTime {
    var spend = 0.0
    var sessions: Int64 = 0
    var tokensIn: Int64 = 0
    var tokensOut: Int64 = 0
    var cacheRead: Int64 = 0
}

/// opencode-go has no usage API, so this estimates rolling-window spend from the
/// local opencode.db against Go's dollar caps, and separately aggregates full
/// usage history across every provider opencode has run.
enum OpenCodeProvider {
    static let cap5h = 12.0
    static let cap7d = 30.0
    static let cap30d = 60.0
    static let provider = "opencode-go"

    static func fetch() -> Snapshot {
        guard Creds.opencodeGoConfigured() else {
            return degraded("opencode-go not configured — run `opencode auth login`")
        }
        guard FileManager.default.fileExists(atPath: Creds.opencodeDbPath.path) else {
            return degraded("no opencode.db on this machine yet")
        }
        do {
            let (gauges, details, remote) = try collect()
            return Snapshot(
                providerId: provider,
                providerName: "opencode go",
                subtitle: subtitle(for: remote),
                authority: .estimated,
                source: source(for: remote),
                gauges: gauges,
                details: details,
                note: note(for: remote),
                error: nil
            )
        } catch {
            return degraded("\(error)")
        }
    }

    /// Full local-history analytics across every provider opencode has run — the
    /// paid Go gateway plus any free/local models — not just the capped Go spend.
    static func usage() -> Usage {
        guard FileManager.default.fileExists(atPath: Creds.opencodeDbPath.path) else {
            return usageUnavailable("no opencode.db on this machine yet")
        }
        do {
            let db = try Database(path: Creds.opencodeDbPath.path)
            defer { db.close() }
            let daily = try db.dailySeries()

            let calendar = Calendar.current
            let today = calendar.startOfDay(for: Date())
            let cutoff7 = calendar.date(byAdding: .day, value: -6, to: today)!
            let cutoff30 = calendar.date(byAdding: .day, value: -29, to: today)!
            var windows = Windows()
            var cost = 0.0
            for d in daily {
                cost += d.cost
                if d.date == today { fold(&windows.today, d) }
                if d.date >= cutoff7 { fold(&windows.seven, d) }
                if d.date >= cutoff30 { fold(&windows.thirty, d) }
            }

            let tokens = try db.tokenBreakdown()
            let (messages, sessions) = try db.counts()
            var totals = Totals()
            totals.costUSD = cost
            totals.input = tokens.input
            totals.output = tokens.output
            totals.cacheWrite = tokens.cacheWrite
            totals.cacheRead = tokens.cacheRead
            totals.messages = messages
            totals.sessions = sessions
            totals.activeDays = Int64(daily.count)
            totals.firstDay = daily.first?.date
            totals.lastDay = daily.last?.date

            return Usage(
                scope: "opencode",
                authority: .estimated,
                source: "local opencode.db · all providers",
                totals: totals,
                windows: windows,
                daily: daily,
                byModel: try db.segments(groupExpr: "json_extract(data,'$.modelID')"),
                byProject: [],
                byProvider: try db.segments(groupExpr: "json_extract(data,'$.providerID')"),
                tokens: tokens,
                heatmap: try db.heatmap(),
                error: nil
            )
        } catch {
            return usageUnavailable("\(error)")
        }
    }

    private static func fold(_ w: inout WinStat, _ d: DayPoint) {
        w.cost += d.cost
        w.tokens += d.tokens
        w.messages += d.messages
    }

    private static func collect() throws -> ([Gauge], [Detail], OpenCodeRemoteReport) {
        let db = try Database(path: Creds.opencodeDbPath.path)
        defer { db.close() }
        let remote = OpenCodeRemote.report()
        let nowMs = Int64(Date().timeIntervalSince1970 * 1000)
        let windows: [(String, String, Int64, Double)] = [
            ("5h", "5-hour rolling", 5 * 3_600_000, cap5h),
            ("7d", "Weekly rolling", 7 * 24 * 3_600_000, cap7d),
            ("30d", "Monthly rolling", 30 * 24 * 3_600_000, cap30d),
        ]
        var gauges: [Gauge] = []
        for (key, label, span, cap) in windows {
            let (localSpend, localRequests) = try db.windowStats(cutoffMs: nowMs - span, provider: provider)
            let extra = remote.stats.windows[key] ?? OpenCodeRemoteWindow()
            let spend = localSpend + extra.spend
            let requests = localRequests + extra.requests
            gauges.append(Gauge(
                key: key, label: label,
                fraction: min(1, max(0, spend / cap)),
                used: spend, limit: cap, unit: .usd,
                detail: "\(requests) req", resetsAt: nil, trustedReset: false
            ))
        }
        let details = detailRows(local: try db.allTimeStats(provider: provider), remote: remote)
        return (gauges, details, remote)
    }

    private static func detailRows(local: OpenCodeAllTime, remote: OpenCodeRemoteReport) -> [Detail] {
        var rows: [Detail] = []
        if !remote.configured.isEmpty {
            rows.append(Detail(key: "Machines", value: coverage(remote)))
        }
        rows += [
            Detail(key: "Go spend (all-time)", value: String(format: "$%.2f", local.spend + remote.stats.allTimeSpend)),
            Detail(key: "Go sessions", value: "\(local.sessions + remote.stats.sessions)"),
            Detail(key: "Go tokens in", value: humanCount(local.tokensIn + remote.stats.tokensIn)),
            Detail(key: "Go tokens out", value: humanCount(local.tokensOut + remote.stats.tokensOut)),
            Detail(key: "Go cache read", value: humanCount(local.cacheRead + remote.stats.cacheRead)),
        ]
        return rows
    }

    private static func coverage(_ remote: OpenCodeRemoteReport) -> String {
        var parts = ["local"]
        parts += remote.reached
        parts += remote.stale.map { "\($0) (cached)" }
        var text = parts.joined(separator: " + ")
        if !remote.unreachable.isEmpty {
            text += " · \(remote.unreachable.joined(separator: ", ")) unreachable"
        }
        return text
    }

    private static func subtitle(for remote: OpenCodeRemoteReport) -> String {
        let machines = 1 + remote.included.count
        return machines > 1 ? "$10/mo · estimated · \(machines) machines" : "$10/mo · estimated locally"
    }

    private static func source(for remote: OpenCodeRemoteReport) -> String {
        guard !remote.included.isEmpty else { return "local opencode.db · estimate" }
        return "opencode.db: \(coverage(remote)) · estimate"
    }

    private static func note(for remote: OpenCodeRemoteReport) -> String {
        guard !remote.configured.isEmpty else {
            return "No usage API exists. Estimated from this machine's opencode.db against Go's rolling dollar caps — may miss usage on other machines and server-side accounting. List other machines in ~/.config/tokenmaxxing/config.json under opencode_remote_hosts to include them over SSH."
        }
        var text = "No usage API exists. Summed from opencode.db on this machine plus \(remote.configured.joined(separator: ", ")) over SSH against Go's rolling dollar caps — may still miss unlisted machines and server-side accounting."
        for host in remote.stale {
            text += " \(host): using figures cached under 15 minutes ago."
        }
        for host in remote.unreachable {
            text += " \(host) unreachable — its spend is not included."
        }
        return text
    }

    private static func degraded(_ message: String) -> Snapshot {
        Snapshot(
            providerId: provider,
            providerName: "opencode go",
            subtitle: "$10/mo subscription",
            authority: .unavailable,
            source: "local opencode.db · unavailable",
            gauges: [],
            details: [],
            note: nil,
            error: message
        )
    }

    private static func usageUnavailable(_ message: String) -> Usage {
        Usage(scope: "opencode", authority: .unavailable, source: "local opencode.db · unavailable", error: message)
    }
}

/// Minimal read-only SQLite wrapper. Opened read/write so it can participate in
/// WAL, but `query_only` forbids any mutation of the user's data.
private final class Database {
    let handle: OpaquePointer

    private let dayFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        f.timeZone = .current
        f.locale = Locale(identifier: "en_US_POSIX")
        return f
    }()

    init(path: String) throws {
        var db: OpaquePointer?
        guard sqlite3_open_v2(path, &db, SQLITE_OPEN_READWRITE, nil) == SQLITE_OK, let db else {
            throw QuotaError.message("open opencode.db failed")
        }
        handle = db
        sqlite3_busy_timeout(db, 2000)
        sqlite3_exec(db, "PRAGMA query_only=ON;", nil, nil, nil)
    }

    func close() {
        sqlite3_close(handle)
    }

    func windowStats(cutoffMs: Int64, provider: String) throws -> (Double, Int64) {
        let sql = """
        SELECT COALESCE(SUM(json_extract(data,'$.cost')),0.0), COUNT(*)
        FROM message
        WHERE json_extract(data,'$.providerID')=?1
          AND json_extract(data,'$.cost') IS NOT NULL
          AND time_created >= ?2
        """
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(handle, sql, -1, &stmt, nil) == SQLITE_OK else {
            throw QuotaError.message("usage query prepare failed")
        }
        defer { sqlite3_finalize(stmt) }
        sqlite3_bind_text(stmt, 1, provider, -1, SQLITE_TRANSIENT)
        sqlite3_bind_int64(stmt, 2, cutoffMs)
        guard sqlite3_step(stmt) == SQLITE_ROW else { return (0, 0) }
        return (sqlite3_column_double(stmt, 0), sqlite3_column_int64(stmt, 1))
    }

    func allTimeStats(provider: String) throws -> OpenCodeAllTime {
        let sql = """
        SELECT COALESCE(SUM(json_extract(data,'$.cost')),0.0),
               COUNT(DISTINCT session_id),
               COALESCE(SUM(json_extract(data,'$.tokens.input')),0),
               COALESCE(SUM(json_extract(data,'$.tokens.output')),0),
               COALESCE(SUM(json_extract(data,'$.tokens.cache.read')),0)
        FROM message WHERE json_extract(data,'$.providerID')=?1
        """
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(handle, sql, -1, &stmt, nil) == SQLITE_OK else {
            throw QuotaError.message("stats query prepare failed")
        }
        defer { sqlite3_finalize(stmt) }
        sqlite3_bind_text(stmt, 1, provider, -1, SQLITE_TRANSIENT)
        guard sqlite3_step(stmt) == SQLITE_ROW else { return OpenCodeAllTime() }
        return OpenCodeAllTime(
            spend: sqlite3_column_double(stmt, 0),
            sessions: sqlite3_column_int64(stmt, 1),
            tokensIn: sqlite3_column_int64(stmt, 2),
            tokensOut: sqlite3_column_int64(stmt, 3),
            cacheRead: sqlite3_column_int64(stmt, 4)
        )
    }

    func dailySeries() throws -> [DayPoint] {
        let sql = """
        SELECT date(time_created/1000,'unixepoch','localtime') d,
               COALESCE(SUM(json_extract(data,'$.cost')),0.0),
               COALESCE(SUM(json_extract(data,'$.tokens.total')),0),
               COUNT(*)
        FROM message WHERE json_extract(data,'$.role')='assistant'
        GROUP BY d ORDER BY d
        """
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(handle, sql, -1, &stmt, nil) == SQLITE_OK else {
            throw QuotaError.message("daily query prepare failed")
        }
        defer { sqlite3_finalize(stmt) }
        var out: [DayPoint] = []
        while sqlite3_step(stmt) == SQLITE_ROW {
            guard let cstr = sqlite3_column_text(stmt, 0) else { continue }
            guard let date = dayFormatter.date(from: String(cString: cstr)) else { continue }
            out.append(DayPoint(
                date: Calendar.current.startOfDay(for: date),
                cost: sqlite3_column_double(stmt, 1),
                tokens: max(0, sqlite3_column_int64(stmt, 2)),
                messages: max(0, sqlite3_column_int64(stmt, 3))
            ))
        }
        return out
    }

    func tokenBreakdown() throws -> TokenBreakdown {
        let sql = """
        SELECT COALESCE(SUM(json_extract(data,'$.tokens.input')),0),
               COALESCE(SUM(json_extract(data,'$.tokens.output')),0),
               COALESCE(SUM(json_extract(data,'$.tokens.cache.write')),0),
               COALESCE(SUM(json_extract(data,'$.tokens.cache.read')),0),
               COALESCE(SUM(json_extract(data,'$.tokens.reasoning')),0)
        FROM message WHERE json_extract(data,'$.role')='assistant'
        """
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(handle, sql, -1, &stmt, nil) == SQLITE_OK else {
            throw QuotaError.message("tokens query prepare failed")
        }
        defer { sqlite3_finalize(stmt) }
        guard sqlite3_step(stmt) == SQLITE_ROW else { return TokenBreakdown() }
        return TokenBreakdown(
            input: max(0, sqlite3_column_int64(stmt, 0)),
            output: max(0, sqlite3_column_int64(stmt, 1)),
            cacheWrite: max(0, sqlite3_column_int64(stmt, 2)),
            cacheRead: max(0, sqlite3_column_int64(stmt, 3)),
            reasoning: max(0, sqlite3_column_int64(stmt, 4))
        )
    }

    func counts() throws -> (Int64, Int64) {
        let messages = scalar("SELECT COUNT(*) FROM message WHERE json_extract(data,'$.role')='assistant'")
        let sessions = scalar("SELECT COUNT(*) FROM session")
        return (messages, sessions)
    }

    private func scalar(_ sql: String) -> Int64 {
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(handle, sql, -1, &stmt, nil) == SQLITE_OK else { return 0 }
        defer { sqlite3_finalize(stmt) }
        guard sqlite3_step(stmt) == SQLITE_ROW else { return 0 }
        return max(0, sqlite3_column_int64(stmt, 0))
    }

    /// Cost / token / message totals grouped by a JSON expression, sorted by tokens desc.
    func segments(groupExpr: String) throws -> [Segment] {
        let sql = """
        SELECT COALESCE(\(groupExpr),'?') g,
               COALESCE(SUM(json_extract(data,'$.cost')),0.0),
               COALESCE(SUM(json_extract(data,'$.tokens.total')),0),
               COUNT(*)
        FROM message WHERE json_extract(data,'$.role')='assistant'
        GROUP BY g ORDER BY 3 DESC
        """
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(handle, sql, -1, &stmt, nil) == SQLITE_OK else {
            throw QuotaError.message("segment query prepare failed")
        }
        defer { sqlite3_finalize(stmt) }
        var out: [Segment] = []
        while sqlite3_step(stmt) == SQLITE_ROW {
            let label = sqlite3_column_text(stmt, 0).map { String(cString: $0) } ?? "?"
            out.append(Segment(
                label: label,
                cost: sqlite3_column_double(stmt, 1),
                tokens: max(0, sqlite3_column_int64(stmt, 2)),
                messages: max(0, sqlite3_column_int64(stmt, 3))
            ))
        }
        return out
    }

    func heatmap() throws -> Heatmap {
        let sql = """
        SELECT CAST(strftime('%w', time_created/1000,'unixepoch','localtime') AS INTEGER),
               CAST(strftime('%H', time_created/1000,'unixepoch','localtime') AS INTEGER),
               COUNT(*)
        FROM message WHERE json_extract(data,'$.role')='assistant'
        GROUP BY 1, 2
        """
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(handle, sql, -1, &stmt, nil) == SQLITE_OK else {
            throw QuotaError.message("heatmap query prepare failed")
        }
        defer { sqlite3_finalize(stmt) }
        var heat = Heatmap()
        while sqlite3_step(stmt) == SQLITE_ROW {
            let sunDow = Int(sqlite3_column_int64(stmt, 0))
            let hour = Int(sqlite3_column_int64(stmt, 1))
            let count = Int(max(0, sqlite3_column_int64(stmt, 2)))
            let weekday = (sunDow + 6) % 7 // strftime 0=Sun → Monday-based
            guard (0..<7).contains(weekday), (0..<24).contains(hour) else { continue }
            heat.counts[weekday][hour] += count
            heat.max = max(heat.max, heat.counts[weekday][hour])
        }
        return heat
    }
}

private func humanCount(_ n: Int64) -> String {
    let value = Double(n)
    if value >= 1e9 { return String(format: "%.1fB", value / 1e9) }
    if value >= 1e6 { return String(format: "%.1fM", value / 1e6) }
    if value >= 1e3 { return String(format: "%.1fK", value / 1e3) }
    return "\(n)"
}
