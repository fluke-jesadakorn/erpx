//! Portable durable job facts. The queue mechanics live in the Runtime
//! adapter; owner workers supply a static, reviewed claim predicate.
use serde::{Deserialize, Serialize};

pub const DEFAULT_LEASE_MILLIS: i64 = 90_000;
pub const MAX_ATTEMPTS: i64 = 6;
const MAX_RETRY_DELAY_MILLIS: i64 = 60_000;
const MAX_ERROR_CODE_UNITS: usize = 64;

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
