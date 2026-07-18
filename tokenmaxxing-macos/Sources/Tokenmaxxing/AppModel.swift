import SwiftUI
import ServiceManagement

/// Owns the quota snapshots and the background refresh loop.
@MainActor
@Observable
final class AppModel {
    var dashboard: Dashboard?
    var updatedText: String = "Connecting…"
    var launchAtLogin: Bool = false

    private var loop: Task<Void, Never>?
    private let claudeHistory = ClaudeHistory()
    private let grokHistory = GrokHistory()
    private var lastGoodClaude: Snapshot?
    private var lastGoodGrok: Snapshot?
    private var claudeNextFetch = Date.distantPast
    private var grokHealthy = true

    init() {
        launchAtLogin = SMAppService.mainApp.status == .enabled
    }

    func start() {
        guard loop == nil else { return }
        loop = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refreshOnce(forceClaude: false)
                // Base tick 30s; Claude's own cooldown decides whether we actually re-hit the API.
                try? await Task.sleep(for: .seconds(30))
            }
        }
    }

    func refresh() {
        Task { await refreshOnce(forceClaude: true) }
    }

    private func refreshOnce(forceClaude: Bool) async {
        async let claude = fetchClaude(force: forceClaude)
        async let grok = fetchGrok()
        async let claudeUsage = claudeHistory.scan()
        async let grokUsage = grokHistory.scan()
        let opencodeQuota = await Task.detached { OpenCodeProvider.fetch() }.value
        let opencodeUsage = await Task.detached { OpenCodeProvider.usage() }.value
        dashboard = Dashboard(
            claudeQuota: await claude,
            claudeUsage: await claudeUsage,
            grokQuota: await grok,
            grokUsage: await grokUsage,
            opencodeQuota: opencodeQuota,
            opencodeUsage: opencodeUsage,
            generatedAt: Date()
        )
        let ver = AppVersion.current
        updatedText = "updated \(Self.timeString()) · tokenmaxxing \(ver)"
    }

    private func fetchClaude(force: Bool) async -> Snapshot {
        if !force, Date() < claudeNextFetch, let good = lastGoodClaude {
            return good
        }
        let result = await AnthropicProvider.fetch()
        claudeNextFetch = Date().addingTimeInterval(result.cooldown)
        if result.fresh || !result.snapshot.gauges.isEmpty || lastGoodClaude == nil {
            lastGoodClaude = result.snapshot
            return result.snapshot
        }
        // Keep rings; annotate cooldown.
        if var good = lastGoodClaude {
            good.source = result.snapshot.source
            good.note = result.snapshot.note
            lastGoodClaude = good
            return good
        }
        return result.snapshot
    }

    private func fetchGrok() async -> Snapshot {
        let result = await fetchGrokLive(lastGood: lastGoodGrok)
        if result.fresh {
            lastGoodGrok = result.snapshot
            grokHealthy = true
        } else {
            grokHealthy = false
        }
        return result.snapshot
    }

    private struct LiveResult {
        var snapshot: Snapshot
        var fresh: Bool
    }

    private func fetchGrokLive(lastGood: Snapshot?) async -> LiveResult {
        var latest = await GrokProvider.fetch()
        if latest.authority != .live {
            try? await Task.sleep(for: .seconds(2))
            latest = await GrokProvider.fetch()
        }
        if latest.authority == .live {
            return LiveResult(snapshot: latest, fresh: true)
        }
        if var good = lastGood {
            good.source = "cli-chat-proxy.grok.com · live (cached, retrying)"
            return LiveResult(snapshot: good, fresh: false)
        }
        return LiveResult(snapshot: latest, fresh: false)
    }

    func exportShareCard() {
        guard let dashboard else { return }
        DashboardExport.export(dashboard: dashboard, sections: buildSections(dashboard))
    }

    func setLaunchAtLogin(_ enabled: Bool) {
        do {
            if enabled {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
        } catch {
            NSLog("tokenmaxxing: launch-at-login toggle failed: \(error)")
        }
        launchAtLogin = SMAppService.mainApp.status == .enabled
    }

    private static func timeString() -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"
        return formatter.string(from: Date())
    }
}
