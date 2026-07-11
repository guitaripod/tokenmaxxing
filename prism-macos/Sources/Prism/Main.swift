import SwiftUI
import AppKit

@main
struct Entry {
    static func main() {
        let arguments = CommandLine.arguments
        if let index = arguments.firstIndex(of: "--export") {
            let path = (arguments.count > index + 1 && !arguments[index + 1].hasPrefix("-"))
                ? arguments[index + 1]
                : nil
            Headless.export(to: path)
            return
        }
        PrismApp.main()
    }
}

/// One-shot render used by `Prism --export <path>` and for headless verification.
enum Headless {
    static func export(to path: String?) {
        let box = SnapshotBox()
        let semaphore = DispatchSemaphore(value: 0)
        Task.detached {
            let anthropic = await AnthropicProvider.fetch()
            let opencode = OpenCodeProvider.fetch()
            box.snapshots = [anthropic, opencode]
            semaphore.signal()
        }
        semaphore.wait()

        MainActor.assumeIsolated {
            _ = NSApplication.shared
            let renderer = ImageRenderer(content: ShareCardView(snapshots: box.snapshots))
            renderer.scale = 3.0
            guard let image = renderer.nsImage,
                  let tiff = image.tiffRepresentation,
                  let bitmap = NSBitmapImageRep(data: tiff),
                  let png = bitmap.representation(using: .png, properties: [:])
            else {
                FileHandle.standardError.write(Data("prism: render failed\n".utf8))
                return
            }
            let url = URL(fileURLWithPath: path ?? defaultPath())
            do {
                try png.write(to: url)
                print(url.path)
            } catch {
                FileHandle.standardError.write(Data("prism: write failed: \(error)\n".utf8))
            }
        }
    }

    private static func defaultPath() -> String {
        let directory = FileManager.default.urls(for: .picturesDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return directory.appending(path: "prism-\(formatter.string(from: Date())).png").path
    }
}

final class SnapshotBox: @unchecked Sendable {
    var snapshots: [Snapshot] = []
}
