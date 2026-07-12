import SwiftUI
import AppKit

/// The menu-bar popover: a compact status summary plus a launcher for the full
/// dashboard window. The dashboard is where the detail lives.
struct ContentView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            content
            Divider().overlay(Palette.track)
            actions
        }
        .padding(14)
        .frame(width: 460)
        .background(Palette.base)
    }

    private var header: some View {
        HStack(spacing: 10) {
            TokenmaxxingMark(size: 26)
            VStack(alignment: .leading, spacing: 0) {
                Text("tokenmaxxing").font(.system(size: 15, weight: .bold, design: .rounded)).foregroundStyle(Palette.text)
                Text(model.updatedText).font(.system(size: 10, design: .monospaced)).foregroundStyle(Palette.muted).lineLimit(1)
            }
            Spacer()
        }
    }

    @ViewBuilder private var content: some View {
        if let dash = model.dashboard {
            VStack(alignment: .leading, spacing: 14) {
                ringsRow("Claude — live quota", authority: dash.claudeQuota.authority, gauges: dash.claudeQuota.gauges, accent: Palette.aqua, error: dash.claudeQuota.error)
                ringsRow("opencode — rolling caps", authority: dash.opencodeQuota.authority, gauges: dash.opencodeQuota.gauges, accent: Palette.lime, error: dash.opencodeQuota.error)
            }
        } else {
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text("Reading quotas…").font(.system(size: 12)).foregroundStyle(Palette.muted)
            }
            .frame(maxWidth: .infinity, alignment: .center)
            .padding(.vertical, 12)
        }
    }

    /// A provider's quota as a row of rings — the entire content of the popover.
    private func ringsRow(_ title: String, authority: Authority, gauges: [Gauge], accent: Color, error: String?) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Text(title).font(.system(size: 12.5, weight: .bold)).foregroundStyle(Palette.text)
                BadgePill(authority: authority, scale: 1)
                Spacer()
            }
            if let error {
                Text(error).font(.system(size: 11)).foregroundStyle(Palette.rose).fixedSize(horizontal: false, vertical: true)
            } else {
                HStack(alignment: .top, spacing: 8) {
                    ForEach(gauges.prefix(5)) { gauge in
                        VStack(spacing: 3) {
                            ZStack(alignment: .top) {
                                RingGauge(gauge: gauge, accent: Palette.gauge(accent, gauge.severity), diameter: 68)
                                if gauge.isActive {
                                    Text("ACTIVE").font(.system(size: 7.5, weight: .bold))
                                        .padding(.horizontal, 4).padding(.vertical, 1)
                                        .background(Capsule().fill(Palette.pink))
                                        .foregroundStyle(Palette.base).offset(y: -4)
                                }
                            }
                            Text(gauge.label)
                                .font(.system(size: 9.5)).foregroundStyle(Palette.text.opacity(0.85))
                                .multilineTextAlignment(.center).lineLimit(2)
                                .frame(minHeight: 24, alignment: .top)
                            if let sub = ringSub(gauge) {
                                Text(sub).font(.system(size: 8.5, design: .monospaced)).foregroundStyle(Palette.muted).lineLimit(1)
                            }
                        }
                        .frame(maxWidth: .infinity)
                    }
                }
            }
        }
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
        VStack(alignment: .leading, spacing: 2) {
            Button { openWindow(id: "dashboard") } label: {
                Label("Open dashboard", systemImage: "square.grid.2x2")
            }
            .buttonStyle(.plain)
            Button { model.refresh() } label: { Label("Refresh", systemImage: "arrow.clockwise") }.buttonStyle(.plain)
            Button { model.exportShareCard() } label: { Label("Export screenshot", systemImage: "camera") }.buttonStyle(.plain)
            Toggle("Launch at login", isOn: launchBinding).toggleStyle(.checkbox).font(.system(size: 12))
            Button {
                if let url = URL(string: "https://opencode.ai/auth") { NSWorkspace.shared.open(url) }
            } label: { Label("Open opencode console", systemImage: "arrow.up.right.square") }.buttonStyle(.plain)
            Divider().overlay(Palette.track)
            Button { NSApplication.shared.terminate(nil) } label: { Label("Quit tokenmaxxing", systemImage: "power") }.buttonStyle(.plain)
        }
        .font(.system(size: 12.5))
        .foregroundStyle(Palette.text)
    }

    private var launchBinding: Binding<Bool> {
        Binding(get: { model.launchAtLogin }, set: { model.setLaunchAtLogin($0) })
    }
}
