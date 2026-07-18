use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const REMOTE_DB_PATH: &str = ".local/share/opencode/opencode.db";
const FRESH_TTL: Duration = Duration::from_secs(60);
const STALE_TTL: Duration = Duration::from_secs(15 * 60);
const SSH_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Default)]
pub struct RemoteWindow {
    pub spend: f64,
    pub requests: i64,
}

#[derive(Clone, Copy, Default)]
pub struct RemoteStats {
    pub w5h: RemoteWindow,
    pub w7d: RemoteWindow,
    pub w30d: RemoteWindow,
    pub all_time_spend: f64,
    pub sessions: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cache_read: i64,
}

impl RemoteStats {
    pub fn window(&self, key: &str) -> RemoteWindow {
        match key {
            "5h" => self.w5h,
            "7d" => self.w7d,
            "30d" => self.w30d,
            _ => RemoteWindow::default(),
        }
    }

    fn add(&mut self, other: &RemoteStats) {
        for (mine, theirs) in [
            (&mut self.w5h, other.w5h),
            (&mut self.w7d, other.w7d),
            (&mut self.w30d, other.w30d),
        ] {
            mine.spend += theirs.spend;
            mine.requests += theirs.requests;
        }
        self.all_time_spend += other.all_time_spend;
        self.sessions += other.sessions;
        self.tokens_in += other.tokens_in;
        self.tokens_out += other.tokens_out;
        self.cache_read += other.cache_read;
    }
}

#[derive(Clone, Default)]
pub struct RemoteReport {
    pub configured: Vec<String>,
    pub reached: Vec<String>,
    pub stale: Vec<String>,
    pub unreachable: Vec<String>,
    pub stats: RemoteStats,
}

impl RemoteReport {
    pub fn included_count(&self) -> usize {
        self.reached.len() + self.stale.len()
    }
}

/// Sums opencode-go spend from other machines' opencode.db over SSH, so the
/// rolling-cap estimate covers the whole account instead of just this machine.
/// Hosts come from `opencode_remote_hosts` in the tokenmaxxing config and must
/// be reachable via non-interactive `ssh <host>` (e.g. Tailscale peers).
pub fn report() -> RemoteReport {
    let mut report = RemoteReport {
        configured: crate::config::load().opencode_remote_hosts,
        ..Default::default()
    };
    report.configured.retain(|host| !host.is_empty());
    let now_ms = chrono::Local::now().timestamp_millis();
    for host in report.configured.clone() {
        if let Some((stats, age)) = cached(&host) {
            if age < FRESH_TTL {
                report.stats.add(&stats);
                report.reached.push(host);
                continue;
            }
        }
        match query(&host, now_ms) {
            Ok(stats) => {
                store(&host, stats);
                report.stats.add(&stats);
                report.reached.push(host);
            }
            Err(_) => match cached(&host) {
                Some((stats, age)) if age < STALE_TTL => {
                    report.stats.add(&stats);
                    report.stale.push(host);
                }
                _ => report.unreachable.push(host),
            },
        }
    }
    report
}

fn query(host: &str, now_ms: i64) -> Result<RemoteStats, String> {
    let raw = run_ssh(host, &aggregate_sql(now_ms))?;
    let rows: serde_json::Value =
        serde_json::from_str(raw.trim()).map_err(|e| format!("{host}: parse: {e}"))?;
    let row = rows
        .as_array()
        .and_then(|rows| rows.first())
        .ok_or_else(|| format!("{host}: empty sqlite output"))?;
    let f = |key: &str| row.get(key).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
    let i = |key: &str| row.get(key).and_then(serde_json::Value::as_i64).unwrap_or(0);
    Ok(RemoteStats {
        w5h: RemoteWindow { spend: f("s5"), requests: i("r5") },
        w7d: RemoteWindow { spend: f("s7"), requests: i("r7") },
        w30d: RemoteWindow { spend: f("s30"), requests: i("r30") },
        all_time_spend: f("sall"),
        sessions: i("sess"),
        tokens_in: i("tin"),
        tokens_out: i("tout"),
        cache_read: i("cr"),
    })
}

fn aggregate_sql(now_ms: i64) -> String {
    let c5 = now_ms - 5 * 3_600_000;
    let c7 = now_ms - 7 * 24 * 3_600_000;
    let c30 = now_ms - 30 * 24 * 3_600_000;
    format!(
        ".timeout 2000\n\
         SELECT \
          COALESCE(SUM(CASE WHEN time_created>={c5} THEN json_extract(data,'$.cost') END),0.0) AS s5, \
          COUNT(CASE WHEN time_created>={c5} AND json_extract(data,'$.cost') IS NOT NULL THEN 1 END) AS r5, \
          COALESCE(SUM(CASE WHEN time_created>={c7} THEN json_extract(data,'$.cost') END),0.0) AS s7, \
          COUNT(CASE WHEN time_created>={c7} AND json_extract(data,'$.cost') IS NOT NULL THEN 1 END) AS r7, \
          COALESCE(SUM(CASE WHEN time_created>={c30} THEN json_extract(data,'$.cost') END),0.0) AS s30, \
          COUNT(CASE WHEN time_created>={c30} AND json_extract(data,'$.cost') IS NOT NULL THEN 1 END) AS r30, \
          COALESCE(SUM(json_extract(data,'$.cost')),0.0) AS sall, \
          COUNT(DISTINCT session_id) AS sess, \
          COALESCE(SUM(json_extract(data,'$.tokens.input')),0) AS tin, \
          COALESCE(SUM(json_extract(data,'$.tokens.output')),0) AS tout, \
          COALESCE(SUM(json_extract(data,'$.tokens.cache.read')),0) AS cr \
         FROM message \
         WHERE json_extract(data,'$.providerID')='opencode-go';\n"
    )
}

/// Streams the SQL over stdin so no shell quoting of the statement is needed;
/// a try_wait poll loop bounds the whole call even if ssh hangs mid-session.
fn run_ssh(host: &str, sql: &str) -> Result<String, String> {
    let mut child = Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=4",
            host,
            &format!("sqlite3 -readonly -json {REMOTE_DB_PATH}"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn ssh: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(sql.as_bytes());
    }
    let deadline = Instant::now() + SSH_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("ssh {host} timed out"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("ssh {host}: {e}")),
        }
    };
    if !status.success() {
        return Err(format!("ssh {host} exited {status}"));
    }
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut out);
    }
    Ok(out)
}

fn cache() -> &'static Mutex<HashMap<String, (RemoteStats, Instant)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (RemoteStats, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached(host: &str) -> Option<(RemoteStats, Duration)> {
    let entries = cache().lock().ok()?;
    let (stats, at) = entries.get(host)?;
    Some((*stats, at.elapsed()))
}

fn store(host: &str, stats: RemoteStats) {
    if let Ok(mut entries) = cache().lock() {
        entries.insert(host.to_string(), (stats, Instant::now()));
    }
}
