//! Portable durable job facts. The queue mechanics live in the Runtime
//! adapter; owner workers supply a static, reviewed claim predicate.
use serde::{Deserialize, Serialize};

pub const DEFAULT_LEASE_MILLIS: i64 = 90_000;
pub const MAX_ATTEMPTS: i64 = 6;
const MAX_RETRY_DELAY_MILLIS: i64 = 60_000;
const MAX_ERROR_CODE_UNITS: usize = 64;
/// Retained generic worker cadence: one sequential pass per second while idle.
pub const DEFAULT_IDLE_POLL_MILLIS: u64 = 1_000;
/// Retained generic worker follow-up cadence after a completed/failed attempt.
pub const DEFAULT_BUSY_POLL_MILLIS: u64 = 10;
/// Retained lease heartbeat: `claim_until = now + 90s` every 15s.
pub const DEFAULT_LEASE_REFRESH_MILLIS: u64 = 15_000;
/// A restarted worker waits one second before retrying its first pass.
pub const DEFAULT_RESTART_BACKOFF_MILLIS: u64 = 1_000;
/// Upper bound on Holding partitions handled in one sequential pass. The
/// DatabaseManifest admits at most 22 slots; 64 leaves deliberate headroom.
pub const DEFAULT_MAX_EVENTS_PER_PASS: usize = 64;
pub const MAX_EVENTS_PER_PASS_LIMIT: usize = 1_024;

/// The exact durable retry decision for a failed attempt.
///
/// `attempts` is the value returned by the claim after incrementing, exactly as
/// the retained worker computes `Math.min(60_000, 1000 * 2 ** Math.min(attempts, 6))`
/// and places the sixth failed attempt into the dead-letter queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutboxRetrySchedule {
    pub next_attempt_at: i64,
    pub dead_letter_at: Option<i64>,
}

pub fn retry_schedule(attempts: i64, now: i64) -> OutboxRetrySchedule {
    OutboxRetrySchedule {
        next_attempt_at: now.saturating_add(retry_delay_millis(attempts)),
        dead_letter_at: (attempts >= MAX_ATTEMPTS).then_some(now),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct OutboxJob {
    pub id: String,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub aggregate_version: i64,
    pub payload_json: String,
    pub attempts: i64,
    pub lease_token: String,
}

/// ECMAScript `String.prototype.slice(0, 64)` counts UTF-16 code units. Queue
/// errors persist the same bounded prefix as the retained TypeScript worker.
pub fn truncate_error_code(value: &str) -> String {
    let mut end = 0;
    let mut units = 0;
    for (index, character) in value.char_indices() {
        let width = character.len_utf16();
        if units + width > MAX_ERROR_CODE_UNITS {
            break;
        }
        units += width;
        end = index + character.len_utf8();
    }
    value[..end].to_owned()
}

/// The retained worker retries after `min(60s, 1s * 2^attempts)` and places the
/// sixth failed attempt into the dead-letter queue.
pub fn retry_delay_millis(attempts: i64) -> i64 {
    (1_000_i64 * (1_i64 << attempts.clamp(0, MAX_ATTEMPTS))).min(MAX_RETRY_DELAY_MILLIS)
}
