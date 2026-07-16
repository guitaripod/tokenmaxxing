import Foundation

/// Grok usage activity from local CLI sessions under `~/.grok/sessions`.
/// The CLI does not persist per-turn token usage on disk, so this aggregates
/// turn/message activity — not dollars.
final class GrokHistory: @unchecked Sendable {
    private struct Record {
        var date: Date
        var weekday: Int
        var hour: Int
        var model: String
        var project: String
        var session: String
        var messages: Int64
    }

    private struct FileEntry {
        var size: UInt64
        var mtime: Date
        var records: [Record]
    }

    private var cache: [URL: FileEntry] = [:]

    func scan() async -> Usage {
        await Task.detached { [self] in
            self.scanSync()
        }.value
    }

    private func scanSync() -> Usage {
        let root = Creds.grokSessionsPath
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: root.path, isDirectory: &isDir), isDir.boolValue else {
            return unavailable("no ~/.grok/sessions — run `grok` to sign in")
        }

        let sessionDirs = collectSessionDirs(root)
        if sessionDirs.isEmpty {
            return unavailable("no Grok sessions on this machine yet")
        }

        let present = Set(sessionDirs)
        cache = cache.filter { present.contains($0.key) }

        for dir in sessionDirs {
            let marker = dir.appending(path: "summary.json")
            let path = FileManager.default.fileExists(atPath: marker.path) ? marker : dir
            guard let attrs = try? FileManager.default.attributesOfItem(atPath: path.path),
                  let size = attrs[.size] as? UInt64,
                  let mtime = attrs[.modificationDate] as? Date
            else { continue }
            if let existing = cache[dir], existing.size == size, existing.mtime == mtime {
                continue
            }
            cache[dir] = FileEntry(size: size, mtime: mtime, records: parseSession(dir))
        }

        return aggregate()
    }

    private func aggregate() -> Usage {
        let calendar = Calendar.current
        let today = calendar.startOfDay(for: Date())
        var sessions = Set<String>()
        var daily: [Date: DayPoint] = [:]
        var byModel: [String: Segment] = [:]
        var byProject: [String: Segment] = [:]
        var heat = Heatmap()
        var totals = Totals()
        var windows = Windows()
        var first: Date?
        var last: Date?

        for entry in cache.values {
            for r in entry.records {
                sessions.insert(r.session)
                totals.messages += r.messages
                let day = calendar.startOfDay(for: r.date)
                first = first.map { min($0, day) } ?? day
                last = last.map { max($0, day) } ?? day

                var dayPoint = daily[day] ?? DayPoint(date: day, cost: 0, tokens: 0, messages: 0)
                dayPoint.messages += r.messages
                daily[day] = dayPoint

                var model = byModel[r.model] ?? Segment(label: r.model, cost: 0, tokens: 0, messages: 0)
                model.messages += r.messages
                byModel[r.model] = model

                var project = byProject[r.project] ?? Segment(label: r.project, cost: 0, tokens: 0, messages: 0)
                project.messages += r.messages
                byProject[r.project] = project

                if r.weekday >= 0, r.weekday < 7, r.hour >= 0, r.hour < 24 {
                    heat.counts[r.weekday][r.hour] += Int(r.messages)
                    heat.max = max(heat.max, heat.counts[r.weekday][r.hour])
                }

                let age = calendar.dateComponents([.day], from: day, to: today).day ?? 999
                if age == 0 { windows.today.messages += r.messages }
                if age < 7 { windows.seven.messages += r.messages }
                if age < 30 { windows.thirty.messages += r.messages }
            }
        }

        totals.sessions = Int64(sessions.count)
        totals.activeDays = Int64(daily.count)
        totals.firstDay = first
        totals.lastDay = last

        return Usage(
            scope: "grok",
            authority: .estimated,
            source: "local ~/.grok/sessions · activity",
            totals: totals,
            windows: windows,
            daily: daily.values.sorted { $0.date < $1.date },
            byModel: byModel.values.sorted { $0.messages > $1.messages },
            byProject: byProject.values.sorted { $0.messages > $1.messages },
            byProvider: [],
            tokens: TokenBreakdown(),
            heatmap: heat,
            error: nil
        )
    }

    private func collectSessionDirs(_ root: URL) -> [URL] {
        guard let entries = try? FileManager.default.contentsOfDirectory(
            at: root, includingPropertiesForKeys: [.isDirectoryKey], options: [.skipsHiddenFiles]
        ) else { return [] }
        var out: [URL] = []
        for entry in entries {
            var isDir: ObjCBool = false
            guard FileManager.default.fileExists(atPath: entry.path, isDirectory: &isDir), isDir.boolValue else {
                continue
            }
            if FileManager.default.fileExists(atPath: entry.appending(path: "summary.json").path)
                || FileManager.default.fileExists(atPath: entry.appending(path: "events.jsonl").path)
            {
                out.append(entry)
                continue
            }
            if let inner = try? FileManager.default.contentsOfDirectory(
                at: entry, includingPropertiesForKeys: [.isDirectoryKey], options: [.skipsHiddenFiles]
            ) {
                for child in inner {
                    var childDir: ObjCBool = false
                    guard FileManager.default.fileExists(atPath: child.path, isDirectory: &childDir),
                          childDir.boolValue
                    else { continue }
                    if FileManager.default.fileExists(atPath: child.appending(path: "summary.json").path)
                        || FileManager.default.fileExists(atPath: child.appending(path: "events.jsonl").path)
                        || FileManager.default.fileExists(atPath: child.appending(path: "chat_history.jsonl").path)
                    {
                        out.append(child)
                    }
                }
            }
        }
        return out
    }

    private func parseSession(_ dir: URL) -> [Record] {
        let sessionId = dir.lastPathComponent
        let project = projectFrom(dir)
        let model = summaryModel(dir) ?? "grok"
        var records = parseTurnEvents(dir, session: sessionId, project: project)
        if records.isEmpty, let fallback = parseSummaryFallback(dir, session: sessionId, project: project, model: model) {
            records.append(fallback)
        }
        return records
    }

    private func projectFrom(_ dir: URL) -> String {
        let workspace = dir.deletingLastPathComponent().lastPathComponent
        let decoded = workspace.removingPercentEncoding ?? workspace
        return URL(fileURLWithPath: decoded).lastPathComponent.isEmpty
            ? "home"
            : URL(fileURLWithPath: decoded).lastPathComponent
    }

    private func summaryModel(_ dir: URL) -> String? {
        guard let data = try? Data(contentsOf: dir.appending(path: "summary.json")),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return nil }
        return json["current_model_id"] as? String
    }

    private func parseTurnEvents(_ dir: URL, session: String, project: String) -> [Record] {
        let path = dir.appending(path: "events.jsonl")
        guard let text = try? String(contentsOf: path, encoding: .utf8) else { return [] }
        var out: [Record] = []
        let calendar = Calendar.current
        for line in text.split(separator: "\n", omittingEmptySubsequences: true) {
            guard line.contains("turn_started"),
                  let data = line.data(using: .utf8),
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  json["type"] as? String == "turn_started",
                  let ts = json["ts"] as? String,
                  let date = parseEventTs(ts)
            else { continue }
            let model = shortModel(json["model_id"] as? String ?? "grok")
            let weekday = (calendar.component(.weekday, from: date) + 5) % 7 // Mon=0
            let hour = calendar.component(.hour, from: date)
            out.append(Record(
                date: date, weekday: weekday, hour: hour,
                model: model, project: project, session: session, messages: 1
            ))
        }
        return out
    }

    private func parseSummaryFallback(_ dir: URL, session: String, project: String, model: String) -> Record? {
        guard let data = try? Data(contentsOf: dir.appending(path: "summary.json")),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return nil }
        let messages = max(1, (json["num_chat_messages"] as? NSNumber)?.int64Value
            ?? (json["num_messages"] as? NSNumber)?.int64Value
            ?? 0)
        let raw = (json["last_active_at"] as? String)
            ?? (json["updated_at"] as? String)
            ?? (json["created_at"] as? String)
        guard let raw, let date = parseEventTs(raw) else { return nil }
        let calendar = Calendar.current
        return Record(
            date: date,
            weekday: (calendar.component(.weekday, from: date) + 5) % 7,
            hour: calendar.component(.hour, from: date),
            model: shortModel(model),
            project: project,
            session: session,
            messages: messages
        )
    }

    private func parseEventTs(_ raw: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: raw) { return date }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: raw)
    }

    private func shortModel(_ model: String) -> String {
        let m = model.lowercased()
        if m.contains("grok-4.5") || m.contains("grok-build") { return "Grok 4.5" }
        if m.contains("grok-4.3") { return "Grok 4.3" }
        if m.contains("grok-4.20") || m.contains("grok-4-20") { return "Grok 4.20" }
        if m.contains("grok-3") { return "Grok 3" }
        return model
    }

    private func unavailable(_ message: String) -> Usage {
        var usage = Usage()
        usage.scope = "grok"
        usage.authority = .unavailable
        usage.source = "local ~/.grok/sessions · unavailable"
        usage.error = message
        return usage
    }
}
