//! Claude usage history, aggregated from the JSONL transcripts Claude Code
//! writes under `~/.claude/projects`. This is the same corpus `ccusage` reads:
//! every assistant turn records its exact token usage. The subscription bills a
//! flat fee, so the dollar figures here are API-equivalent *value*, not spend —
//! always labelled estimated. Token counts are exact.

use crate::creds;
use crate::model::{Authority, DayPoint, Heatmap, Segment, TokenBreakdown, Totals, Usage, WinStat, Windows};
use crate::pricing::{self, TokenCounts};
use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike, Utc};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::SystemTime;

/// One priced assistant turn, reduced to just what the dashboard aggregates.
#[derive(Clone)]
struct Record {
    /// Days since the Common Era epoch, local time — the daily-bucket key.
    date_ord: i32,
    weekday: usize,
    hour: usize,
    model: String,
    project: String,
    session: String,
    /// `Some(hash)` of `message.id:requestId` for de-duplication; `None` when
    /// either id is missing, in which case the turn is always counted.
    dedup: Option<u64>,
    cost: f64,
    input: u64,
    output: u64,
    cache_write: u64,
    cache_read: u64,
    web_search: u64,
    web_fetch: u64,
}

/// Cached parse of one transcript file, keyed by size+mtime so unchanged files
/// are never re-read.
struct FileEntry {
    size: u64,
    mtime: SystemTime,
    records: Vec<Record>,
}

/// Owns the per-file cache across refreshes. The first scan reads every
/// transcript (~seconds for a large corpus); later scans re-read only files
/// whose size or mtime changed.
pub struct ClaudeHistory {
    cache: HashMap<PathBuf, FileEntry>,
}

impl ClaudeHistory {
    pub fn new() -> Self {
        ClaudeHistory { cache: HashMap::new() }
    }

    pub fn scan(&mut self) -> Usage {
        let root = creds::home().join(".claude/projects");
        if !root.is_dir() {
            return unavailable("no ~/.claude/projects — run `claude` to sign in");
        }

        let mut files = Vec::new();
        collect_jsonl(&root, &mut files);
        if files.is_empty() {
            return unavailable("no Claude transcripts on this machine yet");
        }

        let present: HashSet<PathBuf> = files.iter().cloned().collect();
        self.cache.retain(|path, _| present.contains(path));

        for path in &files {
            let Ok(meta) = fs::metadata(path) else { continue };
            let size = meta.len();
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let unchanged = self
                .cache
                .get(path)
                .is_some_and(|e| e.size == size && e.mtime == mtime);
            if unchanged {
                continue;
            }
            let records = parse_file(path);
            self.cache.insert(path.clone(), FileEntry { size, mtime, records });
        }

        self.aggregate()
    }

    fn aggregate(&self) -> Usage {
        let today = Local::now().date_naive().num_days_from_ce();
        let mut seen: HashSet<u64> = HashSet::new();
        let mut sessions: HashSet<String> = HashSet::new();
        let mut days: HashSet<i32> = HashSet::new();
        let mut daily: HashMap<i32, DayPoint> = HashMap::new();
        let mut by_model: HashMap<String, Segment> = HashMap::new();
        let mut by_project: HashMap<String, Segment> = HashMap::new();
        let mut heat = Heatmap::default();
        let mut totals = Totals::default();
        let mut tokens = TokenBreakdown::default();
        let mut windows = Windows::default();
        let (mut first, mut last) = (i32::MAX, i32::MIN);

        for entry in self.cache.values() {
            for r in &entry.records {
                if let Some(id) = r.dedup {
                    if !seen.insert(id) {
                        continue;
                    }
                }
                let msg_tokens = r.input + r.output + r.cache_write + r.cache_read;

                totals.cost_usd += r.cost;
                totals.input += r.input;
                totals.output += r.output;
                totals.cache_write += r.cache_write;
                totals.cache_read += r.cache_read;
                totals.messages += 1;
                totals.web_search += r.web_search;
                totals.web_fetch += r.web_fetch;
                sessions.insert(r.session.clone());
                days.insert(r.date_ord);
                first = first.min(r.date_ord);
                last = last.max(r.date_ord);

                tokens.input += r.input;
                tokens.output += r.output;
                tokens.cache_write += r.cache_write;
                tokens.cache_read += r.cache_read;

                let day = daily.entry(r.date_ord).or_insert(DayPoint { date_ord: r.date_ord, ..Default::default() });
                day.cost += r.cost;
                day.tokens += msg_tokens;
                day.messages += 1;

                accumulate(by_model.entry(r.model.clone()).or_default(), &r.model, r, msg_tokens);
                accumulate(by_project.entry(r.project.clone()).or_default(), &r.project, r, msg_tokens);

                let slot = &mut heat.counts[r.weekday][r.hour];
                *slot += 1;
                heat.max = heat.max.max(*slot);

                let win = |w: &mut WinStat| {
                    w.cost += r.cost;
                    w.tokens += msg_tokens;
                    w.messages += 1;
                };
                if r.date_ord == today {
                    win(&mut windows.today);
                }
                if r.date_ord > today - 7 {
                    win(&mut windows.seven);
                }
                if r.date_ord > today - 30 {
                    win(&mut windows.thirty);
                }
            }
        }

        if totals.messages == 0 {
            return unavailable("Claude transcripts contain no priced usage yet");
        }

        totals.sessions = sessions.len() as u64;
        totals.active_days = days.len() as u64;
        totals.first_day = ord_to_date(first);
        totals.last_day = ord_to_date(last);

        let mut daily: Vec<DayPoint> = daily.into_values().collect();
        daily.sort_by_key(|d| d.date_ord);

        Usage {
            scope: "Claude".into(),
            authority: Authority::Estimated,
            source: "~/.claude/projects · local history".into(),
            totals,
            windows,
            daily,
            by_model: sorted_segments(by_model),
            by_project: sorted_segments(by_project),
            by_provider: Vec::new(),
            tokens,
            heatmap: heat,
            error: None,
        }
    }
}

fn accumulate(seg: &mut Segment, label: &str, r: &Record, msg_tokens: u64) {
    if seg.label.is_empty() {
        seg.label = label.to_string();
    }
    seg.cost += r.cost;
    seg.tokens += msg_tokens;
    seg.messages += 1;
}

/// Segments sorted by cost desc, then tokens desc — the order tables and bars
/// want.
fn sorted_segments(map: HashMap<String, Segment>) -> Vec<Segment> {
    let mut v: Vec<Segment> = map.into_values().collect();
    v.sort_by(|a, b| b.cost.total_cmp(&a.cost).then(b.tokens.cmp(&a.tokens)));
    v
}

fn parse_file(path: &PathBuf) -> Vec<Record> {
    let Ok(file) = fs::File::open(path) else { return Vec::new() };
    let mut records = Vec::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if line.is_empty() {
            continue;
        }
        if let Some(record) = parse_line(&line) {
            records.push(record);
        }
    }
    records
}

fn parse_line(line: &str) -> Option<Record> {
    let json: Value = serde_json::from_str(line).ok()?;
    if json.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let message = json.get("message")?;
    let model = message.get("model").and_then(Value::as_str).unwrap_or("");
    if model.is_empty() || model == "<synthetic>" {
        return None;
    }
    let usage = message.get("usage")?;

    let u = |ptr: &str| usage.pointer(ptr).and_then(Value::as_u64).unwrap_or(0);
    let input = u("/input_tokens");
    let output = u("/output_tokens");
    let cache_write = u("/cache_creation_input_tokens");
    let cache_read = u("/cache_read_input_tokens");
    if input + output + cache_write + cache_read == 0 {
        return None;
    }
    let e5m = u("/cache_creation/ephemeral_5m_input_tokens");
    let e1h = u("/cache_creation/ephemeral_1h_input_tokens");
    let (write_5m, write_1h) = if e5m + e1h == cache_write && cache_write > 0 {
        (e5m, e1h)
    } else {
        (cache_write, 0)
    };

    let cost = pricing::rate_for(model).cost(&TokenCounts {
        input,
        output,
        cache_write_5m: write_5m,
        cache_write_1h: write_1h,
        cache_read,
    });

    let ts = parse_ts(json.get("timestamp"))?;
    let local = ts.with_timezone(&Local);

    Some(Record {
        date_ord: local.date_naive().num_days_from_ce(),
        weekday: local.weekday().num_days_from_monday() as usize,
        hour: local.hour() as usize,
        model: pricing::short_name(model),
        project: project_name(&json),
        session: json.get("sessionId").and_then(Value::as_str).unwrap_or("").to_string(),
        dedup: dedup_key(message, &json),
        cost,
        input,
        output,
        cache_write,
        cache_read,
        web_search: u("/server_tool_use/web_search_requests"),
        web_fetch: u("/server_tool_use/web_fetch_requests"),
    })
}

/// Hash of `message.id:requestId`; `None` when either is missing so the turn is
/// never suppressed by a partial key.
fn dedup_key(message: &Value, json: &Value) -> Option<u64> {
    let id = message.get("id").and_then(Value::as_str)?;
    let request = json.get("requestId").and_then(Value::as_str)?;
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    ":".hash(&mut hasher);
    request.hash(&mut hasher);
    Some(hasher.finish())
}

/// The working directory's basename is the friendliest project label.
fn project_name(json: &Value) -> String {
    if let Some(cwd) = json.get("cwd").and_then(Value::as_str) {
        let name = cwd.trim_end_matches('/').rsplit('/').next().unwrap_or(cwd);
        if !name.is_empty() {
            return name.to_string();
        }
    }
    "unknown".into()
}

fn parse_ts(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let raw = value?.as_str()?;
    DateTime::parse_from_rfc3339(raw).ok().map(|dt| dt.with_timezone(&Utc))
}

fn ord_to_date(ord: i32) -> Option<NaiveDate> {
    if ord == i32::MAX || ord == i32::MIN {
        None
    } else {
        NaiveDate::from_num_days_from_ce_opt(ord)
    }
}

fn collect_jsonl(dir: &PathBuf, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
}

fn unavailable(message: &str) -> Usage {
    Usage {
        scope: "Claude".into(),
        authority: Authority::Unavailable,
        source: "~/.claude/projects · unavailable".into(),
        error: Some(message.to_string()),
        ..Default::default()
    }
}
