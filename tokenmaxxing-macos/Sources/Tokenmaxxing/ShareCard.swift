import SwiftUI
import AppKit

/// Renders the current quota state into a high-resolution PNG, saves it to
/// Pictures, copies it to the clipboard, and reveals it in Finder.
enum ShareCard {
    @MainActor
    static func export(snapshots: [Snapshot]) {
        guard !snapshots.isEmpty else { return }
        let renderer = ImageRenderer(content: ShareCardView(snapshots: snapshots))
        renderer.scale = 3.0
        guard let image = renderer.nsImage,
              let tiff = image.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiff),
              let png = bitmap.representation(using: .png, properties: [:])
        else { return }

        let url = picturesDirectory().appending(path: "tokenmaxxing-\(timestamp()).png")
        try? png.write(to: url)

        NSPasteboard.general.clearContents()
        NSPasteboard.general.writeObjects([image])
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }

    private static func picturesDirectory() -> URL {
        FileManager.default.urls(for: .picturesDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser
    }

    private static func timestamp() -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return formatter.string(from: Date())
    }
}

/// Self-contained share layout — solid cards (not glass) so it captures cleanly
/// through ImageRenderer.
struct ShareCardView: View {
    var snapshots: [Snapshot]

    var body: some View {
        VStack(spacing: 20) {
            header
            ForEach(snapshots) { snapshot in
                ProviderCard(snapshot: snapshot, scale: 1.2, glass: false)
            }
            footer
        }
        .padding(30)
        .frame(width: 540)
        .background(background)
    }

    private var header: some View {
        HStack(alignment: .center, spacing: 14) {
            TokenmaxxingMark(size: 52)
            VStack(alignment: .leading, spacing: 2) {
                Text("tokenmaxxing")
                    .font(.system(size: 34, weight: .bold, design: .rounded))
                    .foregroundStyle(
                        LinearGradient(colors: [Palette.aqua, Palette.violet, Palette.pink],
                                       startPoint: .leading, endPoint: .trailing)
                    )
                Text("LLM token quotas")
                    .font(.system(size: 15))
                    .foregroundStyle(Palette.muted)
            }
            Spacer()
            Text(dateString())
                .font(.system(size: 14, design: .monospaced))
                .foregroundStyle(Palette.muted)
        }
    }

    private var footer: some View {
        HStack {
            Text("github.com/guitaripod/tokenmaxxing")
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(Palette.muted)
            Spacer()
            Text("tokenmaxxing 0.1.0")
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(Palette.muted)
        }
    }

    private var background: some View {
        ZStack {
            Palette.base
            LinearGradient(
                colors: [Palette.indigo.opacity(0.22), .clear, Palette.pink.opacity(0.16)],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        }
    }

    private func dateString() -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd  HH:mm"
        return formatter.string(from: Date())
    }
}
