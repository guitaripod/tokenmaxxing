import Foundation

/// Single source of the marketing version. The bundle value wins when running
/// from the .app; the fallback covers headless runs of the bare SPM binary and
/// must be kept in lockstep with Resources/Info.plist.
enum AppVersion {
    static let current = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.2.1"
}
