use crate::creds;
use crate::model::{Authority, Gauge, Snapshot, Unit};
use chrono::Utc;
use rusqlite::{Connection, OpenFlags};
use std::time::Duration;

const CAP_5H: f64 = 12.0;
const CAP_7D: f64 = 30.0;
const CAP_30D: f64 = 60.0;

const NOTE: &str = "No usage API exists. Estimated from this machine's opencode.db against Go's rolling dollar caps — may miss usage on other machines and server-side accounting.";
const PROVIDER: &str = "opencode-go";

pub fn fetch() -> Snapshot {
    if !creds::opencode_go_configured() {
        return degraded("opencode-go not configured — run `opencode auth login`");
    }
    if !creds::opencode_db_path().exists() {
        return degraded("no opencode.db on this machine yet");
    }
    match collect() {
        Ok((gauges, details)) => Snapshot {
            provider_id: PROVIDER.into(),
            provider_name: "opencode go".into(),
            subtitle: "$10/mo · estimated locally".into(),
            authority: Authority::Estimated,
            source: "local opencode.db · estimate".into(),
            gauges,
            details,
            note: Some(NOTE.into()),
            error: None,
        },
        Err(error) => degraded(&error),
    }
}

fn collect() -> Result<(Vec<Gauge>, Vec<(String, String)>), String> {
    let connection = open_read_only()?;
    let now_ms = Utc::now().timestamp_millis();

    let windows = [
        ("5h", "5-hour rolling", 5 * 3_600_000_i64, CAP_5H),
        ("7d", "Weekly rolling", 7 * 24 * 3_600_000, CAP_7D),
        ("30d", "Monthly rolling", 30 * 24 * 3_600_000, CAP_30D),
    ];
    let gauges = windows
        .into_iter()
        .map(|(key, label, span_ms, cap)| {
            let (spend, requests) = window_stats(&connection, now_ms - span_ms)?;
            Ok(Gauge {
                key: key.into(),
                label: label.into(),
                fraction: (spend / cap).clamp(0.0, 1.0),
                used: Some(spend),
                limit: Some(cap),
                unit: Unit::Usd,
                detail: Some(format!("{requests} req")),
                resets_at: None,
                trusted_reset: false,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok((gauges, all_time_details(&connection)?))
}

/// Open the live opencode SQLite database for reading only. The connection is
/// opened read/write-capable so it can participate in WAL, but `query_only`
/// forbids any mutation of the user's data.
fn open_read_only() -> Result<Connection, String> {
    let path = creds::opencode_db_path();
    let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| format!("open opencode.db: {e}"))?;
    let _ = connection.busy_timeout(Duration::from_millis(2000));
    connection
        .pragma_update(None, "query_only", true)
        .map_err(|e| format!("query_only: {e}"))?;
    Ok(connection)
}

fn window_stats(connection: &Connection, cutoff_ms: i64) -> Result<(f64, i64), String> {
    connection
        .query_row(
            "SELECT COALESCE(SUM(json_extract(data,'$.cost')),0.0), COUNT(*) \
             FROM message \
             WHERE json_extract(data,'$.providerID')=?1 \
               AND json_extract(data,'$.cost') IS NOT NULL \
               AND time_created >= ?2",
            rusqlite::params![PROVIDER, cutoff_ms],
            |row| Ok((row.get::<_, f64>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|e| format!("usage query: {e}"))
}

fn all_time_details(connection: &Connection) -> Result<Vec<(String, String)>, String> {
    let row = connection
        .query_row(
            "SELECT COALESCE(SUM(json_extract(data,'$.cost')),0.0), \
                    COUNT(DISTINCT session_id), \
                    COALESCE(SUM(json_extract(data,'$.tokens.input')),0), \
                    COALESCE(SUM(json_extract(data,'$.tokens.output')),0), \
                    COALESCE(SUM(json_extract(data,'$.tokens.cache.read')),0) \
             FROM message WHERE json_extract(data,'$.providerID')=?1",
            [PROVIDER],
            |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .map_err(|e| format!("stats query: {e}"))?;
    let (spend, sessions, input, output, cache_read) = row;
    Ok(vec![
        ("All-time spend".into(), format!("${spend:.2}")),
        ("Sessions".into(), sessions.to_string()),
        ("Tokens in".into(), human_count(input)),
        ("Tokens out".into(), human_count(output)),
        ("Cache read".into(), human_count(cache_read)),
    ])
}

fn human_count(n: i64) -> String {
    let value = n as f64;
    if value >= 1e9 {
        format!("{:.1}B", value / 1e9)
    } else if value >= 1e6 {
        format!("{:.1}M", value / 1e6)
    } else if value >= 1e3 {
        format!("{:.1}K", value / 1e3)
    } else {
        n.to_string()
    }
}

fn degraded(message: &str) -> Snapshot {
    Snapshot {
        provider_id: PROVIDER.into(),
        provider_name: "opencode go".into(),
        subtitle: "$10/mo subscription".into(),
        authority: Authority::Unavailable,
        source: "local opencode.db · unavailable".into(),
        gauges: Vec::new(),
        details: Vec::new(),
        note: None,
        error: Some(message.to_string()),
    }
}
