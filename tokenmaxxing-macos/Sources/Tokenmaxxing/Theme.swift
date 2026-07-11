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

    /// The full refraction spectrum, used for the mark and window backdrop.
    static let spectrum = [pink, violet, indigo, aqua]

    static func accent(_ providerId: String) -> Color {
        providerId == "anthropic" ? indigo : violet
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
struct TokenmaxxingMark: View {
    var size: CGFloat

    var body: some View {
        Canvas { context, canvasSize in
            let s = min(canvasSize.width, canvasSize.height)
            let triangle = Path { path in
                path.move(to: CGPoint(x: s * 0.5, y: s * 0.14))
                path.addLine(to: CGPoint(x: s * 0.86, y: s * 0.82))
                path.addLine(to: CGPoint(x: s * 0.14, y: s * 0.82))
                path.closeSubpath()
            }
            context.stroke(
                triangle,
                with: .linearGradient(
                    Gradient(colors: [Palette.aqua, Palette.violet, Palette.pink]),
                    startPoint: .zero,
                    endPoint: CGPoint(x: s, y: s)
                ),
                lineWidth: s * 0.07
            )
            context.stroke(
                Path { p in
                    p.move(to: CGPoint(x: 0, y: s * 0.5))
                    p.addLine(to: CGPoint(x: s * 0.46, y: s * 0.5))
                },
                with: .color(Palette.text),
                lineWidth: s * 0.06
            )
            for (index, color) in Palette.spectrum.enumerated() {
                let spread = (CGFloat(index) - 1.5) * s * 0.11
                context.stroke(
                    Path { p in
                        p.move(to: CGPoint(x: s * 0.62, y: s * 0.5))
                        p.addLine(to: CGPoint(x: s, y: s * 0.5 + spread))
                    },
                    with: .color(color),
                    lineWidth: s * 0.045
                )
            }
        }
        .frame(width: size, height: size)
    }
}
