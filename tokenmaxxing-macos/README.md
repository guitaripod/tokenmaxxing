# tokenmaxxing — macOS build

Iridescent LLM-quota meter for the **macOS** menu bar. SwiftUI · Liquid Glass.

![tokenmaxxing on macOS](../assets/tokenmaxxing-macos-sharecard.png)

## Requirements

macOS 26+ (Tahoe) and full Xcode 26 — Liquid Glass (`glassEffect`, `GlassEffectContainer`, `.buttonStyle(.glass)`) and `MenuBarExtra` need the macOS 26 SDK.

## Build & run

```sh
make run        # swift build → assemble Tokenmaxxing.app → ad-hoc codesign → open
make install    # same, into /Applications
```

It's a dock-less menu-bar agent (`LSUIElement`). Click the glass-cube icon to open the popover; the slider menu has the interface-scale picker, share-card export, launch-at-login, open console, and quit.

The app ships **un-sandboxed on purpose** — the App Sandbox blocks reading `~/.claude` and `~/.local/share/opencode`, so this is distributed with Developer ID + notarization, not the Mac App Store. The build ad-hoc signs for local use.

## Headless share card

```sh
Tokenmaxxing --export [path.png]     # renders the quota state to a PNG and exits
```

The in-app "Export share card…" also copies the PNG to the clipboard and reveals it in Finder (writes to `~/Pictures`).

## Data

- **Claude** — live from `~/.claude/.credentials.json` (see [../docs/data-sources.md](../docs/data-sources.md)).
- **opencode go** — estimated from this Mac's `~/.local/share/opencode/opencode.db` via the SQLite3 C API, read-only. Labeled **EST**.
