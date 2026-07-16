import SwiftUI
import AppKit

/// Brand-matched palette:
/// - Claude → Anthropic terracotta `#D97757`
/// - Grok → xAI monochrome (silver on dark, ink on light)
/// - opencode → product green `#03B000`
/// Surfaces follow Anthropic's warm ink paper system for cohesion.
enum Palette {
    static let indigo = Color(red: 0.42, green: 0.44, blue: 0.86)
    /// Claude / Anthropic `#D97757`
    static let aqua = Color(red: 0.851, green: 0.467, blue: 0.341)
    /// Grok / xAI monochrome — silver on dark, ink on light.
    static let violet = Color(
        light: Color(red: 0.12, green: 0.12, blue: 0.12),
        dark: Color(red: 0.910, green: 0.910, blue: 0.920)
    )
    /// Critical
    static let pink = Color(red: 0.88, green: 0.28, blue: 0.32)
    static let amber = Color(red: 0.93, green: 0.68, blue: 0.22)
    static let rose = Color(red: 0.88, green: 0.28, blue: 0.32)
    static let teal = Color(red: 0.30, green: 0.70, blue: 0.62)
    /// Anthropic supporting blue
    static let azure = Color(red: 0.42, green: 0.61, blue: 0.86)
    /// opencode `#03B000`
    static let lime = Color(red: 0.012, green: 0.690, blue: 0.000)
    static let orange = Color(red: 0.90, green: 0.50, blue: 0.28)

    static let text = Color.primary
    static let muted = Color.secondary
    static let track = Color(
        light: Color(red: 0.88, green: 0.87, blue: 0.84),
        dark: Color(red: 0.22, green: 0.22, blue: 0.21)
    )
    static let panel = Color(
        light: Color.white,
        dark: Color(red: 0.118, green: 0.118, blue: 0.114)
    )
    static let canvas = Color(
        light: Color(red: 0.980, green: 0.976, blue: 0.961), // #FAF9F5
        dark: Color(red: 0.078, green: 0.078, blue: 0.075)  // #141413
    )
    static let base = canvas
    static let onBadge = Color.white
    static let cardStroke = Color(
        light: Color(red: 0.86, green: 0.85, blue: 0.82),
        dark: Color(red: 0.20, green: 0.20, blue: 0.19)
    )
    static let secondaryText = Color(
        light: Color(red: 0.30, green: 0.29, blue: 0.27),
        dark: Color(red: 0.82, green: 0.81, blue: 0.78)
    )

    static let spectrum = [aqua, violet, indigo, lime]
    static let ramp: [Color] = [aqua, violet, lime, amber, azure, teal, pink, orange]

    static func series(_ index: Int) -> Color {
        ramp[((index % ramp.count) + ramp.count) % ramp.count]
    }

    static let tokenInput = aqua
    static let tokenOutput = azure
    static let tokenCacheWrite = teal
    static let tokenCacheRead = lime
    static let tokenReasoning = amber

    static func accent(_ providerId: String) -> Color {
        switch providerId {
        case "anthropic": aqua
        case "xai": violet
        default: lime
        }
    }

    static func planMonthlyUSD(_ subtitle: String) -> Double? {
        let s = subtitle.lowercased()
        if s.contains("max") { return s.contains("20") ? 200 : 100 }
        if s.contains("pro") { return 20 }
        return nil
    }

    static func gauge(_ accent: Color, _ severity: Severity) -> Color {
        switch severity {
        case .nominal: accent
        case .warn: amber
        case .critical: rose
        }
    }

    /// High-contrast solid status pills.
    static func badge(_ authority: Authority) -> Color {
        switch authority {
        case .live: Color(red: 0.12, green: 0.55, blue: 0.42)
        case .estimated: Color(red: 0.42, green: 0.40, blue: 0.48)
        case .unavailable: Color(red: 0.72, green: 0.24, blue: 0.28)
        }
    }

    static func badgeInk(_ authority: Authority) -> Color {
        .white
    }
}

extension Color {
    init(light: Color, dark: Color) {
        self.init(nsColor: NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            return NSColor(isDark ? dark : light)
        })
    }
}

struct TokenmaxxingMark: View {
    var size: CGFloat

    var body: some View {
        Canvas { context, canvasSize in
            let s = min(canvasSize.width, canvasSize.height)
            let center = CGPoint(x: s * 0.5, y: s * 0.5)
            let radius = s * 0.34
            let width = s * 0.12

            var ring = Path()
            ring.addArc(
                center: center, radius: radius,
                startAngle: .degrees(-90), endAngle: .degrees(-90 + 302),
                clockwise: false
            )
            context.stroke(
                ring,
                with: .color(Palette.aqua),
                style: StrokeStyle(lineWidth: width, lineCap: .round)
            )

            context.fill(
                Self.boltPath(center: center, box: s * 0.52),
                with: .color(Color.primary)
            )
        }
        .frame(width: size, height: size)
    }

    static func boltPath(center: CGPoint, box: CGFloat) -> Path {
        let ox = center.x - box * 0.52
        let oy = center.y - box * 0.51
        let points: [(CGFloat, CGFloat)] = [
            (0.585, 0.12), (0.34, 0.55), (0.5, 0.55),
            (0.415, 0.9), (0.7, 0.43), (0.52, 0.43),
        ]
        var path = Path()
        path.move(to: CGPoint(x: points[0].0 * box + ox, y: points[0].1 * box + oy))
        for point in points.dropFirst() {
            path.addLine(to: CGPoint(x: point.0 * box + ox, y: point.1 * box + oy))
        }
        path.closeSubpath()
        return path
    }
}

struct AppIconView: View {
    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 1024 * 0.225, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: [
                            Color(red: 0.12, green: 0.12, blue: 0.11),
                            Color(red: 0.05, green: 0.05, blue: 0.05),
                        ],
                        startPoint: .topLeading, endPoint: .bottom
                    )
                )
            RadialGradient(
                colors: [Palette.aqua.opacity(0.20), .clear],
                center: .center, startRadius: 40, endRadius: 580
            )
            TokenmaxxingMark(size: 760)
        }
        .frame(width: 1024, height: 1024)
    }
}
