import SwiftUI

/// One provider's card: header, a grid of ring gauges (one per quota window),
/// details, and any note or error.
struct ProviderCard: View {
    var snapshot: Snapshot
    var scale: Double
    var glass: Bool = true

    private var accent: Color { Palette.accent(snapshot.providerId) }

    var body: some View {
        VStack(alignment: .leading, spacing: 10 * scale) {
            header
            if !snapshot.gauges.isEmpty {
                gaugeGrid
            }
            if let note = snapshot.note {
                Text(note)
                    .font(.system(size: 11 * scale))
                    .foregroundStyle(Palette.aqua.opacity(0.85))
                    .fixedSize(horizontal: false, vertical: true)
            }
            if !snapshot.details.isEmpty {
                Divider().overlay(Palette.track)
                ForEach(snapshot.details) { detail in
                    HStack {
                        Text(detail.key)
                            .font(.system(size: 11.5 * scale))
                            .foregroundStyle(Palette.muted)
                        Spacer()
                        Text(detail.value)
                            .font(.system(size: 12.5 * scale, weight: .semibold, design: .monospaced))
                            .foregroundStyle(Palette.text)
                    }
                }
            }
            if let error = snapshot.error {
                Text(error)
                    .font(.system(size: 11.5 * scale))
                    .foregroundStyle(Palette.rose)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(16 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .modifier(CardBackground(accent: accent, radius: 20 * scale, glass: glass))
    }

    private var header: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 2 * scale) {
                Text(snapshot.providerName)
                    .font(.system(size: 17 * scale, weight: .bold, design: .rounded))
                    .foregroundStyle(accent)
                Text(snapshot.subtitle)
                    .font(.system(size: 11.5 * scale))
                    .foregroundStyle(Palette.muted)
                Text(snapshot.source)
                    .font(.system(size: 10.5 * scale, design: .monospaced))
                    .foregroundStyle(Palette.muted.opacity(0.85))
            }
            Spacer()
            BadgePill(authority: snapshot.authority, scale: scale)
        }
    }

    private var gaugeGrid: some View {
        let columns = Array(
            repeating: GridItem(.flexible(), spacing: 4 * scale),
            count: min(snapshot.gauges.count, 3)
        )
        return LazyVGrid(columns: columns, spacing: 14 * scale) {
            ForEach(snapshot.gauges) { gauge in
                GaugeCell(gauge: gauge, accent: accent, scale: scale)
            }
        }
        .padding(.top, 4 * scale)
    }
}
