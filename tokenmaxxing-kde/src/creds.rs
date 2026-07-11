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
