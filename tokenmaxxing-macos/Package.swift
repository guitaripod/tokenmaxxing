// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Tokenmaxxing",
    platforms: [.macOS("26.0")],
    targets: [
        .executableTarget(
            name: "Tokenmaxxing",
            path: "Sources/Tokenmaxxing",
            linkerSettings: [.linkedLibrary("sqlite3")]
        )
    ]
)
