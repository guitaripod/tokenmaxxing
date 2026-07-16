//! Grok usage activity from local CLI sessions under `~/.grok/sessions`.
//! The CLI does not persist per-turn token usage on disk, so this aggregates
//! turn/message activity (counts, models, projects, heatmap) — not dollars.

use crate::creds;
use crate::model::{
    Authority, DayPoint, Heatmap, Segment, TokenBreakdown, Totals, Usage, WinStat, Windows,
};
use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

struct Record {
    date_ord: i32,
    weekday: usize,
    hour: usize,
    model: String,
    project: String,
    session: String,
    messages: u64,
}

struct FileEntry {
    size: u64,
    mtime: SystemTime,
    records: Vec<Record>,
}

pub struct GrokHistory {
    cache: HashMap<PathBuf, FileEntry>,
}

impl GrokHistory {
    pub fn new() -> Self {
        GrokHistory {
            cache: HashMap::new(),
        }
    }

    pub fn scan(&mut self) -> Usage {
        let root = creds::grok_sessions_path();
        if !root.is_dir() {
            return unavailable("no ~/.grok/sessions — run `grok` to sign in");
        }

        let mut session_dirs = Vec::new();
        collect_session_dirs(&root, &mut session_dirs);
        if session_dirs.is_empty() {
            return unavailable("no Grok sessions on this machine yet");
        }

        let present: std::collections::HashSet<PathBuf> = session_dirs.iter().cloned().collect();
        self.cache.retain(|path, _| present.contains(path));

        for dir in &session_dirs {
            let marker = dir.join("summary.json");
            let Ok(meta) = fs::metadata(&marker).or_else(|_| fs::metadata(dir)) else {
                continue;
            };
            let size = meta.len();
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let unchanged = self
                .cache
                .get(dir)
                .is_some_and(|e| e.size == size && e.mtime == mtime);
            if unchanged {
                continue;
            }
            let records = parse_session(dir);
            self.cache.insert(
                dir.clone(),
                FileEntry {
                    size,
                    mtime,
                    records,
                },
            );
        }

        self.aggregate()
    }

    fn aggregate(&self) -> Usage {
        let today = Local::now().date_naive().num_days_from_ce();
        let mut sessions: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut daily: HashMap<i32, DayPoint> = HashMap::new();
        let mut by_model: HashMap<String, Segment> = HashMap::new();
        let mut by_project: HashMap<String, Segment> = HashMap::new();
        let mut heat = Heatmap::default();
        let mut totals = Totals::default();
        let mut windows = Windows::default();
        let (mut first, mut last) = (i32::MAX, i32::MIN);

        for entry in self.cache.values() {
            for r in &entry.records {
                sessions.insert(r.session.clone());
                totals.messages += r.messages;
                first = first.min(r.date_ord);
                last = last.max(r.date_ord);

                let day = daily.entry(r.date_ord).or_insert(DayPoint {
                    date_ord: r.date_ord,
                    ..Default::default()
                });
                day.messages += r.messages;

                let model = by_model.entry(r.model.clone()).or_insert(Segment {
                    label: r.model.clone(),
                    ..Default::default()
                });
                model.messages += r.messages;

                let project = by_project.entry(r.project.clone()).or_insert(Segment {
                    label: r.project.clone(),
                    ..Default::default()
                });
                project.messages += r.messages;

                if r.weekday < 7 && r.hour < 24 {
                    heat.counts[r.weekday][r.hour] += r.messages;
                    heat.max = heat.max.max(heat.counts[r.weekday][r.hour]);
                }

                let win = |w: &mut WinStat| {
                    w.messages += r.messages;
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

        totals.sessions = sessions.len() as u64;
        totals.active_days = daily.len() as u64;
        totals.first_day = if first == i32::MAX {
            None
        } else {
            NaiveDate::from_num_days_from_ce_opt(first)
        };
        totals.last_day = if last == i32::MIN {
            None
        } else {
            NaiveDate::from_num_days_from_ce_opt(last)
        };

        let mut daily_vec: Vec<DayPoint> = daily.into_values().collect();
        daily_vec.sort_by_key(|d| d.date_ord);

        let mut models: Vec<Segment> = by_model.into_values().collect();
        models.sort_by(|a, b| b.messages.cmp(&a.messages));
        let mut projects: Vec<Segment> = by_project.into_values().collect();
        projects.sort_by(|a, b| b.messages.cmp(&a.messages));

        Usage {
            scope: "grok".into(),
            authority: Authority::Estimated,
            source: "local ~/.grok/sessions · activity".into(),
            totals,
            windows,
            daily: daily_vec,
            by_model: models,
            by_project: projects,
            by_provider: Vec::new(),
            tokens: TokenBreakdown::default(),
            heatmap: heat,
            error: None,
        }
    }
}

fn collect_session_dirs(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Workspace folder → session UUID folders inside.
        if path.join("summary.json").is_file() || path.join("events.jsonl").is_file() {
            out.push(path);
            continue;
        }
        if let Ok(inner) = fs::read_dir(&path) {
            for child in inner.flatten() {
                let child_path = child.path();
                if child_path.is_dir()
                    && (child_path.join("summary.json").is_file()
                        || child_path.join("events.jsonl").is_file()
                        || child_path.join("chat_history.jsonl").is_file())
                {
                    out.push(child_path);
                }
            }
        }
    }
}

fn parse_session(dir: &Path) -> Vec<Record> {
    let session_id = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let project = project_from_dir(dir);
    let model = summary_model(dir).unwrap_or_else(|| "grok".into());

    // Prefer turn_started events for time-bucketed activity; fall back to summary totals.
    let mut records = parse_turn_events(dir, &session_id, &project);
    if records.is_empty() {
        if let Some(rec) = parse_summary_fallback(dir, &session_id, &project, &model) {
            records.push(rec);
        }
    }
    records
}

fn project_from_dir(dir: &Path) -> String {
    // sessions/%2Fhome%2Fmarcus%2FDev%2Fquota/<uuid>
    let workspace = dir
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let decoded = urlencoding_decode(workspace);
    let path = Path::new(&decoded);
    path.file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("home")
        .to_string()
}

fn urlencoding_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn summary_model(dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(dir.join("summary.json")).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    json.get("current_model_id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn parse_turn_events(dir: &Path, session: &str, project: &str) -> Vec<Record> {
    let path = dir.join("events.jsonl");
    let Ok(file) = fs::File::open(&path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in BufReader::new(file).lines().flatten() {
        if !line.contains("turn_started") {
            continue;
        }
        let Ok(json) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if json.get("type").and_then(Value::as_str) != Some("turn_started") {
            continue;
        }
        let Some(ts) = json.get("ts").and_then(Value::as_str).and_then(parse_event_ts) else {
            continue;
        };
        let local = ts.with_timezone(&Local);
        let model = json
            .get("model_id")
            .and_then(Value::as_str)
            .unwrap_or("grok")
            .to_string();
        out.push(Record {
            date_ord: local.date_naive().num_days_from_ce(),
            weekday: local.weekday().num_days_from_monday() as usize,
            hour: local.hour() as usize,
            model: short_model(&model),
            project: project.to_string(),
            session: session.to_string(),
            messages: 1,
        });
    }
    out
}

fn parse_summary_fallback(
    dir: &Path,
    session: &str,
    project: &str,
    model: &str,
) -> Option<Record> {
    let raw = fs::read_to_string(dir.join("summary.json")).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    let messages = json
        .get("num_chat_messages")
        .or_else(|| json.get("num_messages"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .max(1);
    let ts = json
        .get("last_active_at")
        .or_else(|| json.get("updated_at"))
        .or_else(|| json.get("created_at"))
        .and_then(Value::as_str)
        .and_then(parse_event_ts)?;
    let local = ts.with_timezone(&Local);
    Some(Record {
        date_ord: local.date_naive().num_days_from_ce(),
        weekday: local.weekday().num_days_from_monday() as usize,
        hour: local.hour() as usize,
        model: short_model(model),
        project: project.to_string(),
        session: session.to_string(),
        messages,
    })
}

fn parse_event_ts(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| {
            // Some timestamps omit the offset; treat as UTC.
            chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
        })
}

fn short_model(model: &str) -> String {
    let m = model.to_ascii_lowercase();
    if m.contains("grok-4.5") || m.contains("grok-build") {
        "Grok 4.5".into()
    } else if m.contains("grok-4.3") {
        "Grok 4.3".into()
    } else if m.contains("grok-4.20") || m.contains("grok-4-20") {
        "Grok 4.20".into()
    } else if m.contains("grok-3") {
        "Grok 3".into()
    } else if m.starts_with("grok") {
        model.to_string()
    } else {
        model.to_string()
    }
}

fn unavailable(message: &str) -> Usage {
    Usage {
        scope: "grok".into(),
        authority: Authority::Unavailable,
        source: "local ~/.grok/sessions · unavailable".into(),
        error: Some(message.to_string()),
        ..Default::default()
    }
}
