import Foundation
import SQLite3

private let SQLITE_TRANSIENT = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

/// opencode-go has no usage API, so this estimates rolling-window spend from the
/// local opencode.db against Go's dollar caps — clearly flagged as an estimate.
enum OpenCodeProvider {
    static let cap5h = 12.0
    static let cap7d = 30.0
    static let cap30d = 60.0
    static let provider = "opencode-go"
    static let note = "No usage API exists. Estimated from this machine's opencode.db against Go's rolling dollar caps — may miss usage on other machines and server-side accounting."

    static func fetch() -> Snapshot {
        guard Creds.opencodeGoConfigured() else {
            return degraded("opencode-go not configured — run `opencode auth login`")
        }
        guard FileManager.default.fileExists(atPath: Creds.opencodeDbPath.path) else {
            return degraded("no opencode.db on this machine yet")
        }
        do {
            let (gauges, details) = try collect()
            return Snapshot(
                providerId: provider,
                providerName: "opencode go",
                subtitle: "$10/mo · estimated locally",
                authority: .estimated,
                source: "local opencode.db · estimate",
                gauges: gauges,
                details: details,
                note: note,
                error: nil
            )
        } catch {
            return degraded("\(error)")
        }
    }

    private static func collect() throws -> ([Gauge], [Detail]) {
        let db = try Database(path: Creds.opencodeDbPath.path)
        defer { db.close() }
        let nowMs = Int64(Date().timeIntervalSince1970 * 1000)
        let windows: [(String, String, Int64, Double)] = [
            ("5h", "5-hour rolling", 5 * 3_600_000, cap5h),
            ("7d", "Weekly rolling", 7 * 24 * 3_600_000, cap7d),
            ("30d", "Monthly rolling", 30 * 24 * 3_600_000, cap30d),
        ]
        var gauges: [Gauge] = []
        for (key, label, span, cap) in windows {
            let (spend, requests) = try db.windowStats(cutoffMs: nowMs - span, provider: provider)
            gauges.append(Gauge(
                key: key, label: label,
                fraction: min(1, max(0, spend / cap)),
                used: spend, limit: cap, unit: .usd,
                detail: "\(requests) req", resetsAt: nil, trustedReset: false
            ))
        }
        return (gauges, try db.allTimeDetails(provider: provider))
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
}

/// Minimal read-only SQLite wrapper. Opened read/write so it can participate in
/// WAL, but `query_only` forbids any mutation of the user's data.
private final class Database {
    let handle: OpaquePointer

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

    func allTimeDetails(provider: String) throws -> [Detail] {
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
        guard sqlite3_step(stmt) == SQLITE_ROW else { return [] }
        return [
            Detail(key: "All-time spend", value: String(format: "$%.2f", sqlite3_column_double(stmt, 0))),
            Detail(key: "Sessions", value: "\(sqlite3_column_int64(stmt, 1))"),
            Detail(key: "Tokens in", value: humanCount(sqlite3_column_int64(stmt, 2))),
            Detail(key: "Tokens out", value: humanCount(sqlite3_column_int64(stmt, 3))),
            Detail(key: "Cache read", value: humanCount(sqlite3_column_int64(stmt, 4))),
        ]
    }
}

private func humanCount(_ n: Int64) -> String {
    let value = Double(n)
    if value >= 1e9 { return String(format: "%.1fB", value / 1e9) }
    if value >= 1e6 { return String(format: "%.1fM", value / 1e6) }
    if value >= 1e3 { return String(format: "%.1fK", value / 1e3) }
    return "\(n)"
}
