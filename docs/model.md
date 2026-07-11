# Shared quota model

The two apps share no code (idiomatic Rust struct + Swift struct), but render the same normative model. This is the single spec.

```
Snapshot {
  provider_id:   "anthropic" | "opencode-go"
  provider_name: string          // "Claude", "opencode go"
  subtitle:      string          // "Max · 20×", "$10/mo · estimated locally"
  authority:     Live | Estimated | Unavailable
  source:        string          // "api.anthropic.com · live", "local opencode.db · estimate"
  gauges:        [Gauge]         // one per quota window
  details:       [(key, value)]  // spec sheet: tokens, sessions, resets, plan…
  note:          string?         // the opencode-go estimate disclaimer
  error:         string?         // set when authority = Unavailable
}

Gauge {
  key:           string          // "session" | "weekly_all" | "weekly_scoped" | "5h" | "7d" | "30d" …
  label:         string          // "5-hour session", "Weekly · Fable", "Monthly rolling"
  fraction:      float 0..1       // the only value every gauge is guaranteed to have
  used, limit:   float?           // dollars, when known
  unit:          Percent | Usd
  detail:        string?          // "842 req"
  resets_at:     timestamp?       // null when unknown/untrusted
  trusted_reset: bool             // false for weekly + all opencode windows
}
```

## Rendering contract

- **`fraction` is canonical.** `used` / `limit` / `resets_at` are optional enrichments.
- **One ring per gauge.** Never collapse a provider to a single number.
- **Severity from headroom, shared thresholds:** `< 0.60` nominal (provider accent), `0.60–0.85` warn (amber), `> 0.85` critical (magenta / rose).
- **Reset trust:** a session reset is trustworthy; weekly resets and all opencode windows are shown with a `~` prefix or omitted, never used for logic.
- **State is always explicit:** the `authority` badge (LIVE / EST / OFFLINE) and `source` line say where every number came from.

## Distinct identities

| | Ampere | Prism |
| --- | --- | --- |
| Platform | KDE Plasma 6 | macOS 26+ |
| Toolkit | Rust · GTK4 · libadwaita · Cairo | SwiftUI · Liquid Glass |
| Mark | lightning bolt in a current-arc | glass triangle refracting a spectrum |
| Palette | electric cyan `#00E5FF` · acid lime `#B6FF00` on `#0A0E14` | iridescent aqua · violet · pink on `#0B0B13` |
| Home | system tray | menu bar |
