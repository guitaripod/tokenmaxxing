import SwiftUI

/// Tokenmaxxing's iridescent identity — deliberately the glassy opposite of the KDE build's
/// electric terminal palette.
enum Palette {
    static let base = Color(red: 0.043, green: 0.043, blue: 0.075)
    static let indigo = Color(red: 0.39, green: 0.40, blue: 0.95)
    static let aqua = Color(red: 0.13, green: 0.83, blue: 0.93)
    static let violet = Color(red: 0.66, green: 0.55, blue: 0.98)
    static let pink = Color(red: 0.93, green: 0.28, blue: 0.60)
    static let amber = Color(red: 0.98, green: 0.65, blue: 0.14)
    static let rose = Color(red: 0.98, green: 0.44, blue: 0.52)
    static let text = Color(red: 0.91, green: 0.93, blue: 0.98)
    static let muted = Color(red: 0.56, green: 0.60, blue: 0.70)
    static let track = Color(red: 0.16, green: 0.17, blue: 0.24)
    static let panel = Color(red: 0.075, green: 0.086, blue: 0.145)
    static let teal = Color(red: 0.18, green: 0.85, blue: 0.75)
    static let azure = Color(red: 0.30, green: 0.56, blue: 0.98)
    static let lime = Color(red: 0.70, green: 0.95, blue: 0.42)
    static let orange = Color(red: 1.0, green: 0.55, blue: 0.35)

    /// The full refraction spectrum, used for the mark and window backdrop.
    static let spectrum = [pink, violet, indigo, aqua]

    /// An iridescent categorical ramp for series (models, providers). Wraps.
    static let ramp: [Color] = [aqua, violet, pink, amber, teal, azure, lime, orange]

    static func series(_ index: Int) -> Color {
        ramp[((index % ramp.count) + ramp.count) % ramp.count]
    }

    /// Colors for the token tiers, in composition order.
    static let tokenInput = aqua
    static let tokenOutput = violet
    static let tokenCacheWrite = pink
    static let tokenCacheRead = azure
    static let tokenReasoning = amber

    static func accent(_ providerId: String) -> Color {
        providerId == "anthropic" ? aqua : violet
    }

    /// Best-effort monthly list price for the plan named in the quota subtitle,
    /// so the value-returned tile can show a rough return multiple.
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

    static func badge(_ authority: Authority) -> Color {
        switch authority {
        case .live: aqua
        case .estimated: violet
        case .unavailable: rose
        }
    }
}

/// The Tokenmaxxing mark: a glass triangle refracting a white ray into the spectrum.
/// The tokenmaxxing mark: a bolt cradled by a ¾-swept ring gauge, in the
/// iridescent colorway. Shares the motif of the KDE build's electric mark.
struct TokenmaxxingMark: View {
    var size: CGFloat

    var body: some View {
        Canvas { context, canvasSize in
            let s = min(canvasSize.width, canvasSize.height)
            let center = CGPoint(x: s * 0.5, y: s * 0.5)
            let radius = s * 0.34
            let width = s * 0.13

            var ring = Path()
            ring.addArc(
                center: center, radius: radius,
                startAngle: .degrees(-90), endAngle: .degrees(-90 + 302),
                clockwise: false
            )
            context.stroke(
                ring,
                with: .linearGradient(
                    Gradient(colors: [Palette.aqua, Palette.violet, Palette.pink]),
                    startPoint: CGPoint(x: center.x - radius, y: center.y - radius),
                    endPoint: CGPoint(x: center.x + radius, y: center.y + radius)
                ),
                style: StrokeStyle(lineWidth: width, lineCap: .round)
            )

            context.fill(
                Self.boltPath(center: center, box: s * 0.52),
                with: .linearGradient(
                    Gradient(colors: [.white, Palette.aqua]),
                    startPoint: CGPoint(x: center.x, y: center.y - radius),
                    endPoint: CGPoint(x: center.x, y: center.y + radius)
                )
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

/// The full app icon: a squircle with a glow behind the mark. Rendered to a PNG
/// for the .icns via `Tokenmaxxing --icon`.
struct AppIconView: View {
    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 1024 * 0.225, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: [Color(red: 0.063, green: 0.086, blue: 0.125),
                                 Color(red: 0.016, green: 0.027, blue: 0.043)],
                        startPoint: .topLeading, endPoint: .bottom
                    )
                )
            RadialGradient(
                colors: [Palette.violet.opacity(0.30), Palette.aqua.opacity(0.10), .clear],
                center: .center, startRadius: 40, endRadius: 580
            )
            TokenmaxxingMark(size: 760)
        }
        .frame(width: 1024, height: 1024)
    }
}
