import SwiftUI

struct StatRow: Identifiable {
    let id = UUID()
    var key: String
    var value: String
}

/// A data-driven panel description so the dashboard can be built, laid out, and
/// filtered for screenshots from one list — the same approach the KDE build uses.
enum PanelKind {
    case kpi(value: String, label: String, sub: String?, accent: Color)
    case heroRing(gauge: Gauge, accent: Color, authority: Authority)
    case rings(title: String, gauges: [Gauge], accent: Color)
    case callout(title: String, headline: String, body: String, accent: Color)
    case area(title: String, series: [Double], accent: Color, caption: String)
    case bars(title: String, rows: [BarRow], caption: String)
    case donut(title: String, slices: [DonutSlice], centerTop: String, centerBottom: String, legend: [LegendItem])
    case heatmap(title: String, counts: [[Int]], maxV: Int, accent: Color, caption: String)
    case composition(title: String, segments: [DonutSlice], legend: [LegendItem], caption: String)
    case resetHorizon(title: String, ticks: [ResetTick])
    case stat(title: String, rows: [StatRow])
}

struct PanelSpec: Identifiable {
    let id: String
    var kind: PanelKind
}

/// The glass (or solid, for export) card that wraps every panel.
struct PanelCard<Content: View>: View {
    var glass: Bool
    var accent: Color = Palette.violet
    @ViewBuilder var content: () -> Content

    var body: some View {
        content()
            .padding(14)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .modifier(CardBackground(accent: accent, radius: 16, glass: glass))
    }
}

/// Renders one panel's inner content (without the card chrome).
struct PanelView: View {
    var kind: PanelKind

    var body: some View {
        switch kind {
        case let .kpi(value, label, sub, accent):
            KpiTile(value: value, label: label, sub: sub, accent: accent)
        case let .heroRing(gauge, accent, authority):
            HeroRingPanel(gauge: gauge, accent: accent, authority: authority)
        case let .rings(title, gauges, accent):
            RingsPanel(title: title, gauges: gauges, accent: accent)
        case let .callout(title, headline, body, accent):
            CalloutPanel(title: title, headline: headline, body: body, accent: accent)
        case let .area(title, series, accent, caption):
            TitledPanel(title: title, caption: caption) { AreaChart(series: series, accent: accent) }
        case let .bars(title, rows, caption):
            TitledPanel(title: title, caption: caption) { BarsChart(rows: rows) }
        case let .donut(title, slices, top, bottom, legend):
            TitledPanel(title: title, caption: nil) {
                HStack(spacing: 8) {
                    DonutChart(slices: slices, centerTop: top, centerBottom: bottom)
                        .frame(maxWidth: .infinity)
                    LegendView(items: legend).frame(width: 150)
                }
            }
        case let .heatmap(title, counts, maxV, accent, caption):
            TitledPanel(title: title, caption: caption) { HeatmapChart(counts: counts, maxV: maxV, accent: accent) }
        case let .composition(title, segments, legend, caption):
            TitledPanel(title: title, caption: caption) {
                VStack(alignment: .leading, spacing: 10) {
                    CompositionBar(segments: segments)
                    LegendView(items: legend)
                }
            }
        case let .resetHorizon(title, ticks):
            TitledPanel(title: title, caption: "next 7 days") { ResetHorizonChart(ticks: ticks) }
        case let .stat(title, rows):
            StatPanel(title: title, rows: rows)
        }
    }
}

struct TitledPanel<Content: View>: View {
    var title: String
    var caption: String?
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Text(title).font(.system(size: 13, weight: .bold)).foregroundStyle(Palette.text)
                Spacer()
                if let caption { Text(caption).font(.system(size: 11)).foregroundStyle(Palette.muted) }
            }
            content().frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

struct KpiTile: View {
    var value: String
    var label: String
    var sub: String?
    var accent: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label.uppercased()).font(.system(size: 10.5, weight: .bold)).foregroundStyle(Palette.muted).lineLimit(1)
            Text(value).font(.system(size: 27, weight: .bold, design: .rounded)).foregroundStyle(accent).lineLimit(1).minimumScaleFactor(0.5)
            if let sub {
                Text(sub).font(.system(size: 11)).foregroundStyle(Palette.muted).lineLimit(2).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

struct CalloutPanel: View {
    var title: String
    var headline: String
    var body_: String
    var accent: Color

    init(title: String, headline: String, body: String, accent: Color) {
        self.title = title
        self.headline = headline
        self.body_ = body
        self.accent = accent
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title.uppercased()).font(.system(size: 10.5, weight: .bold)).foregroundStyle(Palette.muted).lineLimit(1)
            Text(headline).font(.system(size: 24, weight: .bold, design: .rounded)).foregroundStyle(accent).lineLimit(1).minimumScaleFactor(0.5)
            if !body_.isEmpty {
                Text(body_).font(.system(size: 12)).foregroundStyle(Palette.text.opacity(0.82)).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

struct StatPanel: View {
    var title: String
    var rows: [StatRow]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.system(size: 13, weight: .bold)).foregroundStyle(Palette.text)
            ForEach(rows) { row in
                HStack {
                    Text(row.key).font(.system(size: 12)).foregroundStyle(Palette.muted)
                    Spacer()
                    Text(row.value).font(.system(size: 12.5, weight: .semibold, design: .monospaced)).foregroundStyle(Palette.text)
                }
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

struct RingsPanel: View {
    var title: String
    var gauges: [Gauge]
    var accent: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(.system(size: 13, weight: .bold)).foregroundStyle(Palette.text)
            HStack(alignment: .top, spacing: 6) {
                ForEach(gauges.prefix(5)) { gauge in
                    VStack(spacing: 4) {
                        ZStack(alignment: .top) {
                            RingGauge(gauge: gauge, accent: accent, diameter: 74)
                            if gauge.isActive {
                                Text("ACTIVE")
                                    .font(.system(size: 8, weight: .bold))
                                    .padding(.horizontal, 5).padding(.vertical, 1)
                                    .background(Capsule().fill(Palette.pink))
                                    .foregroundStyle(Palette.base)
                                    .offset(y: -4)
                            }
                        }
                        Text(gauge.label).font(.system(size: 10.5)).foregroundStyle(Palette.text.opacity(0.82))
                            .multilineTextAlignment(.center).lineLimit(2).frame(minHeight: 26, alignment: .top)
                        if let sub = ringSubline(gauge) {
                            Text(sub).font(.system(size: 9.5, design: .monospaced)).foregroundStyle(Palette.muted).lineLimit(1)
                        }
                    }
                    .frame(maxWidth: .infinity)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private func ringSubline(_ g: Gauge) -> String? {
        if g.unit == .usd, let u = g.used, let l = g.limit {
            return String(format: "$%.0f/$%.0f", u, l)
        }
        if let reset = g.resetsAt {
            return (g.trustedReset ? "" : "~") + Fmt.until(reset)
        }
        return g.detail
    }
}

struct HeroRingPanel: View {
    var gauge: Gauge
    var accent: Color
    var authority: Authority

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("CLOSEST LIMIT").font(.system(size: 10.5, weight: .bold)).foregroundStyle(Palette.muted)
                Spacer()
                BadgePill(authority: authority, scale: 1)
            }
            HStack(spacing: 16) {
                RingGauge(gauge: gauge, accent: Palette.gauge(accent, gauge.severity), diameter: 104)
                VStack(alignment: .leading, spacing: 6) {
                    Text(gauge.label).font(.system(size: 16, weight: .bold, design: .rounded)).foregroundStyle(Palette.text)
                        .fixedSize(horizontal: false, vertical: true)
                    if let reset = gauge.resetsAt {
                        Text((gauge.trustedReset ? "resets in " : "~resets in ") + Fmt.until(reset))
                            .font(.system(size: 12.5)).foregroundStyle(Palette.gauge(accent, gauge.severity))
                    }
                    if gauge.isActive {
                        Text("BINDING CONSTRAINT")
                            .font(.system(size: 9.5, weight: .bold))
                            .padding(.horizontal, 7).padding(.vertical, 2)
                            .background(Capsule().fill(Palette.pink))
                            .foregroundStyle(Palette.base)
                    }
                    Spacer(minLength: 0)
                }
                Spacer(minLength: 0)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}
