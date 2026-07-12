# tokenmaxxing — KDE build

Electric LLM-usage **dashboard** for **KDE Plasma**. Rust · GTK4 · libadwaita · Cairo. The whole dashboard is drawn on one Cairo canvas, so a screenshot is pixel-for-pixel the live view.

![tokenmaxxing on KDE](../assets/tokenmaxxing-kde-sharecard.png)

## Requirements

System libraries (Arch package names): `gtk4` (≥ 4.12), `libadwaita`, plus a Rust toolchain. The tray needs a StatusNotifierItem host — standard on Plasma 6. A `rust-toolchain.toml` pins **stable** (a recent nightly's `libsqlite3-sys` needs an unstable `cfg_select!`).

## Build & run

```sh
cargo build --release
./target/release/tokenmaxxing
```

It opens a resizable, fullscreen-capable dashboard window and installs a tray icon (a bolt-in-arc). The header bar has refresh, screenshot, fullscreen, and a ☰ menu (interface scale, export, open console); the tray left-click toggles the window. Closing hides it to the tray.

## Screenshot / share export

The 📷 header button enters screenshot mode — click panels to include, then **Export selected**, or **Export everything**. Either writes a high-resolution PNG (to `$XDG_PICTURES_DIR` / `~/Pictures`) and copies it to the clipboard. Headless:

```sh
tokenmaxxing --export [path.png]   # renders the whole dashboard to a PNG and exits
```

## Data

- **Claude** — live quota from `~/.claude/.credentials.json`, plus usage history from `~/.claude/projects/**/*.jsonl` (see [../docs/data-sources.md](../docs/data-sources.md)).
- **opencode** — estimated caps and all-provider history from `~/.local/share/opencode/opencode.db`, opened read-only. Labeled **EST**.

## Config

`~/.config/tokenmaxxing/config.json` — `{ "ui_scale": 1.25 }`. Set from the ☰ menu (100%–200%); the canvas rescales live.
