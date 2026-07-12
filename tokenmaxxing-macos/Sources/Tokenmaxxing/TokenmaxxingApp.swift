import SwiftUI

struct TokenmaxxingApp: App {
    @State private var store = Store()
    @State private var model = AppModel()

    var body: some Scene {
        // The full dashboard lives in a resizable, fullscreen-capable window —
        // opened on demand from the menu bar, not auto-presented at launch.
        Window("tokenmaxxing", id: "dashboard") {
            DashboardView()
                .environment(store)
                .environment(model)
                .frame(minWidth: 900, minHeight: 600)
                .task { model.start() }
        }
        .defaultSize(width: 1360, height: 900)
        .defaultLaunchBehavior(.suppressed)

        // The menu-bar item is the always-there launcher and status summary.
        MenuBarExtra {
            ContentView()
                .environment(store)
                .environment(model)
                .task { model.start() }
        } label: {
            Image(systemName: "bolt.fill")
        }
        .menuBarExtraStyle(.window)
    }
}
