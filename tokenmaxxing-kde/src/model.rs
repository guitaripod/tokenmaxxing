// This module is the normative shared model (mirrored in the macOS build's
// Swift). Some fields — provider ids, spend balance, `scope`, `key` — are
// carried for the spec and read by one surface or the other, not every one by
// the KDE canvas, so dead-code analysis over this single binary is too strict.
#![allow(dead_code)]

use chrono::{DateTime, Local, NaiveDate, Utc};

/// How much to trust the numbers on a provider card.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Authority {
    Live,
    Estimated,
    #[default]
    Unavailable,
}

impl Authority {
    pub fn badge(self) -> &'static str {
        match self {
            Authority::Live => "LIVE",
            Authority::Estimated => "EST",
            Authority::Unavailable => "OFFLINE",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Nominal,
    Warn,
    Critical,
}

impl Severity {
    pub fn from_fraction(f: f64) -> Self {
        if f >= 0.85 {
            Severity::Critical
        } else if f >= 0.60 {
            Severity::Warn
        } else {
            Severity::Nominal
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unit {
    Percent,
    Usd,
}

/// A single quota window (one ring): a fraction used of some limit.
#[derive(Clone, Debug)]
pub struct Gauge {
    pub key: String,
    pub label: String,
    pub fraction: f64,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub unit: Unit,
    pub detail: Option<String>,
    pub resets_at: Option<DateTime<Utc>>,
    pub trusted_reset: bool,
    /// The server's own severity for this window, when it reports one. Trusted
    /// over the fraction-derived threshold because the server knows the true
    /// shape of the limit (soft caps, grace, etc.).
    pub api_severity: Option<Severity>,
    /// True for the limit the server marks as the currently binding constraint
    /// — the one that will actually stop the user first.
    pub is_active: bool,
}

impl Gauge {
    /// Prefer the server's severity; fall back to the shared fraction thresholds.
    pub fn severity(&self) -> Severity {
        self.api_severity.unwrap_or_else(|| Severity::from_fraction(self.fraction))
    }

    pub fn percent_text(&self) -> String {
        format!("{}%", (self.fraction * 100.0).round() as i64)
    }
}

impl Default for Gauge {
    fn default() -> Self {
        Gauge {
            key: String::new(),
            label: String::new(),
            fraction: 0.0,
            used: None,
            limit: None,
            unit: Unit::Percent,
            detail: None,
            resets_at: None,
            trusted_reset: false,
            api_severity: None,
            is_active: false,
        }
    }
}

/// One provider's full quota picture.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub provider_id: String,
    pub provider_name: String,
    pub subtitle: String,
    pub authority: Authority,
    pub source: String,
    pub gauges: Vec<Gauge>,
    pub details: Vec<(String, String)>,
    pub note: Option<String>,
    pub error: Option<String>,
    /// Pay-as-you-go / prepaid credit state, when the plan exposes it.
    pub spend: Option<SpendInfo>,
}

impl Snapshot {
    /// The gauge the server marks active, else the most-utilized one — the
    /// window a headline "closest limit" callout should point at.
    pub fn binding_gauge(&self) -> Option<&Gauge> {
        self.gauges
            .iter()
            .find(|g| g.is_active)
            .or_else(|| self.gauges.iter().max_by(|a, b| a.fraction.total_cmp(&b.fraction)))
    }
}

/// Overflow-credit state from the Claude usage endpoint's `spend` block.
#[derive(Clone, Debug, Default)]
pub struct SpendInfo {
    pub enabled: bool,
    pub used: f64,
    pub limit: Option<f64>,
    pub balance: Option<f64>,
    pub can_purchase: bool,
    pub disclaimer: Option<String>,
}

/// The whole dashboard for one refresh: every provider's live/estimated quota
/// plus their aggregated local usage history. Order: Claude → Grok → opencode.
#[derive(Clone, Debug)]
pub struct Dashboard {
    pub claude_quota: Snapshot,
    pub claude_usage: Usage,
    pub grok_quota: Snapshot,
    pub grok_usage: Usage,
    pub opencode_quota: Snapshot,
    pub opencode_usage: Usage,
    pub generated_at: DateTime<Local>,
}

/// Aggregated usage history for one provider, computed from local files. The
/// dollar figures are API-equivalent estimates (see [`crate::pricing`]); token
/// counts are exact.
#[derive(Clone, Debug, Default)]
pub struct Usage {
    pub scope: String,
    pub authority: Authority,
    pub source: String,
    pub totals: Totals,
    pub windows: Windows,
    /// Ascending by date; one entry per day with activity.
    pub daily: Vec<DayPoint>,
    pub by_model: Vec<Segment>,
    pub by_project: Vec<Segment>,
    /// Populated for opencode (per inference provider); empty for Claude.
    pub by_provider: Vec<Segment>,
    pub tokens: TokenBreakdown,
    pub heatmap: Heatmap,
    pub error: Option<String>,
}

impl Usage {
    pub fn is_empty(&self) -> bool {
        self.totals.messages == 0
    }

    /// Mean daily spend over the days that had activity — the basis for burn-rate
    /// and projection callouts.
    pub fn avg_daily_cost(&self) -> f64 {
        if self.totals.active_days == 0 {
            0.0
        } else {
            self.totals.cost_usd / self.totals.active_days as f64
        }
    }

    /// Fraction of input that was served from cache — the cache hit rate.
    pub fn cache_hit_rate(&self) -> f64 {
        let base = self.tokens.input + self.tokens.cache_read;
        if base == 0 {
            0.0
        } else {
            self.tokens.cache_read as f64 / base as f64
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Totals {
    pub cost_usd: f64,
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub messages: u64,
    pub sessions: u64,
    pub active_days: u64,
    pub web_search: u64,
    pub web_fetch: u64,
    pub first_day: Option<NaiveDate>,
    pub last_day: Option<NaiveDate>,
}

impl Totals {
    pub fn total_tokens(&self) -> u64 {
        self.input + self.output + self.cache_write + self.cache_read
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DayPoint {
    pub date_ord: i32,
    pub cost: f64,
    pub tokens: u64,
    pub messages: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Segment {
    pub label: String,
    pub cost: f64,
    pub tokens: u64,
    pub messages: u64,
}

/// The token tiers that price differently, for the composition breakdown.
#[derive(Clone, Copy, Debug, Default)]
pub struct TokenBreakdown {
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub reasoning: u64,
}

impl TokenBreakdown {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_write + self.cache_read + self.reasoning
    }
}

/// Rolling cost/token/message totals over the three headline windows.
#[derive(Clone, Copy, Debug, Default)]
pub struct Windows {
    pub today: WinStat,
    pub seven: WinStat,
    pub thirty: WinStat,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WinStat {
    pub cost: f64,
    pub tokens: u64,
    pub messages: u64,
}

/// Activity by local weekday (0 = Monday) and hour, for the punch-card heatmap.
#[derive(Clone, Copy, Debug)]
pub struct Heatmap {
    pub counts: [[u64; 24]; 7],
    pub max: u64,
}

impl Default for Heatmap {
    fn default() -> Self {
        Heatmap { counts: [[0; 24]; 7], max: 0 }
    }
}
