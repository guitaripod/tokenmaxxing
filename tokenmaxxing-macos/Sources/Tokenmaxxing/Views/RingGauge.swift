import SwiftUI

/// A circular quota gauge — restrained stroke, sharp centre percent.
struct RingGauge: View {
    var gauge: Gauge
    var accent: Color
    var diameter: CGFloat

    private var color: Color { accent }
    private var stroke: CGFloat { max(3.2, diameter * 0.095) }

    var body: some View {
        ZStack {
            Circle()
                .stroke(Palette.track.opacity(0.9), style: StrokeStyle(lineWidth: stroke, lineCap: .round))
            Circle()
                .trim(from: 0, to: gauge.fraction)
                .stroke(color, style: StrokeStyle(lineWidth: stroke, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .shadow(color: color.opacity(0.28), radius: diameter * 0.035)
            Text(gauge.percentText)
                .font(.system(size: diameter * 0.23, weight: .bold, design: .rounded))
                .foregroundStyle(Palette.text)
                .minimumScaleFactor(0.7)
                .lineLimit(1)
        }
        .frame(width: diameter, height: diameter)
        .animation(.easeOut(duration: 0.45), value: gauge.fraction)
    }
}

/// One quota window: a ring plus its label and underlying values (dashboard).
struct GaugeCell: View {
    var gauge: Gauge
    var accent: Color
    var scale: Double

    var body: some View {
        VStack(spacing: 5 * scale) {
            ZStack(alignment: .topTrailing) {
                RingGauge(gauge: gauge, accent: Palette.gauge(accent, gauge.severity), diameter: 88 * scale)
                if gauge.isActive {
                    Circle()
                        .fill(Palette.rose)
                        .frame(width: 7 * scale, height: 7 * scale)
                        .offset(x: 2, y: -1)
                }
            }
            Text(gauge.label)
                .font(.system(size: 12 * scale, weight: .medium))
                .foregroundStyle(Palette.secondaryText)
                .multilineTextAlignment(.center)
                .lineLimit(2)
                .frame(minHeight: 28 * scale, alignment: .top)
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
            .font(.system(size: 9 * scale, weight: .bold, design: .rounded))
            .padding(.horizontal, 7 * scale)
            .padding(.vertical, 2.5 * scale)
            .background(Capsule().fill(Palette.badge(authority)))
            .foregroundStyle(Palette.badgeInk(authority))
    }
}

/// Glass in the live app, a solid card when rendered into a share PNG.
struct CardBackground: ViewModifier {
    var accent: Color
    var radius: CGFloat
    var glass: Bool
    @Environment(\.colorScheme) private var colorScheme

    func body(content: Content) -> some View {
        if glass {
            content.glassEffect(.regular.tint(accent.opacity(colorScheme == .dark ? 0.12 : 0.08)), in: .rect(cornerRadius: radius))
        } else {
            content
                .background(
                    RoundedRectangle(cornerRadius: radius, style: .continuous)
                        .fill(Palette.panel)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: radius, style: .continuous)
                        .stroke(Palette.cardStroke, lineWidth: 1)
                )
        }
    }
}
