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
        Text("tokenmaxxing \(AppVersion.current)  ·  github.com/guitaripod/tokenmaxxing")
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

/// Compact menu-bar / limits export — Claude → Grok → opencode cards only.
struct LimitsExportView: View {
    var dashboard: Dashboard

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 10) {
                TokenmaxxingMark(size: 28)
                VStack(alignment: .leading, spacing: 2) {
                    Text("tokenmaxxing")
                        .font(.system(size: 18, weight: .bold, design: .rounded))
                        .foregroundStyle(Palette.text)
                    Text("current limits")
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(Palette.muted)
                }
                Spacer(minLength: 0)
                Text(dateString())
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(Palette.muted)
            }
            .padding(.horizontal, 16)
            .padding(.top, 16)
            .padding(.bottom, 12)

            Rectangle().fill(Palette.track.opacity(0.7)).frame(height: 1)

            VStack(alignment: .leading, spacing: 10) {
                limitsCard(
                    name: "Claude",
                    subtitle: dashboard.claudeQuota.subtitle,
                    authority: dashboard.claudeQuota.authority,
                    gauges: dashboard.claudeQuota.gauges,
                    accent: Palette.aqua,
                    error: dashboard.claudeQuota.error
                )
                limitsCard(
                    name: "Grok",
                    subtitle: dashboard.grokQuota.subtitle,
                    authority: dashboard.grokQuota.authority,
                    gauges: dashboard.grokQuota.gauges,
                    accent: Palette.violet,
                    error: dashboard.grokQuota.error
                )
                limitsCard(
                    name: "opencode",
                    subtitle: dashboard.opencodeQuota.subtitle,
                    authority: dashboard.opencodeQuota.authority,
                    gauges: dashboard.opencodeQuota.gauges,
                    accent: Palette.lime,
                    error: dashboard.opencodeQuota.error
                )
            }
            .padding(14)

            Rectangle().fill(Palette.track.opacity(0.7)).frame(height: 1)

            Text("tokenmaxxing \(AppVersion.current)  ·  github.com/guitaripod/tokenmaxxing")
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(Palette.muted.opacity(0.55))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 12)
        }
        .frame(width: 560)
        .background(Palette.canvas)
        .environment(\.colorScheme, .light)
    }

    private func limitsCard(
        name: String,
        subtitle: String,
        authority: Authority,
        gauges: [Gauge],
        accent: Color,
        error: String?
    ) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                RoundedRectangle(cornerRadius: 1.5, style: .continuous)
                    .fill(accent)
                    .frame(width: 3, height: 16)
                Text(name)
                    .font(.system(size: 14, weight: .semibold, design: .rounded))
                    .foregroundStyle(Palette.text)
                BadgePill(authority: authority, scale: 1.0)
                if !subtitle.isEmpty {
                    Text(subtitle)
                        .font(.system(size: 11.5))
                        .foregroundStyle(Palette.muted)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
            }

            if let error {
                Text(error)
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.rose)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.leading, 11)
            } else if gauges.isEmpty {
                Text("no windows")
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.muted)
                    .padding(.leading, 11)
            } else {
                HStack(alignment: .top, spacing: 6) {
                    ForEach(gauges.prefix(5)) { gauge in
                        limitsRing(gauge, accent: accent)
                    }
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Palette.panel.opacity(0.96))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(Palette.cardStroke, lineWidth: 1)
        )
    }

    private func limitsRing(_ g: Gauge, accent: Color) -> some View {
        let color = Palette.gauge(accent, g.severity)
        return VStack(spacing: 5) {
            ZStack(alignment: .topTrailing) {
                RingGauge(gauge: g, accent: color, diameter: 64)
                if g.isActive {
                    Circle()
                        .fill(Palette.rose)
                        .frame(width: 7, height: 7)
                        .offset(x: 2, y: -1)
                }
            }
            Text(g.label)
                .font(.system(size: 10.5, weight: .medium))
                .foregroundStyle(Palette.secondaryText)
                .multilineTextAlignment(.center)
                .lineLimit(2)
                .frame(minHeight: 24, alignment: .top)
            if let sub = ringSub(g) {
                Text(sub)
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(Palette.muted)
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity)
    }

    private func ringSub(_ g: Gauge) -> String? {
        if g.unit == .usd, let u = g.used, let l = g.limit {
            return String(format: "$%.0f/$%.0f", u, l)
        }
        if let reset = g.resetsAt {
            return (g.trustedReset ? "" : "~") + Fmt.until(reset)
        }
        return g.detail
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
        return write(
            image: render(ExportView(dashboard: dashboard, sections: sections)),
            to: path,
            label: "dashboard"
        )
    }

    @discardableResult
    static func exportLimits(dashboard: Dashboard, to path: URL? = nil) -> URL? {
        write(image: render(LimitsExportView(dashboard: dashboard)), to: path, label: "limits")
    }

    private static func render<V: View>(_ content: V) -> NSImage? {
        let renderer = ImageRenderer(content: content)
        renderer.scale = 2.0
        return renderer.nsImage
    }

    private static func write(image: NSImage?, to path: URL?, label: String) -> URL? {
        guard let image,
              let tiff = image.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiff),
              let png = bitmap.representation(using: .png, properties: [:])
        else { return nil }

        let url = path ?? defaultOutput(label: label)
        do {
            try png.write(to: url)
        } catch {
            NSLog("tokenmaxxing: export write failed: \(error)")
            return nil
        }

        let pb = NSPasteboard.general
        pb.clearContents()
        pb.declareTypes([.png, .tiff], owner: nil)
        pb.setData(png, forType: .png)
        pb.writeObjects([image])
        if path == nil {
            NSWorkspace.shared.activateFileViewerSelecting([url])
        }
        return url
    }

    static func defaultOutput(label: String = "dashboard") -> URL {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        let dir = FileManager.default.urls(for: .picturesDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser
        return dir.appending(path: "tokenmaxxing-\(label)-\(formatter.string(from: Date())).png")
    }
}
