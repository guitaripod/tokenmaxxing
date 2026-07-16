import SwiftUI
import AppKit

@main
struct Entry {
    static func main() {
        let arguments = CommandLine.arguments
        if let index = arguments.firstIndex(of: "--export-limits") {
            let path = (arguments.count > index + 1 && !arguments[index + 1].hasPrefix("-"))
                ? arguments[index + 1]
                : nil
            Headless.export(to: path, limitsOnly: true)
            return
        }
        if let index = arguments.firstIndex(of: "--export") {
            let path = (arguments.count > index + 1 && !arguments[index + 1].hasPrefix("-"))
                ? arguments[index + 1]
                : nil
            Headless.export(to: path, limitsOnly: false)
            return
        }
        if let index = arguments.firstIndex(of: "--icon") {
            let path = (arguments.count > index + 1) ? arguments[index + 1] : "/tmp/tokenmaxxing-icon.png"
            Headless.icon(to: path)
            return
        }
        TokenmaxxingApp.main()
    }
}

/// One-shot render used by `Tokenmaxxing --export` / `--export-limits` and for headless verification.
enum Headless {
    static func export(to path: String?, limitsOnly: Bool) {
        let box = DashboardBox()
        let semaphore = DispatchSemaphore(value: 0)
        Task.detached {
            let anthropic = await AnthropicProvider.fetch()
            let claudeUsage = await ClaudeHistory().scan()
            let grokQuota = await GrokProvider.fetch()
            let grokUsage = await GrokHistory().scan()
            let opencodeQuota = OpenCodeProvider.fetch()
            let opencodeUsage = OpenCodeProvider.usage()
            box.dashboard = Dashboard(
                claudeQuota: anthropic.snapshot,
                claudeUsage: claudeUsage,
                grokQuota: grokQuota,
                grokUsage: grokUsage,
                opencodeQuota: opencodeQuota,
                opencodeUsage: opencodeUsage,
                generatedAt: Date()
            )
            semaphore.signal()
        }
        semaphore.wait()

        MainActor.assumeIsolated {
            _ = NSApplication.shared
            guard let dashboard = box.dashboard else {
                FileHandle.standardError.write(Data("tokenmaxxing: build failed\n".utf8))
                return
            }
            let url: URL?
            if limitsOnly {
                url = DashboardExport.exportLimits(
                    dashboard: dashboard,
                    to: path.map { URL(fileURLWithPath: $0) }
                )
            } else {
                url = DashboardExport.export(
                    dashboard: dashboard,
                    sections: buildSections(dashboard),
                    to: path.map { URL(fileURLWithPath: $0) }
                )
            }
            if let url {
                print(url.path)
            } else {
                FileHandle.standardError.write(Data("tokenmaxxing: render failed\n".utf8))
            }
        }
    }

    static func icon(to path: String) {
        MainActor.assumeIsolated {
            _ = NSApplication.shared
            let renderer = ImageRenderer(content: AppIconView())
            renderer.scale = 1.0
            guard let image = renderer.nsImage,
                  let tiff = image.tiffRepresentation,
                  let bitmap = NSBitmapImageRep(data: tiff),
                  let png = bitmap.representation(using: .png, properties: [:])
            else {
                FileHandle.standardError.write(Data("tokenmaxxing: icon render failed\n".utf8))
                return
            }
            do {
                try png.write(to: URL(fileURLWithPath: path))
                print(path)
            } catch {
                FileHandle.standardError.write(Data("tokenmaxxing: icon write failed: \(error)\n".utf8))
            }
        }
    }
}

final class DashboardBox: @unchecked Sendable {
    var dashboard: Dashboard?
}
