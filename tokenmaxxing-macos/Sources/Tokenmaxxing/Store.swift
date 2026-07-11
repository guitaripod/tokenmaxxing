import SwiftUI

/// Persisted user preferences. The interface scale multiplies every dimension
/// in the popover so a single control resizes the whole UI.
@MainActor
@Observable
final class Store {
    static let scaleSteps: [Double] = [1.0, 1.15, 1.3, 1.5, 1.75]

    var uiScale: Double {
        didSet { UserDefaults.standard.set(uiScale, forKey: "uiScale") }
    }

    init() {
        let stored = UserDefaults.standard.double(forKey: "uiScale")
        uiScale = stored == 0 ? 1.0 : stored
    }

    func scaleIndex() -> Int {
        Self.scaleSteps.firstIndex { abs($0 - uiScale) < 0.01 } ?? 0
    }
}
