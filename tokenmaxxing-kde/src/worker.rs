use crate::model::{Authority, Dashboard, Snapshot};
use crate::providers::claude_history::ClaudeHistory;
use crate::providers::grok_history::GrokHistory;
use crate::providers::{anthropic, grok, opencode};
use chrono::Local;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub enum ToUi {
    Dashboard(Box<Dashboard>),
}

pub enum FromUi {
    RefreshNow,
}

const TICK: Duration = Duration::from_secs(30);
const USER_AGENT: &str = concat!(
    "tokenmaxxing/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/guitaripod/tokenmaxxing)"
);

pub fn spawn(to_ui: async_channel::Sender<ToUi>, from_ui: Receiver<FromUi>) {
    std::thread::Builder::new()
        .name("tokenmaxxing-worker".into())
        .spawn(move || run(&to_ui, &from_ui))
        .expect("spawn worker thread");
}

/// Poll local usage every tick. Each live endpoint is paced by its own
/// cooldown (2 min normally, ≥5 min after HTTP 429; both seed from their disk
/// caches on failure). Manual refresh clears cooldowns for one try.
fn run(to_ui: &async_channel::Sender<ToUi>, from_ui: &Receiver<FromUi>) {
    let client = build_client();
    let mut claude_history = ClaudeHistory::new();
    let mut grok_history = GrokHistory::new();

    let mut claude = LiveCache::new(claude_placeholder);
    let mut grok = LiveCache::new(grok_placeholder);
    // Seed from disk caches immediately so the first paint has rings even
    // when an endpoint is currently 429'ing.
    {
        let result = anthropic::fetch(&client);
        claude.apply(result.snapshot, result.cooldown, result.fresh);
        let result = grok::fetch(&client);
        grok.apply(result.snapshot, result.cooldown, result.fresh);
    }

    loop {
        if claude.due() {
            let result = anthropic::fetch(&client);
            claude.apply(result.snapshot, result.cooldown, result.fresh);
        }
        if grok.due() {
            let result = grok::fetch(&client);
            grok.apply(result.snapshot, result.cooldown, result.fresh);
        }

        let dashboard = Dashboard {
            claude_quota: claude.snapshot(),
            claude_usage: claude_history.scan(),
            grok_quota: grok.snapshot(),
            grok_usage: grok_history.scan(),
            opencode_quota: opencode::fetch(),
            opencode_usage: opencode::usage(),
            generated_at: Local::now(),
        };

        if to_ui
            .send_blocking(ToUi::Dashboard(Box::new(dashboard)))
            .is_err()
        {
            break;
        }

        match from_ui.recv_timeout(TICK) {
            Ok(FromUi::RefreshNow) => {
                // One more attempt now; if an endpoint is still 429, fetch()
                // will re-impose the floor cooldown itself.
                claude.force_due();
                grok.force_due();
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

struct LiveCache {
    current: Option<Snapshot>,
    next_fetch: Instant,
    placeholder: fn() -> Snapshot,
}

impl LiveCache {
    fn new(placeholder: fn() -> Snapshot) -> Self {
        Self {
            current: None,
            next_fetch: Instant::now(),
            placeholder,
        }
    }

    fn due(&self) -> bool {
        Instant::now() >= self.next_fetch
    }

    fn force_due(&mut self) {
        self.next_fetch = Instant::now();
    }

    fn apply(&mut self, snapshot: Snapshot, cooldown: Duration, fresh: bool) {
        // Always prefer a snapshot that has gauges over an empty OFFLINE card
        // when we already hold a good reading.
        if fresh || !snapshot.gauges.is_empty() || self.current.is_none() {
            self.current = Some(snapshot);
        } else if let Some(cur) = self.current.as_mut() {
            // Keep rings; just annotate that we're still cooling down.
            if snapshot.authority != Authority::Live {
                cur.source = snapshot.source.clone();
                if let Some(note) = snapshot.note {
                    cur.note = Some(note);
                }
            }
        }
        self.next_fetch = Instant::now() + cooldown;
    }

    fn snapshot(&self) -> Snapshot {
        self.current.clone().unwrap_or_else(self.placeholder)
    }
}

fn claude_placeholder() -> Snapshot {
    Snapshot {
        provider_id: "anthropic".into(),
        provider_name: "Claude".into(),
        subtitle: "Claude".into(),
        authority: Authority::Unavailable,
        source: "api.anthropic.com · starting".into(),
        gauges: Vec::new(),
        details: Vec::new(),
        note: None,
        error: Some("waiting for first Claude reading".into()),
        spend: None,
    }
}

fn grok_placeholder() -> Snapshot {
    Snapshot {
        provider_id: "xai".into(),
        provider_name: "Grok".into(),
        subtitle: "Grok".into(),
        authority: Authority::Unavailable,
        source: "cli-chat-proxy.grok.com · starting".into(),
        gauges: Vec::new(),
        details: Vec::new(),
        note: None,
        error: Some("waiting for first Grok reading".into()),
        spend: None,
    }
}

/// One-shot build of the full dashboard for the headless `--export` path.
pub fn snapshot_once() -> Dashboard {
    let client = build_client();
    let claude = anthropic::fetch(&client);
    Dashboard {
        claude_quota: claude.snapshot,
        claude_usage: ClaudeHistory::new().scan(),
        grok_quota: grok::fetch(&client).snapshot,
        grok_usage: GrokHistory::new().scan(),
        opencode_quota: opencode::fetch(),
        opencode_usage: opencode::usage(),
        generated_at: Local::now(),
    }
}

fn build_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(20))
        .build()
        .expect("build http client")
}
