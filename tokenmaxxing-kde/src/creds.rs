use std::path::PathBuf;

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

pub fn claude_credentials_path() -> PathBuf {
    home().join(".claude/.credentials.json")
}

pub fn opencode_auth_path() -> PathBuf {
    home().join(".local/share/opencode/auth.json")
}

pub fn opencode_db_path() -> PathBuf {
    home().join(".local/share/opencode/opencode.db")
}

pub fn grok_auth_path() -> PathBuf {
    home().join(".grok/auth.json")
}

pub fn grok_sessions_path() -> PathBuf {
    home().join(".grok/sessions")
}

#[derive(Clone, Debug)]
pub struct ClaudeCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_ms: i64,
    pub subscription_type: String,
    pub rate_limit_tier: String,
}

pub fn load_claude() -> Result<ClaudeCredentials, String> {
    let path = claude_credentials_path();
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("bad JSON in credentials: {e}"))?;
    let oauth = json
        .get("claudeAiOauth")
        .ok_or("no claudeAiOauth block — run `claude` to sign in")?;

    let access_token = oauth
        .get("accessToken")
        .and_then(|v| v.as_str())
        .ok_or("no accessToken")?
        .to_string();
    let refresh_token = oauth
        .get("refreshToken")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let expires_at_ms = oauth
        .get("expiresAt")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let subscription_type = oauth
        .get("subscriptionType")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let rate_limit_tier = oauth
        .get("rateLimitTier")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    Ok(ClaudeCredentials {
        access_token,
        refresh_token,
        expires_at_ms,
        subscription_type,
        rate_limit_tier,
    })
}

/// True when the opencode-go subscription is configured on this machine.
pub fn opencode_go_configured() -> bool {
    let Ok(raw) = std::fs::read_to_string(opencode_auth_path()) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| v.get("opencode-go").cloned())
        .is_some()
}

/// OIDC session written by `grok login` into `~/.grok/auth.json`.
#[derive(Clone, Debug)]
pub struct GrokCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_ms: i64,
    pub oidc_issuer: String,
    pub oidc_client_id: String,
    pub email: String,
    /// JWT `tier` claim when present (0=free … higher=paid).
    pub tier: i64,
    /// Map key under which this entry lives in auth.json (issuer::client_id).
    pub entry_key: String,
}

pub fn load_grok() -> Result<GrokCredentials, String> {
    let path = grok_auth_path();
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("bad JSON in grok auth: {e}"))?;
    let object = json
        .as_object()
        .ok_or("grok auth.json is not an object — run `grok login`")?;

    // Prefer the live xAI OIDC session; fall back to the first entry with a token.
    let (entry_key, entry) = object
        .iter()
        .find(|(k, v)| {
            k.contains("auth.x.ai")
                && v.get("key").and_then(|x| x.as_str()).is_some_and(|t| !t.is_empty())
        })
        .or_else(|| {
            object.iter().find(|(_, v)| {
                v.get("key").and_then(|x| x.as_str()).is_some_and(|t| !t.is_empty())
            })
        })
        .ok_or("no grok session — run `grok login`")?;

    let access_token = entry
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or("no access token in grok auth")?
        .to_string();
    let refresh_token = entry
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let expires_at_ms = parse_expires_ms(
        entry
            .get("expires_at")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
    );
    let oidc_issuer = entry
        .get("oidc_issuer")
        .and_then(|v| v.as_str())
        .unwrap_or("https://auth.x.ai")
        .to_string();
    let oidc_client_id = entry
        .get("oidc_client_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let email = entry
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let tier = jwt_claim_i64(&access_token, "tier").unwrap_or(0);

    Ok(GrokCredentials {
        access_token,
        refresh_token,
        expires_at_ms,
        oidc_issuer,
        oidc_client_id,
        email,
        tier,
        entry_key: entry_key.clone(),
    })
}

fn parse_expires_ms(raw: &str) -> i64 {
    if raw.is_empty() {
        return 0;
    }
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

fn jwt_claim_i64(token: &str, claim: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let padded = match payload.len() % 4 {
        2 => format!("{payload}=="),
        3 => format!("{payload}="),
        _ => payload.to_string(),
    };
    let bytes = base64_url_decode(&padded)?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    json.get(claim).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
}

fn base64_url_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' | b'+' => Some(62),
            b'_' | b'/' => Some(63),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut i = 0;
    while i + 3 < bytes.len() {
        let (a, b, c, d) = (
            val(bytes[i])?,
            val(bytes[i + 1])?,
            if bytes[i + 2] == b'=' { 0 } else { val(bytes[i + 2])? },
            if bytes[i + 3] == b'=' { 0 } else { val(bytes[i + 3])? },
        );
        out.push((a << 2) | (b >> 4));
        if bytes[i + 2] != b'=' {
            out.push((b << 4) | (c >> 2));
        }
        if bytes[i + 3] != b'=' {
            out.push((c << 6) | d);
        }
        i += 4;
    }
    Some(out)
}


