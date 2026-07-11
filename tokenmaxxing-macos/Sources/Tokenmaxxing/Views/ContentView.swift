import SwiftUI
import AppKit

struct ContentView: View {
    @Environment(AppModel.self) private var model
    @Environment(Store.self) private var store

    private var scale: Double { store.uiScale }

    var body: some View {
        VStack(spacing: 0) {
            header
            content
            footer
        }
        .frame(width: 404 * scale, height: 664 * scale)
        .background(backdrop)
    }

    private var header: some View {
        HStack(spacing: 10 * scale) {
            TokenmaxxingMark(size: 28 * scale)
            VStack(alignment: .leading, spacing: 0) {
                Text("tokenmaxxing")
                    .font(.system(size: 16 * scale, weight: .bold, design: .rounded))
                    .foregroundStyle(Palette.text)
                Text("token quotas")
                    .font(.system(size: 10.5 * scale))
                    .foregroundStyle(Palette.muted)
            }
            Spacer()
            Button { model.refresh() } label: {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(.glass)
            settingsMenu
        }
        .padding(.horizontal, 14 * scale)
        .padding(.vertical, 11 * scale)
    }

    private var settingsMenu: some View {
        Menu {
            Picker("Interface scale", selection: scaleBinding) {
                ForEach(Store.scaleSteps.indices, id: \.self) { index in
                    Text("\(Int(Store.scaleSteps[index] * 100))%").tag(index)
                }
            }
            Button("Export share card…") { model.exportShareCard() }
            Button("Open opencode console") {
                if let url = URL(string: "https://opencode.ai/auth") {
                    NSWorkspace.shared.open(url)
                }
            }
            Toggle("Launch at login", isOn: launchBinding)
            Divider()
            Button("Quit Tokenmaxxing") { NSApplication.shared.terminate(nil) }
        } label: {
            Image(systemName: "slider.horizontal.3")
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    @ViewBuilder
    private var content: some View {
        if model.snapshots.isEmpty {
            VStack(spacing: 10) {
                ProgressView()
                Text("Reading quotas…")
                    .font(.system(size: 12 * scale))
                    .foregroundStyle(Palette.muted)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                VStack(spacing: 14 * scale) {
                    ForEach(model.snapshots) { snapshot in
                        ProviderCard(snapshot: snapshot, scale: scale)
                    }
                }
                .padding(14 * scale)
            }
        }
    }

    private var footer: some View {
        HStack {
            Text(model.updatedText)
                .font(.system(size: 10.5 * scale, design: .monospaced))
                .foregroundStyle(Palette.muted)
            Spacer()
        }
        .padding(.horizontal, 14 * scale)
        .padding(.bottom, 10 * scale)
        .padding(.top, 2)
    }

    private var backdrop: some View {
        ZStack {
            Palette.base
            LinearGradient(
                colors: [Palette.indigo.opacity(0.20), .clear, Palette.pink.opacity(0.14)],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        }
        .ignoresSafeArea()
    }

    private var scaleBinding: Binding<Int> {
        Binding(get: { store.scaleIndex() }, set: { store.uiScale = Store.scaleSteps[$0] })
    }

    private var launchBinding: Binding<Bool> {
        Binding(get: { model.launchAtLogin }, set: { model.setLaunchAtLogin($0) })
    }
}
