# tokenmaxxing

<p align="left">
  <img src="assets/icon.png" width="112" alt="tokenmaxxing — KDE" />
  &nbsp;
  <img src="assets/icon-macos.png" width="112" alt="tokenmaxxing — macOS" />
</p>

Native desktop **usage dashboards** for your LLM subscriptions — live quota, ten months of local history, and a value-for-money readout, all on one fullscreen-capable screen. One product, two platform builds with deliberately distinct identities — electric on KDE, iridescent glass on macOS.

| **KDE build** (Rust · GTK4 · libadwaita) | **macOS build** (SwiftUI · Liquid Glass) |
| --- | --- |
| Electric / terminal. Lives in the system tray. | Iridescent glass. Lives in the menu bar. |
| ![tokenmaxxing on KDE](assets/tokenmaxxing-kde-sharecard.png) | ![tokenmaxxing on macOS](assets/tokenmaxxing-macos-sharecard.png) |

Both render the same quota model for two subscriptions:

- **Claude** (Anthropic Max / Pro) — **live**, from the same OAuth usage endpoint Claude Code's `/usage` uses.
- **opencode go** (OpenCode's $10/mo plan) — **estimated locally** (see the honesty note below).

Each quota *window* gets its own ring gauge (5-hour session, weekly, per-model weekly, rolling spend caps…), colored by headroom — accent while healthy, amber past 60%, hot past 85%.

## The opencode-go honesty note

OpenCode Go's quota is a set of **rolling dollar spend caps** (~$12 / 5h, ~$30 / week, ~$60 / month) enforced server-side. There is **no public API** to read the remaining amount — it's only visible in the web console at [opencode.ai/auth](https://opencode.ai/auth) behind a GitHub login.

So both builds **estimate** it by summing this machine's spend from the local `opencode.db` against those caps, and label the card **EST** with a plain-language disclaimer. The estimate can under-count usage from other machines and won't match server-side accounting exactly. Claude's numbers, by contrast, are genuinely live. See [docs/data-sources.md](docs/data-sources.md) for the full breakdown, including how a live opencode reading could be wired if the account token is provided.

## Features (both builds)

- **Live quota, one ring per window** — 5-hour session, weekly, per-model weekly, overflow credits — coloured by headroom and by the API's *own* severity, with the **binding constraint** called out as the hero.
- **Reset horizon** — every upcoming reset across both providers on one soonest-first timeline (trusted resets filled, estimated hollow).
- **Full usage history** from local files — daily cost/token area charts, per-model / per-project / per-provider breakdowns, token composition, cache hit rate, and an hour-of-week activity heatmap, over ~10 months.
- **Value returned** — what your subscription would have cost on the metered API (an estimate; the tokens are exact), plus a burn-rate/month projection.
- **Screenshot utility** — export the whole dashboard *or* chosen segments to a high-resolution PNG, also copied to the clipboard (and headless via `--export`).
- **Fullscreen-capable dashboard window** that reflows from ~1000px to 4K, resident in the tray / menu bar.

## Layout

```
tokenmaxxing/
├── tokenmaxxing-kde/     Rust GTK4 build for KDE Plasma 6
├── tokenmaxxing-macos/   SwiftUI menu-bar build for macOS 26+
├── docs/                 data-sources.md, model.md
└── assets/               share-card renders
```

## Build

- **KDE** — [`tokenmaxxing-kde/README.md`](tokenmaxxing-kde/README.md) (`cargo build --release`)
- **macOS** — [`tokenmaxxing-macos/README.md`](tokenmaxxing-macos/README.md) (`make run`, needs macOS 26 + Xcode 26)

## License

GPL-3.0-or-later — see [LICENSE](LICENSE).
