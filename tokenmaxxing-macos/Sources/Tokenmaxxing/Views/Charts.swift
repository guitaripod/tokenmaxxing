import SwiftUI

struct BarRow: Identifiable {
    let id = UUID()
    var label: String
    var value: Double
    var caption: String
    var color: Color
}

struct DonutSlice {
    var value: Double
    var color: Color
}

struct ResetTick: Identifiable {
    let id = UUID()
    var label: String
    var seconds: Int
    var trusted: Bool
    var color: Color
}

struct LegendItem: Identifiable {
    let id = UUID()
    var label: String
    var color: Color
    var value: String
}

/// A filled area chart of one series (daily cost or tokens), with a bright top
/// line and a highlighted last point.
struct AreaChart: View {
    var series: [Double]
    var accent: Color

    var body: some View {
        Canvas { ctx, size in
            let padY = size.height * 0.10
            let w = size.width
            let h = size.height
            let plotH = h - padY * 2

            var baseline = Path()
            baseline.move(to: CGPoint(x: 0, y: h - padY))
            baseline.addLine(to: CGPoint(x: w, y: h - padY))
            ctx.stroke(baseline, with: .color(Palette.track.opacity(0.8)), lineWidth: 1)

            guard series.count >= 2 else { return }
            let maxV = Swift.max(series.max() ?? 1, 0.000001)
            func point(_ i: Int, _ v: Double) -> CGPoint {
                CGPoint(
                    x: CGFloat(Double(i) / Double(series.count - 1)) * w,
                    y: (h - padY) - CGFloat(v / maxV) * plotH
                )
            }

            var fill = Path()
            fill.move(to: CGPoint(x: 0, y: h - padY))
            for (i, v) in series.enumerated() { fill.addLine(to: point(i, v)) }
            fill.addLine(to: CGPoint(x: w, y: h - padY))
            fill.closeSubpath()
            ctx.fill(fill, with: .linearGradient(
                Gradient(colors: [accent.opacity(0.34), accent.opacity(0.015)]),
                startPoint: CGPoint(x: 0, y: padY),
                endPoint: CGPoint(x: 0, y: h - padY)
            ))

            var line = Path()
            for (i, v) in series.enumerated() {
                let p = point(i, v)
                if i == 0 { line.move(to: p) } else { line.addLine(to: p) }
            }
            ctx.stroke(line, with: .color(accent), style: StrokeStyle(lineWidth: Swift.max(1.6, h * 0.02), lineCap: .round, lineJoin: .round))

            if let last = series.last {
                let p = point(series.count - 1, last)
                ctx.fill(Path(ellipseIn: CGRect(x: p.x - h * 0.05, y: p.y - h * 0.05, width: h * 0.1, height: h * 0.1)), with: .color(accent.opacity(0.28)))
                ctx.fill(Path(ellipseIn: CGRect(x: p.x - h * 0.03, y: p.y - h * 0.03, width: h * 0.06, height: h * 0.06)), with: .color(accent))
            }
        }
    }
}

/// A donut of proportional slices with two lines of centered text.
struct DonutChart: View {
    var slices: [DonutSlice]
    var centerTop: String
    var centerBottom: String

    var body: some View {
        Canvas { ctx, size in
            let cx = size.width / 2
            let cy = size.height / 2
            let radius = Swift.min(size.width, size.height) / 2 - size.height * 0.06
            let thickness = radius * 0.44
            let total = slices.reduce(0) { $0 + $1.value }
            let ringR = radius - thickness / 2

            var track = Path()
            track.addArc(center: CGPoint(x: cx, y: cy), radius: ringR, startAngle: .degrees(0), endAngle: .degrees(360), clockwise: false)
            ctx.stroke(track, with: .color(Palette.track), style: StrokeStyle(lineWidth: thickness))

            if total > 0 {
                var start = -90.0
                let gap = 2.5
                for s in slices where s.value > 0 {
                    let sweep = s.value / total * 360
                    var arc = Path()
                    arc.addArc(
                        center: CGPoint(x: cx, y: cy), radius: ringR,
                        startAngle: .degrees(start + gap),
                        endAngle: .degrees(start + sweep - Swift.min(gap, sweep / 2)),
                        clockwise: false
                    )
                    ctx.stroke(arc, with: .color(s.color), style: StrokeStyle(lineWidth: thickness, lineCap: .butt))
                    start += sweep
                }
            }

            ctx.draw(
                Text(centerTop).font(.system(size: radius * 0.4, weight: .bold, design: .rounded)).foregroundStyle(Palette.text),
                at: CGPoint(x: cx, y: cy - radius * 0.04), anchor: .center
            )
            ctx.draw(
                Text(centerBottom).font(.system(size: radius * 0.2)).foregroundStyle(Palette.muted),
                at: CGPoint(x: cx, y: cy + radius * 0.28), anchor: .center
            )
        }
    }
}

/// A 7×24 activity punch card: rows are weekdays (Mon top), columns are hours,
/// cell brightness scales with activity (square-root so light days stay visible).
struct HeatmapChart: View {
    var counts: [[Int]]
    var maxV: Int
    var accent: Color
    private let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]

    var body: some View {
        Canvas { ctx, size in
            let gutterL = size.width * 0.075
            let gutterB = size.height * 0.16
            let cellW = (size.width - gutterL) / 24
            let cellH = (size.height - gutterB) / 7
            let inset = Swift.min(Swift.min(cellW, cellH) * 0.12, 2)
            let denom = Double(Swift.max(maxV, 1))

            for row in 0..<7 {
                let cy = CGFloat(row) * cellH
                ctx.draw(
                    Text(days[row]).font(.system(size: Swift.min(Swift.max(cellH * 0.44, 8), 13))).foregroundStyle(Palette.muted),
                    at: CGPoint(x: gutterL - inset * 2, y: cy + cellH * 0.5), anchor: .trailing
                )
                for hour in 0..<24 {
                    let x = gutterL + CGFloat(hour) * cellW
                    let rect = CGRect(x: x + inset, y: cy + inset, width: cellW - inset * 2, height: cellH - inset * 2)
                    let path = Path(roundedRect: rect, cornerRadius: Swift.max(inset, 1.5))
                    let count = row < counts.count && hour < counts[row].count ? counts[row][hour] : 0
                    if count == 0 {
                        ctx.fill(path, with: .color(Palette.track.opacity(0.5)))
                    } else {
                        let intensity = Swift.min(Swift.max((Double(count) / denom).squareRoot(), 0.12), 1.0)
                        ctx.fill(path, with: .color(accent.opacity(intensity)))
                    }
                }
            }
            for hour in [0, 6, 12, 18, 23] {
                let x = gutterL + (CGFloat(hour) + 0.5) * cellW
                ctx.draw(
                    Text("\(hour)").font(.system(size: Swift.min(Swift.max(cellH * 0.4, 8), 12))).foregroundStyle(Palette.muted),
                    at: CGPoint(x: x, y: size.height - gutterB * 0.28), anchor: .bottom
                )
            }
        }
    }
}

/// A single soonest-first timeline collapsing every reset across both providers.
struct ResetHorizonChart: View {
    var ticks: [ResetTick]

    var body: some View {
        Canvas { ctx, size in
            let axisY = size.height * 0.5
            let span = 7.0 * 86_400.0

            var axis = Path()
            axis.move(to: CGPoint(x: 0, y: axisY))
            axis.addLine(to: CGPoint(x: size.width, y: axisY))
            ctx.stroke(axis, with: .color(Palette.track), lineWidth: 2)

            for d in 0...7 {
                let x = CGFloat(Double(d) / 7) * size.width
                var g = Path()
                g.move(to: CGPoint(x: x, y: axisY - 4))
                g.addLine(to: CGPoint(x: x, y: axisY + 4))
                ctx.stroke(g, with: .color(Palette.track.opacity(0.6)), lineWidth: 1)
                ctx.draw(Text("\(d)d").font(.system(size: 9.5)).foregroundStyle(Palette.muted), at: CGPoint(x: x, y: size.height - 2), anchor: .bottom)
            }

            var lastX = -CGFloat.infinity
            var below = false
            for t in ticks.sorted(by: { $0.seconds < $1.seconds }) {
                let frac = Swift.min(Swift.max(Double(t.seconds) / span, 0), 1)
                let x = CGFloat(frac) * size.width
                let dot = Path(ellipseIn: CGRect(x: x - 5, y: axisY - 5, width: 10, height: 10))
                if t.trusted {
                    ctx.fill(dot, with: .color(t.color))
                } else {
                    ctx.stroke(dot, with: .color(t.color), lineWidth: 2)
                }
                below = abs(x - lastX) < size.width * 0.16 ? !below : false
                lastX = x
                let ly = below ? axisY + 18 : axisY - 12
                let label = Self.short(t.label) + " · " + Fmt.until(t.seconds)
                ctx.draw(
                    Text(label).font(.system(size: 10, weight: .semibold)).foregroundStyle(t.color),
                    at: CGPoint(x: Swift.min(Swift.max(x, size.width * 0.14), size.width * 0.86), y: ly), anchor: .center
                )
            }
        }
    }

    private static func short(_ s: String) -> String {
        s.split(separator: "·").last.map { $0.trimmingCharacters(in: .whitespaces) } ?? s
    }
}

/// A ranked bar breakdown: label left, caption right, proportional bar below.
struct BarsChart: View {
    var rows: [BarRow]
    private var maxV: Double { Swift.max(rows.map(\.value).max() ?? 1, 0.000001) }

    var body: some View {
        VStack(spacing: 7) {
            ForEach(rows) { row in
                VStack(spacing: 3) {
                    HStack(spacing: 6) {
                        Text(row.label)
                            .font(.system(size: 12))
                            .foregroundStyle(Palette.text.opacity(0.82))
                            .lineLimit(1)
                        Spacer()
                        Text(row.caption)
                            .font(.system(size: 11.5, weight: .semibold, design: .monospaced))
                            .foregroundStyle(Palette.text)
                    }
                    GeometryReader { geo in
                        ZStack(alignment: .leading) {
                            Capsule().fill(Palette.track).frame(height: 6)
                            Capsule().fill(row.color)
                                .frame(width: Swift.max(6, geo.size.width * CGFloat(row.value / maxV)), height: 6)
                        }
                    }
                    .frame(height: 6)
                }
            }
        }
    }
}

/// A single proportional stacked bar (token composition, free/paid split).
struct CompositionBar: View {
    var segments: [DonutSlice]
    private var total: Double { segments.reduce(0) { $0 + $1.value } }

    var body: some View {
        GeometryReader { geo in
            HStack(spacing: 0) {
                ForEach(Array(segments.enumerated()), id: \.offset) { _, seg in
                    Rectangle()
                        .fill(seg.color)
                        .frame(width: total > 0 ? geo.size.width * CGFloat(seg.value / total) : 0)
                }
            }
        }
        .frame(height: 18)
        .background(Capsule().fill(Palette.track))
        .clipShape(Capsule())
    }
}

struct LegendView: View {
    var items: [LegendItem]

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            ForEach(items) { item in
                HStack(spacing: 8) {
                    RoundedRectangle(cornerRadius: 2).fill(item.color).frame(width: 9, height: 9)
                    Text(item.label).font(.system(size: 11.5)).foregroundStyle(Palette.text.opacity(0.82)).lineLimit(1)
                    Spacer(minLength: 6)
                    Text(item.value).font(.system(size: 11, weight: .semibold, design: .monospaced)).foregroundStyle(Palette.text)
                }
            }
            Spacer(minLength: 0)
        }
    }
}
