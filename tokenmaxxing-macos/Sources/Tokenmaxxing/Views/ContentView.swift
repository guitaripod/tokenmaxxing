import SwiftUI
import AppKit

/// The menu-bar popover: a compact, sharp status summary plus a launcher for
/// the full dashboard. Three providers stack as dense cards.
struct ContentView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
                .padding(.horizontal, 14)
                .padding(.top, 12)
                .padding(.bottom, 10)

            Divider().overlay(Palette.track.opacity(0.7))

            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    if let dash = model.dashboard {
                        providerCard(
                            name: "Claude",
                            subtitle: dash.claudeQuota.subtitle,
                            authority: dash.claudeQuota.authority,
                            gauges: dash.claudeQuota.gauges,
                            accent: Palette.aqua,
                            error: dash.claudeQuota.error
                        )
                        providerCard(
                            name: "Grok",
                            subtitle: dash.grokQuota.subtitle,
                            authority: dash.grokQuota.authority,
                            gauges: dash.grokQuota.gauges,
                            accent: Palette.violet,
                            error: dash.grokQuota.error
                        )
                        providerCard(
                            name: "opencode",
                            subtitle: dash.opencodeQuota.subtitle,
                            authority: dash.opencodeQuota.authority,
                            gauges: dash.opencodeQuota.gauges,
                            accent: Palette.lime,
                            error: dash.opencodeQuota.error
                        )
                    } else {
                        HStack(spacing: 8) {
                            ProgressView().controlSize(.small)
                            Text("Reading quotas…")
                                .font(.system(size: 12))
                                .foregroundStyle(Palette.muted)
                        }
                        .frame(maxWidth: .infinity, alignment: .center)
                        .padding(.vertical, 28)
                    }
                }
                .padding(12)
            }
            .frame(maxHeight: 420)

            Divider().overlay(Palette.track.opacity(0.7))

            actions
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
        }
        .frame(width: 420)
        .background(Palette.canvas)
    }

    private var header: some View {
        HStack(spacing: 10) {
            TokenmaxxingMark(size: 22)
            VStack(alignment: .leading, spacing: 1) {
                Text("tokenmaxxing")
                    .font(.system(size: 13.5, weight: .bold, design: .rounded))
                    .foregroundStyle(Palette.text)
                Text(model.updatedText)
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(Palette.muted)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
    }

    /// Dense provider strip — accent rail, title/badge, tight ring row.
    private func providerCard(
        name: String,
        subtitle: String,
        authority: Authority,
        gauges: [Gauge],
        accent: Color,
        error: String?
    ) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                RoundedRectangle(cornerRadius: 1.5, style: .continuous)
                    .fill(accent)
                    .frame(width: 3, height: 14)
                Text(name)
                    .font(.system(size: 12.5, weight: .semibold, design: .rounded))
                    .foregroundStyle(Palette.text)
                BadgePill(authority: authority, scale: 0.9)
                if !subtitle.isEmpty {
                    Text(subtitle)
                        .font(.system(size: 10.5))
                        .foregroundStyle(Palette.muted)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
            }

            if let error {
                Text(error)
                    .font(.system(size: 11))
                    .foregroundStyle(Palette.rose)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.leading, 11)
            } else if gauges.isEmpty {
                Text("no windows")
                    .font(.system(size: 11))
                    .foregroundStyle(Palette.muted)
                    .padding(.leading, 11)
            } else {
                HStack(alignment: .top, spacing: 4) {
                    ForEach(gauges.prefix(5)) { gauge in
                        compactRing(gauge, accent: accent)
                    }
                }
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(Palette.panel.opacity(colorScheme == .dark ? 0.72 : 0.96))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(Palette.cardStroke.opacity(colorScheme == .dark ? 0.9 : 1), lineWidth: 1)
        )
    }

    private func compactRing(_ g: Gauge, accent: Color) -> some View {
        let color = Palette.gauge(accent, g.severity)
        return VStack(spacing: 4) {
            ZStack(alignment: .topTrailing) {
                RingGauge(gauge: g, accent: color, diameter: 52)
                if g.isActive {
                    Circle()
                        .fill(Palette.rose)
                        .frame(width: 6, height: 6)
                        .offset(x: 2, y: -1)
                }
            }
            Text(g.label)
                .font(.system(size: 9.5, weight: .medium))
                .foregroundStyle(Palette.secondaryText)
                .multilineTextAlignment(.center)
                .lineLimit(2)
                .frame(minHeight: 22, alignment: .top)
            if let sub = ringSub(g) {
                Text(sub)
                    .font(.system(size: 9, design: .monospaced))
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

    private var actions: some View {
        VStack(alignment: .leading, spacing: 1) {
            actionButton("Open dashboard", systemImage: "square.grid.2x2") {
                openWindow(id: "dashboard")
            }
            actionButton("Refresh", systemImage: "arrow.clockwise") {
                model.refresh()
            }
            actionButton("Export screenshot", systemImage: "camera") {
                model.exportShareCard()
            }
            Toggle(isOn: launchBinding) {
                Label("Launch at login", systemImage: "power.circle")
                    .font(.system(size: 12))
            }
            .toggleStyle(.checkbox)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)

            actionButton("Grok usage", systemImage: "link") {
                if let url = URL(string: "https://grok.com/?_s=usage") {
                    NSWorkspace.shared.open(url)
                }
            }
            actionButton("opencode console", systemImage: "link") {
                if let url = URL(string: "https://opencode.ai/auth") {
                    NSWorkspace.shared.open(url)
                }
            }

            Divider().overlay(Palette.track.opacity(0.7)).padding(.vertical, 4)

            actionButton("Quit tokenmaxxing", systemImage: "xmark.circle") {
                NSApplication.shared.terminate(nil)
            }
        }
    }

    private func actionButton(_ title: String, systemImage: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .font(.system(size: 12))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 8)
                .padding(.vertical, 5)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(Palette.text)
    }

    private var launchBinding: Binding<Bool> {
        Binding(get: { model.launchAtLogin }, set: { model.setLaunchAtLogin($0) })
    }
}
