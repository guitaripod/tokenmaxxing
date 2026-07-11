import SwiftUI

/// A circular quota gauge. It sits on top of a glass card — never glass itself.
struct RingGauge: View {
    var gauge: Gauge
    var accent: Color
    var diameter: CGFloat

    private var color: Color { Palette.gauge(accent, gauge.severity) }

    var body: some View {
        ZStack {
            Circle()
                .stroke(Palette.track, style: StrokeStyle(lineWidth: diameter * 0.11, lineCap: .round))
            Circle()
                .trim(from: 0, to: gauge.fraction)
                .stroke(color, style: StrokeStyle(lineWidth: diameter * 0.11, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .shadow(color: color.opacity(0.55), radius: diameter * 0.05)
            Text(gauge.percentText)
                .font(.system(size: diameter * 0.25, weight: .bold, design: .rounded))
                .foregroundStyle(Palette.text)
        }
        .frame(width: diameter, height: diameter)
        .animation(.easeOut(duration: 0.5), value: gauge.fraction)
    }
}

/// One quota window: a ring plus its label and underlying values.
struct GaugeCell: View {
    var gauge: Gauge
    var accent: Color
    var scale: Double

    var body: some View {
        VStack(spacing: 5 * scale) {
            RingGauge(gauge: gauge, accent: accent, diameter: 92 * scale)
            Text(gauge.label)
                .font(.system(size: 12.5 * scale, weight: .medium))
                .foregroundStyle(Palette.text.opacity(0.86))
                .multilineTextAlignment(.center)
                .lineLimit(2)
                .frame(minHeight: 30 * scale, alignment: .top)
            if let sub = gauge.subline {
                Text(sub)
                    .font(.system(size: 10.5 * scale, design: .monospaced))
                    .foregroundStyle(Palette.muted)
                    .multilineTextAlignment(.center)
                    .lineLimit(3)
            }
        }
        .frame(maxWidth: .infinity)
    }
}

struct BadgePill: View {
    var authority: Authority
    var scale: Double

    var body: some View {
        Text(authority.badge)
            .font(.system(size: 10.5 * scale, weight: .bold, design: .rounded))
            .padding(.horizontal, 9 * scale)
            .padding(.vertical, 3 * scale)
            .background(Capsule().fill(Palette.badge(authority)))
            .foregroundStyle(Palette.base)
    }
}

/// Glass in the live app, a solid card when rendered into a share PNG (glass is
/// a live effect that does not capture through ImageRenderer).
struct CardBackground: ViewModifier {
    var accent: Color
    var radius: CGFloat
    var glass: Bool

    func body(content: Content) -> some View {
        if glass {
            content.glassEffect(.regular.tint(accent.opacity(0.12)), in: .rect(cornerRadius: radius))
        } else {
            content
                .background(RoundedRectangle(cornerRadius: radius).fill(Color(red: 0.075, green: 0.101, blue: 0.157)))
                .overlay(RoundedRectangle(cornerRadius: radius).stroke(Palette.track, lineWidth: 1))
        }
    }
}
