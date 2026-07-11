use chrono::{DateTime, Utc};

/// How much to trust the numbers on a provider card.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Authority {
    Live,
    Estimated,
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

    pub fn css_class(self) -> &'static str {
        match self {
            Authority::Live => "badge-live",
            Authority::Estimated => "badge-est",
            Authority::Unavailable => "badge-offline",
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
    #[allow(dead_code)]
    pub key: String,
    pub label: String,
    pub fraction: f64,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub unit: Unit,
    pub detail: Option<String>,
    pub resets_at: Option<DateTime<Utc>>,
    pub trusted_reset: bool,
}

impl Gauge {
    pub fn severity(&self) -> Severity {
        Severity::from_fraction(self.fraction)
    }

    pub fn percent_text(&self) -> String {
        format!("{}%", (self.fraction * 100.0).round() as i64)
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
}
