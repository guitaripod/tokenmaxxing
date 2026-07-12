use crate::creds;
use crate::model::{
    Authority, DayPoint, Gauge, Heatmap, Segment, Snapshot, TokenBreakdown, Totals, Unit, Usage,
    WinStat, Windows,
};
use chrono::{Datelike, Local, NaiveDate};
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
            spend: None,
        },
        Err(error) => degraded(&error),
    }
}

/// Full local-history analytics across every provider opencode has run — the
/// paid Go gateway plus any free/local models — not just the capped Go spend.
pub fn usage() -> Usage {
    if !creds::opencode_db_path().exists() {
        return usage_unavailable("no opencode.db on this machine yet");
    }
    match collect_usage() {
        Ok(usage) => usage,
        Err(error) => usage_unavailable(&error),
    }
}

fn collect() -> Result<(Vec<Gauge>, Vec<(String, String)>), String> {
    let connection = open_read_only()?;
    let now_ms = Local::now().timestamp_millis();

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
                ..Default::default()
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok((gauges, all_time_details(&connection)?))
}

fn collect_usage() -> Result<Usage, String> {
    let db = open_read_only()?;

    let daily = daily_series(&db)?;
    let today = Local::now().date_naive().num_days_from_ce();
    let mut windows = Windows::default();
    let mut cost_usd = 0.0;
    for d in &daily {
        cost_usd += d.cost;
        let win = |w: &mut WinStat| {
            w.cost += d.cost;
            w.tokens += d.tokens;
            w.messages += d.messages;
        };
        if d.date_ord == today {
            win(&mut windows.today);
        }
        if d.date_ord > today - 7 {
            win(&mut windows.seven);
        }
        if d.date_ord > today - 30 {
            win(&mut windows.thirty);
        }
    }

    let tokens = token_breakdown(&db)?;
    let (messages, sessions, projects) = counts(&db)?;
    let _ = projects;
    let first = daily.first().map(|d| d.date_ord).and_then(NaiveDate::from_num_days_from_ce_opt);
    let last = daily.last().map(|d| d.date_ord).and_then(NaiveDate::from_num_days_from_ce_opt);

    let totals = Totals {
        cost_usd,
        input: tokens.input,
        output: tokens.output,
        cache_write: tokens.cache_write,
        cache_read: tokens.cache_read,
        messages,
        sessions,
        active_days: daily.len() as u64,
        web_search: 0,
        web_fetch: 0,
        first_day: first,
        last_day: last,
    };

    Ok(Usage {
        scope: "opencode".into(),
        authority: Authority::Estimated,
        source: "local opencode.db · all providers".into(),
        totals,
        windows,
        daily,
        by_model: segments(&db, "json_extract(data,'$.modelID')")?,
        by_project: Vec::new(),
        by_provider: segments(&db, "json_extract(data,'$.providerID')")?,
        tokens,
        heatmap: heatmap(&db)?,
        error: None,
    })
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
        ("Go spend (all-time)".into(), format!("${spend:.2}")),
        ("Go sessions".into(), sessions.to_string()),
        ("Go tokens in".into(), human_count(input)),
        ("Go tokens out".into(), human_count(output)),
        ("Go cache read".into(), human_count(cache_read)),
    ])
}

/// Cost, tokens and message count per local calendar day, ascending.
fn daily_series(db: &Connection) -> Result<Vec<DayPoint>, String> {
    let mut stmt = db
        .prepare(
            "SELECT date(time_created/1000,'unixepoch','localtime') d, \
                    COALESCE(SUM(json_extract(data,'$.cost')),0.0), \
                    COALESCE(SUM(json_extract(data,'$.tokens.total')),0), \
                    COUNT(*) \
             FROM message WHERE json_extract(data,'$.role')='assistant' \
             GROUP BY d ORDER BY d",
        )
        .map_err(|e| format!("daily prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| format!("daily query: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let (date, cost, tokens, msgs) = row.map_err(|e| e.to_string())?;
        let Ok(nd) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") else { continue };
        out.push(DayPoint {
            date_ord: nd.num_days_from_ce(),
            cost,
            tokens: tokens.max(0) as u64,
            messages: msgs.max(0) as u64,
        });
    }
    Ok(out)
}

fn token_breakdown(db: &Connection) -> Result<TokenBreakdown, String> {
    db.query_row(
        "SELECT COALESCE(SUM(json_extract(data,'$.tokens.input')),0), \
                COALESCE(SUM(json_extract(data,'$.tokens.output')),0), \
                COALESCE(SUM(json_extract(data,'$.tokens.cache.write')),0), \
                COALESCE(SUM(json_extract(data,'$.tokens.cache.read')),0), \
                COALESCE(SUM(json_extract(data,'$.tokens.reasoning')),0) \
         FROM message WHERE json_extract(data,'$.role')='assistant'",
        [],
        |row| {
            Ok(TokenBreakdown {
                input: row.get::<_, i64>(0)?.max(0) as u64,
                output: row.get::<_, i64>(1)?.max(0) as u64,
                cache_write: row.get::<_, i64>(2)?.max(0) as u64,
                cache_read: row.get::<_, i64>(3)?.max(0) as u64,
                reasoning: row.get::<_, i64>(4)?.max(0) as u64,
            })
        },
    )
    .map_err(|e| format!("tokens query: {e}"))
}

fn counts(db: &Connection) -> Result<(u64, u64, u64), String> {
    let messages = db
        .query_row(
            "SELECT COUNT(*) FROM message WHERE json_extract(data,'$.role')='assistant'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| format!("msg count: {e}"))?;
    let sessions = db
        .query_row("SELECT COUNT(*) FROM session", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0);
    let projects = db
        .query_row("SELECT COUNT(*) FROM project", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0);
    Ok((messages.max(0) as u64, sessions.max(0) as u64, projects.max(0) as u64))
}

/// Cost / token / message totals grouped by an arbitrary JSON expression
/// (provider id or model id), sorted by tokens desc.
fn segments(db: &Connection, group_expr: &str) -> Result<Vec<Segment>, String> {
    let sql = format!(
        "SELECT COALESCE({group_expr},'?') g, \
                COALESCE(SUM(json_extract(data,'$.cost')),0.0), \
                COALESCE(SUM(json_extract(data,'$.tokens.total')),0), \
                COUNT(*) \
         FROM message WHERE json_extract(data,'$.role')='assistant' \
         GROUP BY g ORDER BY 3 DESC"
    );
    let mut stmt = db.prepare(&sql).map_err(|e| format!("segment prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Segment {
                label: row.get::<_, String>(0)?,
                cost: row.get::<_, f64>(1)?,
                tokens: row.get::<_, i64>(2)?.max(0) as u64,
                messages: row.get::<_, i64>(3)?.max(0) as u64,
            })
        })
        .map_err(|e| format!("segment query: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

fn heatmap(db: &Connection) -> Result<Heatmap, String> {
    let mut stmt = db
        .prepare(
            "SELECT CAST(strftime('%w', time_created/1000,'unixepoch','localtime') AS INTEGER), \
                    CAST(strftime('%H', time_created/1000,'unixepoch','localtime') AS INTEGER), \
                    COUNT(*) \
             FROM message WHERE json_extract(data,'$.role')='assistant' \
             GROUP BY 1, 2",
        )
        .map_err(|e| format!("heatmap prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
        })
        .map_err(|e| format!("heatmap query: {e}"))?;
    let mut heat = Heatmap::default();
    for row in rows {
        let (sun_dow, hour, count) = row.map_err(|e| e.to_string())?;
        let weekday = ((sun_dow + 6) % 7) as usize; // strftime 0=Sun → Monday-based
        let hour = hour.clamp(0, 23) as usize;
        let count = count.max(0) as u64;
        heat.counts[weekday][hour] += count;
        heat.max = heat.max.max(heat.counts[weekday][hour]);
    }
    Ok(heat)
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
        spend: None,
    }
}

fn usage_unavailable(message: &str) -> Usage {
    Usage {
        scope: "opencode".into(),
        authority: Authority::Unavailable,
        source: "local opencode.db · unavailable".into(),
        error: Some(message.to_string()),
        ..Default::default()
    }
}
