# Shared quota model

The two apps share no code (idiomatic Rust struct + Swift struct), but render the same normative model. This is the single spec.

```
Snapshot {
  provider_id:   "anthropic" | "xai" | "opencode-go"
  provider_name: string          // "Claude", "Grok", "opencode go"
  subtitle:      string          // "Max · 20×", "X Premium · live", "$10/mo · estimated locally"
  authority:     Live | Estimated | Unavailable
  source:        string          // "api.anthropic.com · live", "cli-chat-proxy.grok.com · live", "local opencode.db · estimate"
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

## Usage analytics model

The live `Snapshot` (rings) is now joined by a `Usage` value per provider, computed from local history ([data-sources.md](data-sources.md)). The dashboard renders both.

```
Dashboard {
  claude_quota:    Snapshot     // live rings
  claude_usage:    Usage        // from ~/.claude/projects transcripts
  grok_quota:      Snapshot     // live credits from cli-chat-proxy
  grok_usage:      Usage        // from ~/.grok/sessions activity (no token $)
  opencode_quota:  Snapshot     // estimated caps
  opencode_usage:  Usage        // from opencode.db, all providers
  generated_at:    timestamp
}

Usage {
  scope, source, authority
  totals:   { cost_usd, input, output, cache_write, cache_read, messages,
              sessions, active_days, web_search, web_fetch, first_day, last_day }
  windows:  { today, seven, thirty } of { cost, tokens, messages }   // calendar-day
  daily:    [DayPoint { date, cost, tokens, messages }]              // ascending
  by_model / by_project / by_provider: [Segment { label, cost, tokens, messages }]
  tokens:   { input, output, cache_write, cache_read, reasoning }    // composition
  heatmap:  counts[7][24] (weekday 0=Mon × hour) + max               // punch card
}
```

`Snapshot`/`Gauge` gained three fields the live endpoint exposes: `Gauge.api_severity` (the server's own severity, trusted over the fraction threshold), `Gauge.is_active` (the *binding* constraint — the limit that will stop the user first), and `Snapshot.spend` (prepaid/overflow-credit state).

- **Dollars are API-equivalent estimates**, tokens are exact — every `$` is under an EST badge.
- **`cost` breakdowns rank by tokens** (a metric every row has, priced or free) so bar lengths are comparable; the caption shows dollars where priced.

## Dashboard

A fullscreen-capable window (was a ~400px popover), everything on one screen, laid out by a responsive flow that reflows from ~1000px up to 4K. One layout engine drives both the live canvas and the PNG export, so a screenshot is pixel-for-pixel the live view.

- **Hero** — the binding limit (`is_active`) as the largest ring, severity-coloured, with its reset ETA. The first-200ms "am I about to hit a wall?" answer.
- **Sections** — Claude → Grok → opencode. Each provider gets live/estimated quota rings (and reset horizon where known), then a usage section. Claude has value-returned + token composition; Grok has activity-only history (turns/sessions — the CLI does not store per-turn tokens); opencode has caps + all-provider usage and a free-vs-paid donut.
- **Screenshot utility** — export the whole dashboard or a chosen subset (segments on KDE, sections on macOS) to a high-resolution PNG that is also copied to the clipboard.

## Distinct identities

One product, `tokenmaxxing`, in two platform builds:

| | KDE build | macOS build |
| --- | --- | --- |
| Platform | KDE Plasma 6 | macOS 26+ |
| Toolkit | Rust · GTK4 · libadwaita · Cairo | SwiftUI · Liquid Glass |
| Mark | lightning bolt in a current-arc | glass triangle refracting a spectrum |
| Palette | electric cyan `#00E5FF` · acid lime `#B6FF00` on `#0A0E14` | iridescent aqua · violet · pink on `#0B0B13` |
| Home | system tray | menu bar |
