import SwiftUI

/// A horizontal band within a section: either one full-width panel or an
/// adaptive grid of uniform-height panels. Grouping panels this way keeps the
/// SwiftUI layout reliable while still reflowing to the window width.
enum Band {
    case full(PanelSpec, height: CGFloat)
    case grid([PanelSpec], minWidth: CGFloat, height: CGFloat)
}

struct SectionSpec: Identifiable {
    let id: String
    var title: String
    var authority: Authority?
    var source: String?
    var bands: [Band]
}

// MARK: - Section construction (mirrors the KDE build_cards)

func buildSections(_ dash: Dashboard) -> [SectionSpec] {
    [
        claudeQuotaSection(dash.claudeQuota),
        usageSection(dash.claudeUsage, id: "claude-usage", title: "Claude usage", kind: .claude,
                     roi: Palette.planMonthlyUSD(dash.claudeQuota.subtitle)),
        grokQuotaSection(dash.grokQuota),
        usageSection(dash.grokUsage, id: "grok-usage", title: "Grok usage", kind: .grok, roi: nil),
        opencodeQuotaSection(dash.opencodeQuota),
        usageSection(dash.opencodeUsage, id: "opencode-usage", title: "opencode usage", kind: .opencode, roi: nil),
    ]
}

private func claudeQuotaSection(_ snap: Snapshot) -> SectionSpec {
    if let error = snap.error {
        return SectionSpec(id: "claude-quota", title: "Claude — live quota", authority: snap.authority, source: snap.source,
                           bands: [.full(PanelSpec(id: "claude-quota-err", kind: .callout(title: "Claude quota unavailable", headline: "OFFLINE", body: error, accent: Palette.pink)), height: 100)])
    }
    let accent = Palette.aqua
    var top: [PanelSpec] = []
    if let binding = snap.bindingGauge {
        top.append(PanelSpec(id: "claude-binding", kind: .heroRing(gauge: binding, accent: accent, authority: snap.authority)))
    }
    top.append(PanelSpec(id: "claude-rings", kind: .rings(title: "Rate-limit windows", gauges: snap.gauges, accent: accent)))
    if let spend = snap.spend {
        let value = spend.enabled ? Fmt.usdCents(spend.used) : "off"
        let sub = spend.enabled ? (spend.limit.map { "of \(Fmt.usdCents($0)) cap" } ?? "used") : "extra-usage credits"
        top.append(PanelSpec(id: "claude-credits", kind: .kpi(value: value, label: "Overflow credits", sub: sub, accent: Palette.lime)))
    }

    var bands: [Band] = [.grid(top, minWidth: 300, height: 156)]
    let ticks = resetTicks(snap, accent)
    if !ticks.isEmpty {
        bands.append(.full(PanelSpec(id: "claude-reset-horizon", kind: .resetHorizon(title: "Reset horizon — next unlocks", ticks: ticks)), height: 108))
    }
    return SectionSpec(id: "claude-quota", title: "Claude — live quota", authority: snap.authority, source: snap.source, bands: bands)
}

private func grokQuotaSection(_ snap: Snapshot) -> SectionSpec {
    if let error = snap.error {
        return SectionSpec(id: "grok-quota", title: "Grok — live credits", authority: snap.authority, source: snap.source,
                           bands: [.full(PanelSpec(id: "grok-quota-err", kind: .callout(title: "Grok quota unavailable", headline: "OFFLINE", body: error, accent: Palette.pink)), height: 100)])
    }
    let accent = Palette.violet
    var top: [PanelSpec] = []
    if let binding = snap.bindingGauge {
        top.append(PanelSpec(id: "grok-binding", kind: .heroRing(gauge: binding, accent: accent, authority: snap.authority)))
    }
    top.append(PanelSpec(id: "grok-rings", kind: .rings(title: "Credit windows", gauges: snap.gauges, accent: accent)))
    if let spend = snap.spend {
        let value = spend.balance.map { Fmt.usdCents($0) } ?? "—"
        let sub = spend.enabled ? "prepaid remaining" : "no prepaid balance"
        top.append(PanelSpec(id: "grok-prepaid", kind: .kpi(value: value, label: "Prepaid balance", sub: sub, accent: Palette.teal)))
    }
    var bands: [Band] = [.grid(top, minWidth: 300, height: 156)]
    let ticks = resetTicks(snap, accent)
    if !ticks.isEmpty {
        bands.append(.full(PanelSpec(id: "grok-reset-horizon", kind: .resetHorizon(title: "Reset horizon — next unlocks", ticks: ticks)), height: 108))
    }
    return SectionSpec(id: "grok-quota", title: "Grok — live credits", authority: snap.authority, source: snap.source, bands: bands)
}

private func opencodeQuotaSection(_ snap: Snapshot) -> SectionSpec {
    if let error = snap.error {
        return SectionSpec(id: "opencode-quota", title: "opencode — rolling caps", authority: snap.authority, source: snap.source,
                           bands: [.full(PanelSpec(id: "oc-quota-err", kind: .callout(title: "opencode caps unavailable", headline: "OFFLINE", body: error, accent: Palette.pink)), height: 100)])
    }
    var panels: [PanelSpec] = [
        PanelSpec(id: "oc-rings", kind: .rings(title: "Estimated spend vs Go caps", gauges: snap.gauges, accent: Palette.lime))
    ]
    if let note = snap.note {
        panels.append(PanelSpec(id: "oc-note", kind: .callout(title: "Estimate — read this", headline: "EST only", body: note, accent: Palette.teal)))
    }
    return SectionSpec(id: "opencode-quota", title: "opencode — rolling caps", authority: snap.authority, source: snap.source,
                       bands: [.grid(panels, minWidth: 320, height: 156)])
}

private enum UsageKind {
    case claude, grok, opencode

    var accent: Color {
        switch self {
        case .claude: Palette.aqua
        case .grok: Palette.violet
        case .opencode: Palette.lime
        }
    }

    var isClaude: Bool { self == .claude }
    var activityOnly: Bool { self == .grok }
}

private func usageSection(_ usage: Usage, id: String, title: String, kind: UsageKind, roi: Double?) -> SectionSpec {
    if usage.isEmpty {
        let msg = usage.error ?? "no local usage yet"
        return SectionSpec(id: id, title: title, authority: usage.authority, source: usage.source,
                           bands: [.full(PanelSpec(id: "\(id)-empty", kind: .callout(title: "No usage history", headline: "", body: msg, accent: Palette.muted)), height: 96)])
    }
    let accent = kind.accent
    let isClaude = kind.isClaude
    let activityOnly = kind.activityOnly
    let t = usage.totals

    var kpis: [PanelSpec] = []
    if activityOnly {
        kpis.append(PanelSpec(id: "\(id)-30d", kind: .kpi(value: Fmt.count(usage.windows.thirty.messages), label: "Turns 30d", sub: "\(Fmt.count(usage.windows.today.messages)) today", accent: accent)))
        kpis.append(PanelSpec(id: "\(id)-alltime", kind: .kpi(value: Fmt.count(t.messages), label: "Turns all-time", sub: "over \(t.activeDays) days", accent: Palette.lime)))
        kpis.append(PanelSpec(id: "\(id)-sessions", kind: .kpi(value: "\(t.sessions)", label: "Sessions", sub: "\(t.activeDays) active days", accent: Palette.teal)))
        kpis.append(PanelSpec(id: "\(id)-models", kind: .kpi(value: "\(usage.byModel.count)", label: "Models used", sub: usage.byModel.first?.label ?? "—", accent: Palette.azure)))
    } else {
        if let plan = roi {
            let sub = plan > 0
                ? "≈\(String(format: "%.0f", usage.windows.thirty.cost / plan))× your ~\(Fmt.usd(plan))/mo plan"
                : "API-equivalent value"
            kpis.append(PanelSpec(id: "\(id)-value-hero", kind: .kpi(value: Fmt.usd(usage.windows.thirty.cost), label: "Value returned 30d", sub: sub, accent: Palette.lime)))
        } else {
            kpis.append(PanelSpec(id: "\(id)-30d", kind: .kpi(value: Fmt.usd(usage.windows.thirty.cost), label: "Spend 30d", sub: "\(Fmt.usd(usage.windows.today.cost)) today", accent: accent)))
        }
        kpis.append(PanelSpec(id: "\(id)-alltime", kind: .kpi(value: Fmt.usd(t.costUSD), label: isClaude ? "Value all-time" : "Spend all-time", sub: "over \(t.activeDays) days", accent: Palette.lime)))
        kpis.append(PanelSpec(id: "\(id)-tokens", kind: .kpi(value: Fmt.count(t.totalTokens), label: "Tokens all-time", sub: "\(Fmt.count(t.messages)) msgs", accent: Palette.violet)))
        kpis.append(PanelSpec(id: "\(id)-sessions", kind: .kpi(value: "\(t.sessions)", label: "Sessions", sub: "\(t.activeDays) active days", accent: Palette.teal)))
        kpis.append(PanelSpec(id: "\(id)-cache", kind: .kpi(value: Fmt.percent(usage.cacheHitRate), label: "Cache hit rate", sub: "\(Fmt.count(t.cacheRead)) cached", accent: Palette.azure)))
        if isClaude {
            kpis.append(PanelSpec(id: "\(id)-tools", kind: .kpi(value: Fmt.count(t.webSearch + t.webFetch), label: "Web tool calls", sub: "\(t.webSearch) search · \(t.webFetch) fetch", accent: Palette.orange)))
        } else {
            kpis.append(PanelSpec(id: "\(id)-reason", kind: .kpi(value: Fmt.count(usage.tokens.reasoning), label: "Reasoning tokens", sub: "across providers", accent: Palette.orange)))
        }
    }

    let charts: [PanelSpec]
    if activityOnly {
        let avg = t.activeDays == 0 ? 0.0 : Double(t.messages) / Double(t.activeDays)
        let msgSeries = tail(usage.daily, 45).map { Double($0.messages) }
        let msgPeak = usage.daily.map(\.messages).max() ?? 0
        charts = [
            PanelSpec(id: "\(id)-burn", kind: .callout(
                title: "Activity — turns/day",
                headline: String(format: "%.1f", avg),
                body: "≈ \(String(format: "%.0f", avg * 30))/mo · today \(Fmt.count(usage.windows.today.messages))",
                accent: accent)),
            PanelSpec(id: "\(id)-daily-msgs", kind: .area(title: "Daily turns (45d)", series: msgSeries, accent: accent, caption: "peak \(Fmt.count(msgPeak))")),
        ]
    } else {
        let avg = usage.avgDailyCost
        let costSeries = tail(usage.daily, 45).map(\.cost)
        let tokenSeries = tail(usage.daily, 45).map { Double($0.tokens) }
        let costPeak = usage.daily.map(\.cost).max() ?? 0
        let tokenPeak = usage.daily.map(\.tokens).max() ?? 0
        charts = [
            PanelSpec(id: "\(id)-burn", kind: .callout(
                title: isClaude ? "Burn rate — value/day" : "Burn rate — spend/day",
                headline: Fmt.usd(avg),
                body: "≈ \(Fmt.usd(avg * 30))/mo · today \(Fmt.usd(usage.windows.today.cost))",
                accent: accent)),
            PanelSpec(id: "\(id)-daily-cost", kind: .area(title: isClaude ? "Daily value (45d)" : "Daily spend (45d)", series: costSeries, accent: accent, caption: "peak \(Fmt.usd(costPeak))")),
            PanelSpec(id: "\(id)-daily-tokens", kind: .area(title: "Daily tokens (45d)", series: tokenSeries, accent: Palette.violet, caption: "peak \(Fmt.count(tokenPeak))")),
        ]
    }

    var breakdown: [PanelSpec] = [
        PanelSpec(id: "\(id)-by-model", kind: .bars(title: "By model", rows: barRows(usage.byModel), caption: "top \(min(usage.byModel.count, 6))")),
    ]
    if !usage.byProvider.isEmpty {
        breakdown.append(PanelSpec(id: "\(id)-by-provider", kind: .bars(title: "By provider", rows: barRows(usage.byProvider), caption: "top \(min(usage.byProvider.count, 6))")))
        breakdown.append(freePaidPanel(id: "\(id)-freepaid", usage: usage))
    }
    if !usage.byProject.isEmpty {
        breakdown.append(PanelSpec(id: "\(id)-by-project", kind: .bars(title: "By project", rows: barRows(usage.byProject), caption: "top \(min(usage.byProject.count, 6))")))
    }
    if !activityOnly {
        breakdown.append(compositionPanel(id: "\(id)-tokens", usage: usage))
    }

    let heatmap = PanelSpec(id: "\(id)-heatmap", kind: .heatmap(title: "Activity — when you work", counts: usage.heatmap.counts, maxV: usage.heatmap.max, accent: accent, caption: "msgs / hour"))
    var statRows: [StatRow] = [
        StatRow(key: "First activity", value: usage.totals.firstDay.map(dayString) ?? "—"),
        StatRow(key: "Latest activity", value: usage.totals.lastDay.map(dayString) ?? "—"),
    ]
    if activityOnly {
        statRows.append(contentsOf: [
            StatRow(key: "Turns", value: Fmt.count(t.messages)),
            StatRow(key: "Sessions", value: "\(t.sessions)"),
            StatRow(key: "Projects", value: "\(usage.byProject.count)"),
            StatRow(key: "Models", value: "\(usage.byModel.count)"),
        ])
    } else {
        statRows.append(contentsOf: [
            StatRow(key: "Input tokens", value: Fmt.count(usage.tokens.input)),
            StatRow(key: "Output tokens", value: Fmt.count(usage.tokens.output)),
            StatRow(key: "Cache write", value: Fmt.count(usage.tokens.cacheWrite)),
            StatRow(key: "Cache read", value: Fmt.count(usage.tokens.cacheRead)),
        ])
        if usage.tokens.reasoning > 0 {
            statRows.append(StatRow(key: "Reasoning", value: Fmt.count(usage.tokens.reasoning)))
        }
    }
    let stat = PanelSpec(id: "\(id)-detail", kind: .stat(title: "Detail", rows: statRows))

    return SectionSpec(id: id, title: title, authority: usage.authority, source: usage.source, bands: [
        .grid(kpis, minWidth: 150, height: 104),
        .grid(charts, minWidth: 260, height: 150),
        .grid(breakdown, minWidth: 250, height: 210),
        .grid([heatmap, stat], minWidth: 300, height: 210),
    ])
}

// MARK: - Panel-data helpers

private func barRows(_ segments: [Segment]) -> [BarRow] {
    let byTokens = segments.contains { $0.tokens > 0 }
    let ordered = segments.sorted {
        byTokens ? $0.tokens > $1.tokens : $0.messages > $1.messages
    }.prefix(6)
    return ordered.enumerated().map { index, s in
        let value = byTokens ? s.tokens : s.messages
        return BarRow(label: s.label, value: Double(value),
                      caption: s.cost > 0 ? Fmt.usd(s.cost) : Fmt.count(value),
                      color: Palette.series(index))
    }
}

private func compositionPanel(id: String, usage: Usage) -> PanelSpec {
    let t = usage.tokens
    var slices = [
        DonutSlice(value: Double(t.input), color: Palette.tokenInput),
        DonutSlice(value: Double(t.output), color: Palette.tokenOutput),
        DonutSlice(value: Double(t.cacheWrite), color: Palette.tokenCacheWrite),
        DonutSlice(value: Double(t.cacheRead), color: Palette.tokenCacheRead),
    ]
    var legend = [
        LegendItem(label: "Input", color: Palette.tokenInput, value: Fmt.count(t.input)),
        LegendItem(label: "Output", color: Palette.tokenOutput, value: Fmt.count(t.output)),
        LegendItem(label: "Cache write", color: Palette.tokenCacheWrite, value: Fmt.count(t.cacheWrite)),
        LegendItem(label: "Cache read", color: Palette.tokenCacheRead, value: Fmt.count(t.cacheRead)),
    ]
    if t.reasoning > 0 {
        slices.append(DonutSlice(value: Double(t.reasoning), color: Palette.tokenReasoning))
        legend.append(LegendItem(label: "Reasoning", color: Palette.tokenReasoning, value: Fmt.count(t.reasoning)))
    }
    return PanelSpec(id: id, kind: .composition(title: "Token composition", segments: slices, legend: legend, caption: Fmt.count(t.total)))
}

private func freePaidPanel(id: String, usage: Usage) -> PanelSpec {
    let paid = usage.byProvider.filter { $0.cost > 0 }.reduce(Int64(0)) { $0 + $1.tokens }
    let free = usage.byProvider.filter { $0.cost <= 0 }.reduce(Int64(0)) { $0 + $1.tokens }
    return PanelSpec(id: id, kind: .donut(
        title: "Free vs paid",
        slices: [DonutSlice(value: Double(paid), color: Palette.lime), DonutSlice(value: Double(free), color: Palette.azure)],
        centerTop: Fmt.count(paid + free),
        centerBottom: "tokens",
        legend: [
            LegendItem(label: "Paid (Go)", color: Palette.lime, value: Fmt.usd(usage.totals.costUSD)),
            LegendItem(label: "Free / local", color: Palette.azure, value: Fmt.count(free)),
        ]))
}

private func resetTicks(_ snap: Snapshot, _ accent: Color) -> [ResetTick] {
    let now = Date()
    return snap.gauges.compactMap { g -> ResetTick? in
        guard let reset = g.resetsAt else { return nil }
        let seconds = Int(reset.timeIntervalSince(now))
        guard seconds >= 0, seconds <= 7 * 86_400 else { return nil }
        return ResetTick(label: g.label, seconds: seconds, trusted: g.trustedReset, color: Palette.gauge(accent, g.severity))
    }
    .sorted { $0.seconds < $1.seconds }
}

private func tail<T>(_ arr: [T], _ n: Int) -> [T] { arr.count > n ? Array(arr.suffix(n)) : arr }

private func dayString(_ date: Date) -> String {
    let f = DateFormatter()
    f.dateFormat = "yyyy-MM-dd"
    return f.string(from: date)
}

// MARK: - Rendering

struct SectionHeader: View {
    var title: String
    var authority: Authority?
    var source: String?
    var accent: Color = Palette.aqua

    var body: some View {
        HStack(spacing: 8) {
            RoundedRectangle(cornerRadius: 1.5, style: .continuous)
                .fill(accent)
                .frame(width: 3, height: 16)
            Text(title)
                .font(.system(size: 15, weight: .bold, design: .rounded))
                .foregroundStyle(Palette.text)
            if let authority { BadgePill(authority: authority, scale: 0.95) }
            if let source {
                Text(source)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(Palette.muted)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .padding(.top, 2)
    }
}

/// The dashboard body — a stack of sections, each with its bands. Used both live
/// (glass) and for the export (solid).
struct DashboardContent: View {
    var dashboard: Dashboard
    var sections: [SectionSpec]
    var glass: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            ForEach(sections) { section in
                VStack(alignment: .leading, spacing: 8) {
                    SectionHeader(
                        title: section.title,
                        authority: section.authority,
                        source: section.source,
                        accent: sectionAccent(section.id)
                    )
                    ForEach(Array(section.bands.enumerated()), id: \.offset) { _, band in
                        bandView(band)
                    }
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .background(backdrop)
    }

    private func sectionAccent(_ id: String) -> Color {
        if id.contains("claude") { return Palette.aqua }
        if id.contains("grok") { return Palette.violet }
        if id.contains("opencode") || id.contains("oc") { return Palette.lime }
        return Palette.aqua
    }

    @ViewBuilder private func bandView(_ band: Band) -> some View {
        switch band {
        case let .full(spec, height):
            PanelCard(glass: glass) { PanelView(kind: spec.kind) }.frame(height: height)
        case let .grid(specs, minWidth, height):
            LazyVGrid(columns: [GridItem(.adaptive(minimum: minWidth, maximum: .infinity), spacing: 14)], spacing: 14) {
                ForEach(specs) { spec in
                    PanelCard(glass: glass) { PanelView(kind: spec.kind) }.frame(height: height)
                }
            }
        }
    }

    private var backdrop: some View {
        ZStack {
            Palette.canvas
            LinearGradient(
                colors: [
                    Palette.aqua.opacity(0.07),
                    .clear,
                    Palette.lime.opacity(0.04),
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        }
        .ignoresSafeArea()
    }
}

/// The full-window live dashboard, with its refresh and screenshot toolbar.
struct DashboardView: View {
    @Environment(AppModel.self) private var model
    @State private var showScreenshot = false

    var body: some View {
        ZStack {
            Palette.canvas.ignoresSafeArea()
            if let dash = model.dashboard {
                ScrollView {
                    DashboardContent(dashboard: dash, sections: buildSections(dash), glass: true)
                }
            } else {
                VStack(spacing: 10) {
                    ProgressView()
                    Text("Reading quotas & usage history…")
                        .font(.system(size: 13))
                        .foregroundStyle(Palette.muted)
                }
            }
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { model.refresh() } label: { Image(systemName: "arrow.clockwise") }
            }
            ToolbarItem(placement: .primaryAction) {
                Button { showScreenshot = true } label: { Image(systemName: "camera") }
            }
        }
        .sheet(isPresented: $showScreenshot) {
            if let dash = model.dashboard {
                ScreenshotSheet(dashboard: dash)
            }
        }
    }
}

/// Choose which sections to include in a high-resolution PNG.
struct ScreenshotSheet: View {
    var dashboard: Dashboard
    @Environment(\.dismiss) private var dismiss
    @State private var selected: Set<String>

    private let sections: [SectionSpec]

    init(dashboard: Dashboard) {
        self.dashboard = dashboard
        let built = buildSections(dashboard)
        self.sections = built
        _selected = State(initialValue: Set(built.map(\.id)))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Screenshot").font(.title2.bold())
            Text("Choose the sections to include.").font(.callout).foregroundStyle(.secondary)
            ForEach(sections) { section in
                Toggle(section.title, isOn: Binding(
                    get: { selected.contains(section.id) },
                    set: { on in if on { selected.insert(section.id) } else { selected.remove(section.id) } }
                ))
            }
            HStack {
                Button("Everything") {
                    DashboardExport.export(dashboard: dashboard, sections: sections)
                    dismiss()
                }
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Export selected") {
                    let chosen = sections.filter { selected.contains($0.id) }
                    DashboardExport.export(dashboard: dashboard, sections: chosen)
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(selected.isEmpty)
            }
            .padding(.top, 6)
        }
        .padding(20)
        .frame(width: 340)
    }
}
