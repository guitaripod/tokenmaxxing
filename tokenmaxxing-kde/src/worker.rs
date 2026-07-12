use crate::model::{Authority, Dashboard, Snapshot};
use crate::providers::claude_history::ClaudeHistory;
use crate::providers::{anthropic, opencode};
use chrono::Local;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

pub enum ToUi {
    Dashboard(Box<Dashboard>),
}

pub enum FromUi {
    RefreshNow,
}

const TICK: Duration = Duration::from_secs(30);
const ANTHROPIC_EVERY: u64 = 4;
const USER_AGENT: &str = concat!("tokenmaxxing/", env!("CARGO_PKG_VERSION"), " (+https://github.com/guitaripod/tokenmaxxing)");

pub fn spawn(to_ui: async_channel::Sender<ToUi>, from_ui: Receiver<FromUi>) {
    std::thread::Builder::new()
        .name("tokenmaxxing-worker".into())
        .spawn(move || run(&to_ui, &from_ui))
        .expect("spawn worker thread");
}

/// Poll cheap local usage (opencode DB, incremental Claude transcript scan)
/// every tick and the rate-limited Anthropic live endpoint every fourth tick
/// (~2 min), forcing all sources on a manual refresh.
fn run(to_ui: &async_channel::Sender<ToUi>, from_ui: &Receiver<FromUi>) {
    let client = build_client();
    let mut history = ClaudeHistory::new();
    let mut tick: u64 = 0;
    let mut cached_anthropic: Option<Snapshot> = None;
    let mut last_good: Option<Snapshot> = None;
    let mut healthy = true;

    loop {
        // Fetch the live quota every few ticks while healthy, but every tick while
        // recovering from a failure. Between fetches, reuse the cached snapshot.
        let due = cached_anthropic.is_none() || if healthy { tick % ANTHROPIC_EVERY == 0 } else { true };
        let claude_quota = if due {
            let (snapshot, fresh) = fetch_claude(&client, &mut last_good);
            healthy = fresh;
            cached_anthropic = Some(snapshot.clone());
            snapshot
        } else {
            cached_anthropic.clone().expect("anthropic snapshot cached")
        };

        let dashboard = Dashboard {
            claude_quota,
            claude_usage: history.scan(),
            opencode_quota: opencode::fetch(),
            opencode_usage: opencode::usage(),
            generated_at: Local::now(),
        };

        if to_ui.send_blocking(ToUi::Dashboard(Box::new(dashboard))).is_err() {
            break;
        }
        tick = tick.wrapping_add(1);

        match from_ui.recv_timeout(TICK) {
            Ok(FromUi::RefreshNow) => tick = 0,
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// Fetch the live Claude quota with a quick retry, and — crucially — fall back to
/// the last successful snapshot on a transient failure (429, network blip, a
/// mid-refresh token race) instead of blanking the card to OFFLINE. Returns the
/// snapshot and whether it was a fresh live read. Only shows OFFLINE when no good
/// snapshot has ever been obtained.
fn fetch_claude(client: &reqwest::blocking::Client, last_good: &mut Option<Snapshot>) -> (Snapshot, bool) {
    let mut last_fail: Option<Snapshot> = None;
    for attempt in 0..2 {
        let snapshot = anthropic::fetch(client);
        if snapshot.authority == Authority::Live {
            *last_good = Some(snapshot.clone());
            return (snapshot, true);
        }
        last_fail = Some(snapshot);
        if attempt == 0 {
            std::thread::sleep(Duration::from_secs(3));
        }
    }
    match last_good.clone() {
        Some(good) => (mark_cached(good), false),
        None => (last_fail.expect("at least one fetch attempt"), false),
    }
}

/// Keep the LIVE data but note in the source line that it's a cached read being
/// retried, so the number is honest about its freshness.
fn mark_cached(mut snapshot: Snapshot) -> Snapshot {
    snapshot.source = "api.anthropic.com · live (cached, retrying)".into();
    snapshot
}

/// One-shot build of the full dashboard for the headless `--export` path.
pub fn snapshot_once() -> Dashboard {
    let client = build_client();
    Dashboard {
        claude_quota: anthropic::fetch(&client),
        claude_usage: ClaudeHistory::new().scan(),
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
