# tokenmaxxing — macOS build

LLM-usage **dashboard** for **macOS**. SwiftUI · Liquid Glass · glass-card panels with Canvas-drawn charts.

![tokenmaxxing on macOS](../assets/tokenmaxxing-macos-sharecard.png)

## Requirements

macOS 26+ (Tahoe) and full Xcode 26 — Liquid Glass (`glassEffect`, `.buttonStyle(.glass)`), `MenuBarExtra`, and the `Window` scene need the macOS 26 SDK.

## Build & run

```sh
make run        # swift build → assemble Tokenmaxxing.app → ad-hoc codesign → open
make install    # same, into /Applications
```

It's a dock-less menu-bar agent (`LSUIElement`). The menu-bar item is a compact status summary (Claude → Grok → opencode) and a launcher — **Open dashboard** opens the full, resizable, native-fullscreen dashboard window. Light/dark follows system appearance. Brand accents match Anthropic / xAI / opencode.

The app ships **un-sandboxed on purpose** — the App Sandbox blocks reading `~/.claude`, `~/.grok`, and `~/.local/share/opencode`, so this is distributed with Developer ID + notarization, not the Mac App Store. The build ad-hoc signs for local use.

## Screenshot / share export

The menu-bar **Export screenshot** action, and the dashboard's screenshot sheet, render the dashboard (or the chosen sections) to a high-resolution PNG via `ImageRenderer`, copy it to the clipboard (PNG + image), and reveal it in Finder (`~/Pictures`). Exports carry a subtle `tokenmaxxing <version> · github.com/guitaripod/tokenmaxxing` credit. Headless:

```sh
Tokenmaxxing --export [path.png]     # renders the whole dashboard to a PNG and exits
```

## Data

- **Claude** — live quota from `~/.claude/.credentials.json`, plus usage history from `~/.claude/projects/**/*.jsonl`. Last-good usage cached under `~/Library/Application Support/tokenmaxxing/` for 429 resilience (see [../docs/data-sources.md](../docs/data-sources.md)).
- **Grok** — live credits from `~/.grok/auth.json` via `cli-chat-proxy.grok.com/v1/billing`, plus activity history from `~/.grok/sessions`.
- **opencode** — estimated caps and all-provider history from this Mac's `~/.local/share/opencode/opencode.db` via the SQLite3 C API, read-only. Labeled **EST**.
