//! Per-model API pricing, used to value local token history in API-equivalent
//! dollars. The Claude Max/Pro subscription bills a flat fee, so this is not
//! what the user pays — it is the retail cost the same tokens would incur on the
//! metered API, i.e. the value the subscription returns. Always presented as an
//! estimate.

/// USD per million tokens for one model, plus the cache-tier multipliers the
/// Anthropic API applies against the base input rate.
#[derive(Clone, Copy)]
pub struct Rate {
    pub input: f64,
    pub output: f64,
}

impl Rate {
    /// 5-minute cache writes bill at 1.25× input, 1-hour at 2×, reads at 0.1×.
    const CACHE_WRITE_5M: f64 = 1.25;
    const CACHE_WRITE_1H: f64 = 2.0;
    const CACHE_READ: f64 = 0.1;

    /// API-equivalent dollar cost of one message's token usage.
    pub fn cost(&self, t: &TokenCounts) -> f64 {
        let per = |tokens: u64, rate: f64| tokens as f64 / 1_000_000.0 * rate;
        per(t.input, self.input)
            + per(t.output, self.output)
            + per(t.cache_write_5m, self.input * Self::CACHE_WRITE_5M)
            + per(t.cache_write_1h, self.input * Self::CACHE_WRITE_1H)
            + per(t.cache_read, self.input * Self::CACHE_READ)
    }
}

/// The token tiers that price differently. `cache_write_5m` / `cache_write_1h`
/// split the single `cache_creation_input_tokens` field when the finer
/// `cache_creation.ephemeral_*` breakdown is present; otherwise all cache-write
/// tokens fall into the 5-minute tier.
#[derive(Clone, Copy, Default)]
pub struct TokenCounts {
    pub input: u64,
    pub output: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
}

/// Resolve a model id to its rate. Matches Claude ids loosely (`claude-opus-*`,
/// `claude-fable-*`, …) so future point releases keep pricing, and falls back to
/// the Opus tier for unrecognized Claude models rather than dropping the row.
pub fn rate_for(model: &str) -> Rate {
    let m = model.to_ascii_lowercase();
    let is = |needle: &str| m.contains(needle);
    if is("fable") || is("mythos") {
        Rate { input: 10.0, output: 50.0 }
    } else if is("haiku") {
        Rate { input: 1.0, output: 5.0 }
    } else if is("sonnet") {
        Rate { input: 3.0, output: 15.0 }
    } else if is("opus") || is("claude") {
        Rate { input: 5.0, output: 25.0 }
    } else {
        Rate { input: 5.0, output: 25.0 }
    }
}

/// A friendly display name for a Claude model id, for legends and tables.
pub fn short_name(model: &str) -> String {
    let m = model.to_ascii_lowercase();
    if m.contains("opus-4-8") {
        "Opus 4.8".into()
    } else if m.contains("opus") {
        "Opus".into()
    } else if m.contains("fable") {
        "Fable 5".into()
    } else if m.contains("mythos") {
        "Mythos 5".into()
    } else if m.contains("sonnet") {
        "Sonnet".into()
    } else if m.contains("haiku") {
        "Haiku".into()
    } else if m == "<synthetic>" {
        "synthetic".into()
    } else {
        model.to_string()
    }
}
