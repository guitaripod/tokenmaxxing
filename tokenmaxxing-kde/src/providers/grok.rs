use crate::creds::{self, GrokCredentials};
use crate::model::{Authority, Gauge, Snapshot, SpendInfo, Unit};
use crate::providers::FetchResult;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

const CREDITS_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
const BILLING_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing";
const CLIENT_VERSION: &str = "0.2.101";

/// Floor cooldown after a 429 so we never hammer the proxy back into the limit.
const RATE_LIMIT_FLOOR: Duration = Duration::from_secs(5 * 60);
/// Backoff after a non-429 failure (network, 5xx, …).
const FAIL_COOLDOWN: Duration = Duration::from_secs(90);
/// Normal spacing between successful live polls.
pub const LIVE_INTERVAL: Duration = Duration::from_secs(2 * 60);

pub fn fetch(client: &reqwest::blocking::Client) -> FetchResult {
    match load_and_fetch(client) {
        Ok((credits_body, dollars_body, creds)) => {
            save_usage_cache(&credits_body, dollars_body.as_deref());
            FetchResult {
                snapshot: parse(&credits_body, dollars_body.as_deref(), &creds),
                cooldown: LIVE_INTERVAL,
                fresh: true,
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

fn load_and_fetch(
    client: &reqwest::blocking::Client,
) -> Result<(String, Option<String>, GrokCredentials), FetchError> {
    let mut creds = creds::load_grok().map_err(FetchError::Other)?;

    if is_near_expiry(&creds) {
        if let Ok(fresh) = refresh(client, &creds) {
            creds = fresh;
        }
    }

    let (status, credits_body, retry_after) = get_json(client, CREDITS_URL, &creds.access_token)?;
    if status == 401 || status == 403 {
        let fresh = refresh(client, &creds).map_err(FetchError::Other)?;
        let (retry_status, retry_body, retry_after) =
            get_json(client, CREDITS_URL, &fresh.access_token)?;
        let body = classify_status(retry_status, retry_body, retry_after)?;
        let dollars = fetch_dollars(client, &fresh.access_token);
        return Ok((body, dollars, fresh));
    }
    let body = classify_status(status, credits_body, retry_after)?;
    let dollars = fetch_dollars(client, &creds.access_token);
    Ok((body, dollars, creds))
}

fn fetch_dollars(client: &reqwest::blocking::Client, token: &str) -> Option<String> {
    get_json(client, BILLING_URL, token)
        .ok()
        .filter(|(s, _, _)| *s == 200)
        .map(|(_, b, _)| b)
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
            message: "billing endpoint rate limited (HTTP 429)".into(),
            retry_after,
        });
    }
    Err(FetchError::Other(format!("billing endpoint returned {status}")))
}

fn is_near_expiry(creds: &GrokCredentials) -> bool {
    creds.expires_at_ms > 0 && Utc::now().timestamp_millis() >= creds.expires_at_ms - 120_000
}

fn get_json(
    client: &reqwest::blocking::Client,
    url: &str,
    token: &str,
) -> Result<(u16, String, Option<Duration>), FetchError> {
    let response = client
        .get(url)
        .header("authorization", format!("Bearer {token}"))
        .header("accept", "application/json")
        .header("x-grok-client-version", CLIENT_VERSION)
        .header("x-grok-client-mode", "cli")
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

/// Prefer an on-disk last-good body so the rings stay on screen through 429s
/// and brief outages. Only shows OFFLINE when nothing has ever succeeded on
/// this machine.
fn fallback_snapshot(message: &str, reason: &str) -> Snapshot {
    if let Some((credits, dollars)) = load_usage_cache() {
        if let Ok(creds) = creds::load_grok() {
            let mut snap = parse(&credits, dollars.as_deref(), &creds);
            snap.source = format!("cli-chat-proxy.grok.com · {reason}");
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
    base.join("tokenmaxxing/grok_usage_cache.json")
}

fn save_usage_cache(credits_body: &str, dollars_body: Option<&str>) {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let payload = serde_json::json!({
        "saved_at_ms": Utc::now().timestamp_millis(),
        "credits": credits_body,
        "dollars": dollars_body,
    });
    if let Ok(bytes) = serde_json::to_vec_pretty(&payload) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

fn load_usage_cache() -> Option<(String, Option<String>)> {
    let raw = std::fs::read_to_string(cache_path()).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    let credits = json.get("credits")?.as_str()?.to_string();
    let dollars = json.get("dollars").and_then(Value::as_str).map(str::to_string);
    Some((credits, dollars))
}

fn parse(credits_body: &str, dollars_body: Option<&str>, creds: &GrokCredentials) -> Snapshot {
    let credits: Value = serde_json::from_str(credits_body).unwrap_or(Value::Null);
    let config = credits.get("config").cloned().unwrap_or(Value::Null);

    let mut gauges = Vec::new();

    let weekly_pct = config
        .get("creditUsagePercent")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let weekly_reset = parse_ts(
        config
            .pointer("/currentPeriod/end")
            .or_else(|| config.get("billingPeriodEnd")),
    );
    gauges.push(Gauge {
        key: "weekly".into(),
        label: "Weekly credits".into(),
        fraction: (weekly_pct / 100.0).clamp(0.0, 1.0),
        unit: Unit::Percent,
        resets_at: weekly_reset,
        trusted_reset: true,
        is_active: false,
        ..Default::default()
    });

    if let Some(products) = config.get("productUsage").and_then(Value::as_array) {
        for product in products {
            let name = product
                .get("product")
                .and_then(Value::as_str)
                .unwrap_or("product");
            let Some(pct) = product.get("usagePercent").and_then(Value::as_f64) else {
                continue;
            };
            gauges.push(Gauge {
                key: format!("product_{}", name.to_ascii_lowercase()),
                label: pretty_product(name),
                fraction: (pct / 100.0).clamp(0.0, 1.0),
                unit: Unit::Percent,
                resets_at: weekly_reset,
                trusted_reset: true,
                is_active: false,
                ..Default::default()
            });
        }
    }

    let on_cap = money_cents(config.get("onDemandCap"));
    let on_used = money_cents(config.get("onDemandUsed")).unwrap_or(0.0);
    if let Some(cap) = on_cap.filter(|c| *c > 0.0) {
        gauges.push(Gauge {
            key: "on_demand".into(),
            label: "Pay-as-you-go".into(),
            fraction: (on_used / cap).clamp(0.0, 1.0),
            used: Some(on_used),
            limit: Some(cap),
            unit: Unit::Usd,
            trusted_reset: false,
            is_active: false,
            ..Default::default()
        });
    }

    if let Some(dollars_raw) = dollars_body {
        if let Ok(dollars) = serde_json::from_str::<Value>(dollars_raw) {
            let dcfg = dollars.get("config").cloned().unwrap_or(Value::Null);
            if let (Some(used), Some(limit)) = (
                money_cents(dcfg.get("used")),
                money_cents(dcfg.get("monthlyLimit")).filter(|l| *l > 0.0),
            ) {
                gauges.push(Gauge {
                    key: "monthly".into(),
                    label: "Monthly spend".into(),
                    fraction: (used / limit).clamp(0.0, 1.0),
                    used: Some(used),
                    limit: Some(limit),
                    unit: Unit::Usd,
                    resets_at: parse_ts(dcfg.get("billingPeriodEnd")),
                    trusted_reset: false,
                    is_active: false,
                    ..Default::default()
                });
            }
        }
    }

    mark_binding(&mut gauges);

    let prepaid = money_cents(config.get("prepaidBalance"));
    let spend = prepaid.map(|balance| SpendInfo {
        enabled: balance > 0.0 || on_cap.unwrap_or(0.0) > 0.0,
        used: on_used,
        limit: on_cap,
        balance: Some(balance),
        can_purchase: true,
        disclaimer: Some("Prepaid balance / on-demand from grok.com billing".into()),
    });

    Snapshot {
        provider_id: "xai".into(),
        provider_name: "Grok".into(),
        subtitle: subtitle(creds),
        authority: Authority::Live,
        source: "cli-chat-proxy.grok.com · live".into(),
        gauges,
        details: details(&config, creds, prepaid),
        note: None,
        error: None,
        spend,
    }
}

fn mark_binding(gauges: &mut [Gauge]) {
    if gauges.is_empty() {
        return;
    }
    let idx = gauges
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.fraction.total_cmp(&b.fraction))
        .map(|(i, _)| i)
        .unwrap_or(0);
    gauges[idx].is_active = true;
}

fn money_cents(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    if let Some(n) = value.as_f64() {
        return Some(n / 100.0);
    }
    let n = value.get("val").and_then(Value::as_f64)?;
    Some(n / 100.0)
}

fn pretty_product(name: &str) -> String {
    match name {
        "GrokBuild" => "Grok Build".into(),
        "Api" | "API" => "API".into(),
        other => other.to_string(),
    }
}

fn subtitle(creds: &GrokCredentials) -> String {
    match creds.tier {
        0 => "Free · live".into(),
        1 => "Basic · live".into(),
        2 => "SuperGrok · live".into(),
        3 => "X Premium · live".into(),
        n if n > 3 => format!("Tier {n} · live"),
        _ => "Grok · live".into(),
    }
}

fn details(config: &Value, creds: &GrokCredentials, prepaid: Option<f64>) -> Vec<(String, String)> {
    let mut details = Vec::new();
    if !creds.email.is_empty() {
        details.push(("Account".into(), creds.email.clone()));
    }
    if let Some(ts) = parse_ts(
        config
            .pointer("/currentPeriod/end")
            .or_else(|| config.get("billingPeriodEnd")),
    ) {
        details.push(("Weekly resets".into(), human_until(ts)));
    }
    if let Some(balance) = prepaid {
        details.push(("Prepaid balance".into(), format!("${balance:.2}")));
    }
    let period = config
        .pointer("/currentPeriod/type")
        .and_then(Value::as_str)
        .unwrap_or("weekly");
    details.push(("Period".into(), period.replace("USAGE_PERIOD_TYPE_", "").to_ascii_lowercase()));
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

fn parse_ts(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let raw = value?.as_str()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn refresh(
    client: &reqwest::blocking::Client,
    creds: &GrokCredentials,
) -> Result<GrokCredentials, String> {
    if creds.refresh_token.is_empty() {
        return Err("no refresh token — run `grok login`".into());
    }
    if creds.oidc_client_id.is_empty() {
        return Err("no OIDC client id in grok auth".into());
    }
    let token_url = format!(
        "{}/oauth2/token",
        creds.oidc_issuer.trim_end_matches('/')
    );
    let body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={}",
        urlencoding_form(&creds.refresh_token),
        urlencoding_form(&creds.oidc_client_id),
    );
    let response = client
        .post(&token_url)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .body(body)
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
        .unwrap_or(&creds.refresh_token)
        .to_string();
    let expires_in = json.get("expires_in").and_then(Value::as_i64).unwrap_or(21_600);
    let expires_at = (Utc::now() + chrono::Duration::seconds(expires_in)).to_rfc3339();

    write_back(creds, &access, &new_refresh, &expires_at)?;
    creds::load_grok()
}

/// Rotate the OIDC tokens back into auth.json without disturbing sibling keys,
/// writing atomically at mode 600.
fn write_back(
    creds: &GrokCredentials,
    access: &str,
    refresh: &str,
    expires_at: &str,
) -> Result<(), String> {
    let path = creds::grok_auth_path();
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut json: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let entry = json
        .get_mut(&creds.entry_key)
        .and_then(Value::as_object_mut)
        .ok_or("auth entry missing after refresh")?;
    entry.insert("key".into(), Value::String(access.to_string()));
    entry.insert("refresh_token".into(), Value::String(refresh.to_string()));
    entry.insert("expires_at".into(), Value::String(expires_at.to_string()));

    let serialized = serde_json::to_vec_pretty(&json).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tokenmaxxing-tmp");
    std::fs::write(&tmp, &serialized).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

fn urlencoding_form(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn unavailable(error: String) -> Snapshot {
    Snapshot {
        provider_id: "xai".into(),
        provider_name: "Grok".into(),
        subtitle: "Grok Build".into(),
        authority: Authority::Unavailable,
        source: "cli-chat-proxy.grok.com · unreachable".into(),
        gauges: Vec::new(),
        details: Vec::new(),
        note: None,
        error: Some(error),
        spend: None,
    }
}
