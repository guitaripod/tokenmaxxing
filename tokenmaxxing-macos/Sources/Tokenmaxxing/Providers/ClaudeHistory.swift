import Foundation

/// Claude usage history, aggregated from the JSONL transcripts Claude Code
/// writes under `~/.claude/projects` — the same corpus `ccusage` reads. The
/// subscription bills a flat fee, so the dollar figures here are API-equivalent
/// *value*, not spend, and are always labelled estimated. Token counts are exact.
actor ClaudeHistory {
    /// One priced assistant turn, reduced to what the dashboard aggregates.
    private struct Record {
        var day: Date        // local start-of-day, the daily bucket key
        var weekday: Int     // 0 = Monday
        var hour: Int
        var model: String
        var project: String
        var session: String
        var dedup: Int?      // hash of message.id:requestId, nil when either is missing
        var cost: Double
        var input: Int64
        var output: Int64
        var cacheWrite: Int64
        var cacheRead: Int64
        var webSearch: Int64
        var webFetch: Int64
    }

    private struct FileEntry {
        var size: Int
        var mtime: Date
        var records: [Record]
    }

    private var cache: [String: FileEntry] = [:]
    private let calendar = Calendar.current

    // Instance-owned (actor-isolated) so there's no shared non-Sendable global.
    private let isoFractional: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
    private let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()

    /// The first scan reads every transcript; later scans re-read only files whose
    /// size or modification date changed.
    func scan() -> Usage {
        let root = Creds.home.appendingPathComponent(".claude/projects")
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: root.path, isDirectory: &isDir), isDir.boolValue else {
            return Self.unavailable("no ~/.claude/projects — run `claude` to sign in")
        }

        let files = jsonlFiles(under: root)
        if files.isEmpty {
            return Self.unavailable("no Claude transcripts on this machine yet")
        }

        let present = Set(files.map(\.path))
        cache = cache.filter { present.contains($0.key) }

        for url in files {
            let attrs = try? FileManager.default.attributesOfItem(atPath: url.path)
            let size = (attrs?[.size] as? Int) ?? 0
            let mtime = (attrs?[.modificationDate] as? Date) ?? .distantPast
            if let entry = cache[url.path], entry.size == size, entry.mtime == mtime {
                continue
            }
            cache[url.path] = FileEntry(size: size, mtime: mtime, records: parse(url))
        }

        return aggregate()
    }

    private func aggregate() -> Usage {
        let startOfToday = calendar.startOfDay(for: Date())
        let cutoff7 = calendar.date(byAdding: .day, value: -6, to: startOfToday)!
        let cutoff30 = calendar.date(byAdding: .day, value: -29, to: startOfToday)!

        var seen = Set<Int>()
        var sessions = Set<String>()
        var days = Set<Date>()
        var daily: [Date: (cost: Double, tokens: Int64, messages: Int64)] = [:]
        var byModel: [String: Segment] = [:]
        var byProject: [String: Segment] = [:]
        var heat = Heatmap()
        var totals = Totals()
        var tokens = TokenBreakdown()
        var windows = Windows()
        var first = Date.distantFuture
        var last = Date.distantPast

        func addSegment(_ map: inout [String: Segment], _ key: String, _ r: Record, _ msgTokens: Int64) {
            var seg = map[key] ?? Segment(label: key, cost: 0, tokens: 0, messages: 0)
            seg.cost += r.cost
            seg.tokens += msgTokens
            seg.messages += 1
            map[key] = seg
        }

        for entry in cache.values {
            for r in entry.records {
                if let id = r.dedup {
                    if seen.contains(id) { continue }
                    seen.insert(id)
                }
                let msgTokens = r.input + r.output + r.cacheWrite + r.cacheRead

                totals.costUSD += r.cost
                totals.input += r.input
                totals.output += r.output
                totals.cacheWrite += r.cacheWrite
                totals.cacheRead += r.cacheRead
                totals.messages += 1
                totals.webSearch += r.webSearch
                totals.webFetch += r.webFetch
                sessions.insert(r.session)
                days.insert(r.day)
                first = min(first, r.day)
                last = max(last, r.day)

                tokens.input += r.input
                tokens.output += r.output
                tokens.cacheWrite += r.cacheWrite
                tokens.cacheRead += r.cacheRead

                var bucket = daily[r.day] ?? (0, 0, 0)
                bucket.cost += r.cost
                bucket.tokens += msgTokens
                bucket.messages += 1
                daily[r.day] = bucket

                addSegment(&byModel, r.model, r, msgTokens)
                addSegment(&byProject, r.project, r, msgTokens)

                heat.counts[r.weekday][r.hour] += 1
                heat.max = Swift.max(heat.max, heat.counts[r.weekday][r.hour])

                if r.day == startOfToday { accumulate(&windows.today, r, msgTokens) }
                if r.day >= cutoff7 { accumulate(&windows.seven, r, msgTokens) }
                if r.day >= cutoff30 { accumulate(&windows.thirty, r, msgTokens) }
            }
        }

        if totals.messages == 0 {
            return Self.unavailable("Claude transcripts contain no priced usage yet")
        }

        totals.sessions = Int64(sessions.count)
        totals.activeDays = Int64(days.count)
        totals.firstDay = first == .distantFuture ? nil : first
        totals.lastDay = last == .distantPast ? nil : last

        let dailyPoints = daily
            .map { DayPoint(date: $0.key, cost: $0.value.cost, tokens: $0.value.tokens, messages: $0.value.messages) }
            .sorted { $0.date < $1.date }

        return Usage(
            scope: "Claude",
            authority: .estimated,
            source: "~/.claude/projects · local history",
            totals: totals,
            windows: windows,
            daily: dailyPoints,
            byModel: sortedSegments(byModel),
            byProject: sortedSegments(byProject),
            byProvider: [],
            tokens: tokens,
            heatmap: heat,
            error: nil
        )
    }

    private func accumulate(_ w: inout WinStat, _ r: Record, _ msgTokens: Int64) {
        w.cost += r.cost
        w.tokens += msgTokens
        w.messages += 1
    }

    private func sortedSegments(_ map: [String: Segment]) -> [Segment] {
        map.values.sorted { a, b in
            a.cost != b.cost ? a.cost > b.cost : a.tokens > b.tokens
        }
    }

    private func parse(_ url: URL) -> [Record] {
        guard let content = try? String(contentsOf: url, encoding: .utf8) else { return [] }
        var records: [Record] = []
        for line in content.split(separator: "\n", omittingEmptySubsequences: true) {
            if let record = parseLine(String(line)) { records.append(record) }
        }
        return records
    }

    private func parseLine(_ line: String) -> Record? {
        guard let data = line.data(using: .utf8),
              let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              json["type"] as? String == "assistant",
              let message = json["message"] as? [String: Any]
        else { return nil }

        let model = (message["model"] as? String) ?? ""
        if model.isEmpty || model == "<synthetic>" { return nil }
        guard let usage = message["usage"] as? [String: Any] else { return nil }

        func i64(_ dict: [String: Any]?, _ key: String) -> Int64 {
            ((dict?[key] as? NSNumber)?.int64Value) ?? 0
        }
        let input = i64(usage, "input_tokens")
        let output = i64(usage, "output_tokens")
        let cacheWrite = i64(usage, "cache_creation_input_tokens")
        let cacheRead = i64(usage, "cache_read_input_tokens")
        if input + output + cacheWrite + cacheRead == 0 { return nil }

        let creation = usage["cache_creation"] as? [String: Any]
        let e5m = i64(creation, "ephemeral_5m_input_tokens")
        let e1h = i64(creation, "ephemeral_1h_input_tokens")
        let (write5m, write1h): (Int64, Int64) = (e5m + e1h == cacheWrite && cacheWrite > 0) ? (e5m, e1h) : (cacheWrite, 0)

        let tools = usage["server_tool_use"] as? [String: Any]
        let cost = Pricing.rate(for: model).cost(
            TokenCounts(input: input, output: output, cacheWrite5m: write5m, cacheWrite1h: write1h, cacheRead: cacheRead)
        )

        guard let tsString = json["timestamp"] as? String,
              let date = isoFractional.date(from: tsString) ?? iso.date(from: tsString)
        else { return nil }

        return Record(
            day: calendar.startOfDay(for: date),
            weekday: (calendar.component(.weekday, from: date) + 5) % 7,
            hour: calendar.component(.hour, from: date),
            model: Pricing.shortName(model),
            project: projectName(json),
            session: (json["sessionId"] as? String) ?? "",
            dedup: dedupKey(message, json),
            cost: cost,
            input: input,
            output: output,
            cacheWrite: cacheWrite,
            cacheRead: cacheRead,
            webSearch: i64(tools, "web_search_requests"),
            webFetch: i64(tools, "web_fetch_requests")
        )
    }

    private func dedupKey(_ message: [String: Any], _ json: [String: Any]) -> Int? {
        guard let id = message["id"] as? String, let request = json["requestId"] as? String else { return nil }
        var hasher = Hasher()
        hasher.combine(id)
        hasher.combine(request)
        return hasher.finalize()
    }

    private func projectName(_ json: [String: Any]) -> String {
        if let cwd = json["cwd"] as? String {
            let name = cwd.split(separator: "/").last.map(String.init) ?? cwd
            if !name.isEmpty { return name }
        }
        return "unknown"
    }

    private func jsonlFiles(under root: URL) -> [URL] {
        guard let enumerator = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil) else {
            return []
        }
        return enumerator.compactMap { $0 as? URL }.filter { $0.pathExtension == "jsonl" }
    }

    private static func unavailable(_ message: String) -> Usage {
        Usage(scope: "Claude", authority: .unavailable, source: "~/.claude/projects · unavailable", error: message)
    }
}
