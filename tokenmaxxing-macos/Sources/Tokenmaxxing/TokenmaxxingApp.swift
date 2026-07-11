import SwiftUI

struct TokenmaxxingApp: App {
    @State private var store = Store()
    @State private var model = AppModel()

    var body: some Scene {
        MenuBarExtra {
            ContentView()
                .environment(store)
                .environment(model)
                .task { model.start() }
        } label: {
            Image(systemName: "cube.transparent")
        }
        .menuBarExtraStyle(.window)
    }
}
