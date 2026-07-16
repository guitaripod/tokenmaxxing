import SwiftUI
import AppKit

/// A self-contained export layout: branded header, the (solid, non-glass)
/// dashboard sections, and a footer. Rendered to PNG via `ImageRenderer` — glass
/// is a live effect that does not capture, so `DashboardContent` is drawn solid.
struct ExportView: View {
    var dashboard: Dashboard
    var sections: [SectionSpec]

    var body: some View {
        VStack(spacing: 0) {
            header
            DashboardContent(dashboard: dashboard, sections: sections, glass: false)
            footer
        }
        .frame(width: 1500)
        .background(Palette.base)
    }

    private var header: some View {
        HStack(spacing: 16) {
            TokenmaxxingMark(size: 52)
            VStack(alignment: .leading, spacing: 2) {
                Text("tokenmaxxing")
                    .font(.system(size: 32, weight: .bold, design: .rounded))
                    .foregroundStyle(LinearGradient(colors: [Palette.aqua, Palette.violet, Palette.pink], startPoint: .leading, endPoint: .trailing))
                Text("LLM usage dashboard").font(.system(size: 14)).foregroundStyle(Palette.muted)
            }
            Spacer()
            Text(dateString()).font(.system(size: 14, design: .monospaced)).foregroundStyle(Palette.muted)
        }
        .padding(.horizontal, 20)
        .padding(.top, 20)
        .padding(.bottom, 8)
    }

    private var footer: some View {
        Text("tokenmaxxing \(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.2.0")  ·  github.com/guitaripod/tokenmaxxing")
            .font(.system(size: 10.5, design: .monospaced))
            .foregroundStyle(Palette.muted.opacity(0.55))
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 20)
            .padding(.vertical, 12)
    }

    private func dateString() -> String {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd  HH:mm"
        return f.string(from: dashboard.generatedAt)
    }
}

/// Renders the dashboard (or a chosen subset of sections) to a high-resolution
/// PNG, saves it to Pictures, copies it to the clipboard, and reveals it.
@MainActor
enum DashboardExport {
    @discardableResult
    static func export(dashboard: Dashboard, sections: [SectionSpec], to path: URL? = nil) -> URL? {
        guard !sections.isEmpty else { return nil }
        let renderer = ImageRenderer(content: ExportView(dashboard: dashboard, sections: sections))
        renderer.scale = 2.0
        guard let image = renderer.nsImage,
              let tiff = image.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiff),
              let png = bitmap.representation(using: .png, properties: [:])
        else { return nil }

        let url = path ?? defaultOutput()
        do {
            try png.write(to: url)
        } catch {
            NSLog("tokenmaxxing: export write failed: \(error)")
            return nil
        }

        // PNG bytes + NSImage — some paste targets only accept one of the two.
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.declareTypes([.png, .tiff], owner: nil)
        pb.setData(png, forType: .png)
        pb.writeObjects([image])
        NSWorkspace.shared.activateFileViewerSelecting([url])
        return url
    }

    static func defaultOutput() -> URL {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        let dir = FileManager.default.urls(for: .picturesDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser
        return dir.appending(path: "tokenmaxxing-\(formatter.string(from: Date())).png")
    }
}
