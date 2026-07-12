# tokenmaxxing — macOS build

Iridescent LLM-usage **dashboard** for **macOS**. SwiftUI · Liquid Glass · glass-card panels with Canvas-drawn charts.

![tokenmaxxing on macOS](../assets/tokenmaxxing-macos-sharecard.png)

## Requirements

macOS 26+ (Tahoe) and full Xcode 26 — Liquid Glass (`glassEffect`, `.buttonStyle(.glass)`), `MenuBarExtra`, and the `Window` scene need the macOS 26 SDK.

## Build & run

```sh
make run        # swift build → assemble Tokenmaxxing.app → ad-hoc codesign → open
make install    # same, into /Applications
```

It's a dock-less menu-bar agent (`LSUIElement`). The menu-bar item is a compact status summary and a launcher — **Open dashboard** opens the full, resizable, native-fullscreen dashboard window. The window's 📷 toolbar button opens a sheet to pick which sections to export.

The app ships **un-sandboxed on purpose** — the App Sandbox blocks reading `~/.claude` and `~/.local/share/opencode`, so this is distributed with Developer ID + notarization, not the Mac App Store. The build ad-hoc signs for local use.

## Screenshot / share export

The menu-bar **Export screenshot** action, and the dashboard's screenshot sheet, render the dashboard (or the chosen sections) to a high-resolution PNG via `ImageRenderer`, copy it to the clipboard, and reveal it in Finder (`~/Pictures`). Headless:

```sh
Tokenmaxxing --export [path.png]     # renders the whole dashboard to a PNG and exits
```

## Data

- **Claude** — live quota from `~/.claude/.credentials.json`, plus usage history from `~/.claude/projects/**/*.jsonl` (see [../docs/data-sources.md](../docs/data-sources.md)).
- **opencode** — estimated caps and all-provider history from this Mac's `~/.local/share/opencode/opencode.db` via the SQLite3 C API, read-only. Labeled **EST**.
