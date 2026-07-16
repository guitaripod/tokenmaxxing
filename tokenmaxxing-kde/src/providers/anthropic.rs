use crate::creds::{self, ClaudeCredentials};
use crate::model::{Authority, Gauge, Severity, Snapshot, SpendInfo, Unit};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const OAUTH_BETA: &str = "oauth-2025-04-20";

/// Floor cooldown after a 429 so we never hammer Anthropic back into the limit.
const RATE_LIMIT_FLOOR: Duration = Duration::from_secs(5 * 60);
/// Backoff after a non-429 failure (network, 5xx, …).
const FAIL_COOLDOWN: Duration = Duration::from_secs(90);
/// Normal spacing between successful live polls.
pub const LIVE_INTERVAL: Duration = Duration::from_secs(2 * 60);

/// Result of a Claude usage fetch, including how long the worker should wait
/// before hitting the endpoint again.
pub struct FetchResult {
    pub snapshot: Snapshot,
    pub cooldown: Duration,
    pub fresh: bool,
}

pub fn fetch(client: &reqwest::blocking::Client) -> FetchResult {
    match load_and_fetch(client) {
        Ok(body) => {
            save_usage_cache(&body);
            match creds::load_claude() {
                Ok(creds) => FetchResult {
                    snapshot: parse(&body, &creds),
                    cooldown: LIVE_INTERVAL,
                    fresh: true,
                },
                Err(error) => FetchResult {
                    snapshot: unavailable(error),
                    cooldown: FAIL_COOLDOWN,
                    fresh: false,
                },
            }
        }
        Err(FetchError::RateLimited { message, retry_after }) => {
            let cooldown = retry_after.unwrap_or(RATE_LIMIT_FLOOR).max(RATE_LIMIT_FLOOR);
            FetchResult {
                snapshot: fallback_snapshot(&message, "rate limited"),
                cooldown,
                fresh: false,
            }
        }
        Err(FetchError::Other(message)) => FetchResult {
            snapshot: fallback_snapshot(&message, "cached"),
            cooldown: FAIL_COOLDOWN,
            fresh: false,
        },
    }
}

enum FetchError {
    RateLimited {
        message: String,
        retry_after: Option<Duration>,
    },
    Other(String),
}

fn load_and_fetch(client: &reqwest::blocking::Client) -> Result<String, FetchError> {
    let mut creds = creds::load_claude().map_err(FetchError::Other)?;

    if is_near_expiry(&creds) {
        if let Ok(fresh) = refresh(client, &creds.refresh_token) {
            creds = fresh;
        }
    }

    let (status, body, retry_after) = get_usage(client, &creds.access_token)?;
    if status == 401 || status == 403 {
        let fresh = refresh(client, &creds.refresh_token).map_err(FetchError::Other)?;
        let (retry_status, retry_body, retry_after) = get_usage(client, &fresh.access_token)?;
        return classify_status(retry_status, retry_body, retry_after);
    }
    classify_status(status, body, retry_after)
}

fn classify_status(
    status: u16,
    body: String,
    retry_after: Option<Duration>,
) -> Result<String, FetchError> {
    if status == 200 {
        return Ok(body);
    }
    if status == 429 {
        return Err(FetchError::RateLimited {
            message: "usage endpoint rate limited (HTTP 429)".into(),
            retry_after,
        });
    }
    Err(FetchError::Other(format!("usage endpoint returned {status}")))
}

fn is_near_expiry(creds: &ClaudeCredentials) -> bool {
    creds.expires_at_ms > 0 && Utc::now().timestamp_millis() >= creds.expires_at_ms - 120_000
}

fn get_usage(
    client: &reqwest::blocking::Client,
    token: &str,
) -> Result<(u16, String, Option<Duration>), FetchError> {
    let response = client
        .get(USAGE_URL)
        .header("authorization", format!("Bearer {token}"))
        .header("anthropic-beta", OAUTH_BETA)
        .header("accept", "application/json")
        .send()
        .map_err(|e| FetchError::Other(format!("request failed: {e}")))?;
    let status = response.status().as_u16();
    let retry_after = parse_retry_after(response.headers().get("retry-after"));
    let body = response.text().unwrap_or_default();
    Ok((status, body, retry_after))
}

fn parse_retry_after(value: Option<&reqwest::header::HeaderValue>) -> Option<Duration> {
    let raw = value?.to_str().ok()?.trim();
    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(Duration::from_secs(seconds.max(1)));
    }
    // HTTP-date form is rare for this endpoint; ignore if not integer seconds.
    None
}

/// Prefer an in-process or on-disk last-good body so the rings stay on screen
/// through 429s and brief outages. Only shows OFFLINE when nothing has ever
/// succeeded on this machine.
fn fallback_snapshot(message: &str, reason: &str) -> Snapshot {
    if let Some(body) = load_usage_cache() {
        if let Ok(creds) = creds::load_claude() {
            let mut snap = parse(&body, &creds);
            snap.source = format!("api.anthropic.com · {reason}");
            snap.note = Some(format!("Showing last good reading — {message}"));
            snap.error = None;
            return snap;
        }
    }
    unavailable(message.to_string())
}

fn cache_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| creds::home().join(".config"));
    base.join("tokenmaxxing/claude_usage_cache.json")
}

fn save_usage_cache(body: &str) {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let payload = serde_json::json!({
        "saved_at_ms": Utc::now().timestamp_millis(),
        "body": body,
    });
    if let Ok(bytes) = serde_json::to_vec_pretty(&payload) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

fn load_usage_cache() -> Option<String> {
    let raw = std::fs::read_to_string(cache_path()).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    json.get("body")?.as_str().map(str::to_string)
}

fn parse(body: &str, creds: &ClaudeCredentials) -> Snapshot {
    let json: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let mut gauges = Vec::new();

    if let Some(limits) = json.get("limits").and_then(Value::as_array) {
        gauges.extend(limits.iter().filter_map(gauge_from_limit));
    }
    if gauges.is_empty() {
        gauges.extend(gauges_from_top_level(&json));
    }
    if let Some(extra) = gauge_from_extra_usage(&json) {
        gauges.push(extra);
    }

    Snapshot {
        provider_id: "anthropic".into(),
        provider_name: "Claude".into(),
        subtitle: subtitle(creds),
        authority: Authority::Live,
        source: "api.anthropic.com · live".into(),
        gauges,
        details: details(&json),
        note: None,
        error: None,
        spend: spend_info(&json),
    }
}

fn spend_info(json: &Value) -> Option<SpendInfo> {
    let spend = json.get("spend")?;
    Some(SpendInfo {
        enabled: spend.get("enabled").and_then(Value::as_bool).unwrap_or(false),
        used: money(spend.get("used")).unwrap_or(0.0),
        limit: money(spend.get("limit")),
        balance: money(spend.get("balance")),
        can_purchase: spend
            .get("can_purchase_credits")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        disclaimer: spend
            .get("disclaimer")
            .and_then(Value::as_str)
            .map(strip_markdown_links),
    })
}

fn money(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    if let Some(n) = value.as_f64() {
        return Some(n);
    }
    let minor = value.get("amount_minor").and_then(Value::as_f64)?;
    let exponent = value.get("exponent").and_then(Value::as_i64).unwrap_or(2);
    Some(minor / 10f64.powi(exponent as i32))
}

fn strip_markdown_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let label: String = chars.by_ref().take_while(|&c| c != ']').collect();
            if chars.peek() == Some(&'(') {
                for c in chars.by_ref() {
                    if c == ')' {
                        break;
                    }
                }
            }
            out.push_str(&label);
        } else {
            out.push(c);
        }
    }
    out
}

fn severity_from_str(value: Option<&str>) -> Option<Severity> {
    match value {
        Some("critical") => Some(Severity::Critical),
        Some("warn") | Some("warning") => Some(Severity::Warn),
        Some("normal") | Some("ok") => Some(Severity::Nominal),
        _ => None,
    }
}

fn details(json: &Value) -> Vec<(String, String)> {
    let mut details = Vec::new();
    if let Some(ts) = parse_ts(json.pointer("/five_hour/resets_at")) {
        details.push(("Session resets".into(), human_until(ts)));
    }
    if let Some(ts) = parse_ts(json.pointer("/seven_day/resets_at")) {
        details.push(("Weekly resets".into(), human_until(ts)));
    }
    let extra_enabled = json
        .pointer("/extra_usage/is_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    details.push((
        "Extra usage credits".into(),
        if extra_enabled {
            "enabled".into()
        } else {
            "disabled".into()
        },
    ));
    details
}

fn human_until(ts: DateTime<Utc>) -> String {
    let seconds = (ts - Utc::now()).num_seconds();
    if seconds <= 0 {
        return "now".into();
    }
    let (days, hours, minutes) = (
        seconds / 86_400,
        (seconds % 86_400) / 3_600,
        (seconds % 3_600) / 60,
    );
    if days > 0 {
        format!("in {days}d {hours}h")
    } else if hours > 0 {
        format!("in {hours}h {minutes}m")
    } else {
        format!("in {minutes}m")
    }
}

fn gauge_from_limit(item: &Value) -> Option<Gauge> {
    let kind = item.get("kind").and_then(Value::as_str)?;
    let percent = item.get("percent").and_then(Value::as_f64)?;
    let model = item
        .get("scope")
        .and_then(|s| s.get("model"))
        .and_then(|m| m.get("display_name"))
        .and_then(Value::as_str);
    let label = match (kind, model) {
        ("session", _) => "5-hour session".to_string(),
        ("weekly_all", _) => "Weekly · all models".to_string(),
        ("weekly_scoped", Some(name)) => format!("Weekly · {name}"),
        ("weekly_scoped", None) => "Weekly · scoped".to_string(),
        (other, Some(name)) => format!("{} · {name}", pretty(other)),
        (other, None) => pretty(other),
    };
    Some(Gauge {
        key: kind.to_string(),
        label,
        fraction: (percent / 100.0).clamp(0.0, 1.0),
        unit: Unit::Percent,
        resets_at: parse_ts(item.get("resets_at")),
        trusted_reset: kind == "session",
        api_severity: severity_from_str(item.get("severity").and_then(Value::as_str)),
        is_active: item.get("is_active").and_then(Value::as_bool).unwrap_or(false),
        ..Default::default()
    })
}

fn gauges_from_top_level(json: &Value) -> Vec<Gauge> {
    [
        ("five_hour", "5-hour session", true),
        ("seven_day", "Weekly · all models", false),
    ]
    .into_iter()
    .filter_map(|(key, label, trusted)| {
        let obj = json.get(key)?;
        let utilization = obj.get("utilization").and_then(Value::as_f64)?;
        Some(Gauge {
            key: key.to_string(),
            label: label.to_string(),
            fraction: (utilization / 100.0).clamp(0.0, 1.0),
            unit: Unit::Percent,
            resets_at: parse_ts(obj.get("resets_at")),
            trusted_reset: trusted,
            ..Default::default()
        })
    })
    .collect()
}

fn gauge_from_extra_usage(json: &Value) -> Option<Gauge> {
    let extra = json.get("extra_usage")?;
    if extra.get("is_enabled").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let utilization = extra.get("utilization").and_then(Value::as_f64)?;
    Some(Gauge {
        key: "extra_usage".into(),
        label: "Extra usage credits".into(),
        fraction: (utilization / 100.0).clamp(0.0, 1.0),
        used: extra.get("used_credits").and_then(Value::as_f64),
        limit: extra.get("monthly_limit").and_then(Value::as_f64),
        unit: Unit::Usd,
        ..Default::default()
    })
}

fn pretty(kind: &str) -> String {
    let mut chars = kind.replace('_', " ");
    if let Some(first) = chars.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    chars
}

fn subtitle(creds: &ClaudeCredentials) -> String {
    let plan = match creds.subscription_type.as_str() {
        "max" => "Max",
        "pro" => "Pro",
        other => other,
    };
    match creds
        .rate_limit_tier
        .rsplit('_')
        .next()
        .filter(|s| s.ends_with('x'))
    {
        Some(mult) => format!("{plan} · {}×", mult.trim_end_matches('x')),
        None => plan.to_string(),
    }
}

fn parse_ts(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let raw = value?.as_str()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn refresh(
    client: &reqwest::blocking::Client,
    refresh_token: &str,
) -> Result<ClaudeCredentials, String> {
    if refresh_token.is_empty() {
        return Err("no refresh token — run `claude` to sign in".into());
    }
    let payload = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": CLIENT_ID,
    });
    let response = client
        .post(TOKEN_URL)
        .json(&payload)
        .send()
        .map_err(|e| format!("refresh request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("token refresh returned {}", response.status()));
    }
    let json: Value = response
        .json()
        .map_err(|e| format!("bad refresh response: {e}"))?;
    let access = json
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or("no access_token in refresh response")?
        .to_string();
    let new_refresh = json
        .get("refresh_token")
        .and_then(Value::as_str)
        .unwrap_or(refresh_token)
        .to_string();
    let expires_in = json.get("expires_in").and_then(Value::as_i64).unwrap_or(28_800);
    let expires_at_ms = Utc::now().timestamp_millis() + expires_in * 1000;

    write_back(&access, &new_refresh, expires_at_ms)?;
    creds::load_claude()
}

fn write_back(access: &str, refresh: &str, expires_at_ms: i64) -> Result<(), String> {
    let path = creds::claude_credentials_path();
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut json: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    let oauth = json
        .get_mut("claudeAiOauth")
        .and_then(Value::as_object_mut)
        .ok_or("credentials file missing claudeAiOauth")?;
    oauth.insert("accessToken".into(), Value::String(access.to_string()));
    oauth.insert("refreshToken".into(), Value::String(refresh.to_string()));
    oauth.insert("expiresAt".into(), Value::Number(expires_at_ms.into()));

    let serialized = serde_json::to_vec_pretty(&json).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tokenmaxxing-tmp");
    std::fs::write(&tmp, &serialized).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

fn unavailable(error: String) -> Snapshot {
    Snapshot {
        provider_id: "anthropic".into(),
        provider_name: "Claude".into(),
        subtitle: "Claude Max".into(),
        authority: Authority::Unavailable,
        source: "api.anthropic.com · unreachable".into(),
        gauges: Vec::new(),
        details: Vec::new(),
        note: None,
        error: Some(error),
        spend: None,
    }
}
