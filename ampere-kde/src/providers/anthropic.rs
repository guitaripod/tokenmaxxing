use crate::creds::{self, ClaudeCredentials};
use crate::model::{Authority, Gauge, Snapshot, Unit};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const OAUTH_BETA: &str = "oauth-2025-04-20";

pub fn fetch(client: &reqwest::blocking::Client) -> Snapshot {
    match load_and_fetch(client) {
        Ok(snapshot) => snapshot,
        Err(error) => unavailable(error),
    }
}

fn load_and_fetch(client: &reqwest::blocking::Client) -> Result<Snapshot, String> {
    let mut creds = creds::load_claude()?;

    if is_near_expiry(&creds) {
        if let Ok(fresh) = refresh(client, &creds.refresh_token) {
            creds = fresh;
        }
    }

    let (status, body) = get_usage(client, &creds.access_token)?;
    if status == 401 || status == 403 {
        let fresh = refresh(client, &creds.refresh_token)?;
        let (retry_status, retry_body) = get_usage(client, &fresh.access_token)?;
        if retry_status != 200 {
            return Err(format!("usage endpoint returned {retry_status} after refresh"));
        }
        return Ok(parse(&retry_body, &fresh));
    }
    if status != 200 {
        return Err(format!("usage endpoint returned {status}"));
    }
    Ok(parse(&body, &creds))
}

fn is_near_expiry(creds: &ClaudeCredentials) -> bool {
    creds.expires_at_ms > 0 && Utc::now().timestamp_millis() >= creds.expires_at_ms - 120_000
}

fn get_usage(client: &reqwest::blocking::Client, token: &str) -> Result<(u16, String), String> {
    let response = client
        .get(USAGE_URL)
        .header("authorization", format!("Bearer {token}"))
        .header("anthropic-beta", OAUTH_BETA)
        .header("accept", "application/json")
        .send()
        .map_err(|e| format!("request failed: {e}"))?;
    let status = response.status().as_u16();
    let body = response.text().unwrap_or_default();
    Ok((status, body))
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
        if extra_enabled { "enabled".into() } else { "disabled".into() },
    ));
    details
}

fn human_until(ts: DateTime<Utc>) -> String {
    let seconds = (ts - Utc::now()).num_seconds();
    if seconds <= 0 {
        return "now".into();
    }
    let (days, hours, minutes) = (seconds / 86_400, (seconds % 86_400) / 3_600, (seconds % 3_600) / 60);
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
        used: None,
        limit: None,
        unit: Unit::Percent,
        detail: None,
        resets_at: parse_ts(item.get("resets_at")),
        trusted_reset: kind == "session",
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
            used: None,
            limit: None,
            unit: Unit::Percent,
            detail: None,
            resets_at: parse_ts(obj.get("resets_at")),
            trusted_reset: trusted,
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
        detail: None,
        resets_at: None,
        trusted_reset: false,
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

/// Rotate the OAuth tokens back into the credentials file without disturbing
/// any sibling keys (e.g. `mcpOAuth`), writing atomically at mode 600.
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
    let tmp = path.with_extension("json.ampere-tmp");
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
    }
}
