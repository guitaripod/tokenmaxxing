# tokenmaxxing — KDE build

Electric LLM-quota meter for the **KDE Plasma** system tray. Rust · GTK4 · libadwaita.

![tokenmaxxing on KDE](../assets/tokenmaxxing-kde-sharecard.png)

## Requirements

System libraries (Arch package names): `gtk4` (≥ 4.12), `libadwaita`, plus a Rust toolchain. The tray needs a StatusNotifierItem host — standard on Plasma 6.

## Build & run

```sh
cargo build --release
./target/release/tokenmaxxing
```

It opens a compact window and installs a tray icon (a bolt-in-arc). Left-click the tray toggles the window; right-click gives Refresh / Export / Open opencode console / Quit. Closing the window hides it to the tray.

## Headless share card

```sh
tokenmaxxing --export [path.png]   # renders the quota state to a PNG and exits
```

Without a path it writes to `$XDG_PICTURES_DIR` (or `~/Pictures`).

## Data

- **Claude** — live from `~/.claude/.credentials.json` (see [../docs/data-sources.md](../docs/data-sources.md)).
- **opencode go** — estimated from `~/.local/share/opencode/opencode.db`, opened read-only. Labeled **EST**.

## Config

`~/.config/tokenmaxxing/config.json` — `{ "ui_scale": 1.25 }`. Set from the ☰ menu (100%–200%); the whole UI rescales live.
