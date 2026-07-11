import SwiftUI
import ServiceManagement

/// Owns the quota snapshots and the background refresh loop.
@MainActor
@Observable
final class AppModel {
    var snapshots: [Snapshot] = []
    var updatedText: String = "Connecting…"
    var launchAtLogin: Bool = false

    private var loop: Task<Void, Never>?

    init() {
        launchAtLogin = SMAppService.mainApp.status == .enabled
    }

    func start() {
        guard loop == nil else { return }
        loop = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refreshOnce()
                try? await Task.sleep(for: .seconds(90))
            }
        }
    }

    func refresh() {
        Task { await refreshOnce() }
    }

    private func refreshOnce() async {
        async let anthropic = AnthropicProvider.fetch()
        let opencode = await Task.detached { OpenCodeProvider.fetch() }.value
        snapshots = [await anthropic, opencode]
        updatedText = "updated \(Self.timeString()) · tokenmaxxing 0.1.0"
    }

    func exportShareCard() {
        ShareCard.export(snapshots: snapshots)
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
