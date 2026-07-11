use crate::model::Snapshot;
use crate::providers::{anthropic, opencode};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

pub enum ToUi {
    Snapshots(Vec<Snapshot>),
}

pub enum FromUi {
    RefreshNow,
}

const TICK: Duration = Duration::from_secs(30);
const ANTHROPIC_EVERY: u64 = 4;
const USER_AGENT: &str = concat!("ampere/", env!("CARGO_PKG_VERSION"), " (+https://github.com/guitaripod/quota)");

pub fn spawn(to_ui: async_channel::Sender<ToUi>, from_ui: Receiver<FromUi>) {
    std::thread::Builder::new()
        .name("ampere-worker".into())
        .spawn(move || run(&to_ui, &from_ui))
        .expect("spawn worker thread");
}

/// Poll cheap local opencode usage every tick and the rate-limited Anthropic
/// endpoint every fourth tick (~2 min), forcing both on a manual refresh.
fn run(to_ui: &async_channel::Sender<ToUi>, from_ui: &Receiver<FromUi>) {
    let client = build_client();
    let mut tick: u64 = 0;
    let mut cached_anthropic: Option<Snapshot> = None;

    loop {
        let anthropic = if tick % ANTHROPIC_EVERY == 0 || cached_anthropic.is_none() {
            let snapshot = anthropic::fetch(&client);
            cached_anthropic = Some(snapshot.clone());
            snapshot
        } else {
            cached_anthropic.clone().expect("anthropic snapshot cached")
        };
        let opencode = opencode::fetch();

        if to_ui
            .send_blocking(ToUi::Snapshots(vec![anthropic, opencode]))
            .is_err()
        {
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

fn build_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(20))
        .build()
        .expect("build http client")
}
