use crate::model::Snapshot;
use std::time::Duration;

/// Result of a live quota fetch, including how long the worker should wait
/// before hitting the endpoint again.
pub struct FetchResult {
    pub snapshot: Snapshot,
    pub cooldown: Duration,
    pub fresh: bool,
}

pub mod anthropic;
pub mod claude_history;
pub mod grok;
pub mod grok_history;
pub mod opencode;
pub mod opencode_remote;
