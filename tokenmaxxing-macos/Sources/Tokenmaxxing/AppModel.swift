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
    private let history = ClaudeHistory()
    private var lastGoodClaude: Snapshot?
    private var claudeHealthy = true

    init() {
        launchAtLogin = SMAppService.mainApp.status == .enabled
    }

    func start() {
        guard loop == nil else { return }
        loop = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refreshOnce()
                // Poll gently when healthy; retry sooner while recovering.
                let seconds = (self?.claudeHealthy ?? true) ? 90 : 30
                try? await Task.sleep(for: .seconds(seconds))
            }
        }
    }

    func refresh() {
        Task { await refreshOnce() }
    }

    private func refreshOnce() async {
        async let anthropic = fetchClaude()
        async let claudeUsage = history.scan()
        let opencodeQuota = await Task.detached { OpenCodeProvider.fetch() }.value
        let opencodeUsage = await Task.detached { OpenCodeProvider.usage() }.value
        dashboard = Dashboard(
            claudeQuota: await anthropic,
            claudeUsage: await claudeUsage,
            opencodeQuota: opencodeQuota,
            opencodeUsage: opencodeUsage,
            generatedAt: Date()
        )
        updatedText = "updated \(Self.timeString()) · tokenmaxxing 0.1.0"
    }

    /// Fetch the live quota with a quick retry, falling back to the last
    /// successful snapshot on a transient failure (429, network blip, token
    /// race) instead of blanking the card to OFFLINE. Only OFFLINE if no good
    /// snapshot has ever been obtained.
    private func fetchClaude() async -> Snapshot {
        var latest = await AnthropicProvider.fetch()
        if latest.authority != .live {
            // One quick retry to ride out a transient blip.
            try? await Task.sleep(for: .seconds(3))
            latest = await AnthropicProvider.fetch()
        }
        if latest.authority == .live {
            lastGoodClaude = latest
            claudeHealthy = true
            return latest
        }
        claudeHealthy = false
        if var good = lastGoodClaude {
            good.source = "api.anthropic.com · live (cached, retrying)"
            return good
        }
        return latest
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
