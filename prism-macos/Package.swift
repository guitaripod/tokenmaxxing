// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Prism",
    platforms: [.macOS("26.0")],
    targets: [
        .executableTarget(
            name: "Prism",
            path: "Sources/Prism",
            linkerSettings: [.linkedLibrary("sqlite3")]
        )
    ]
)
