//! Pure response types and invariants for the native Legal rule-impact read.
//!
//! No SQL and no I/O belong here. The Postgres adapter binds the fixed
//! statements and the HTTP adapter transports the resulting value.

use erpx_platform_shared_server::public::RecordId;
use serde::{Deserialize, Serialize};

/// The original owner `LIMIT 500`, owned here as a Rust invariant.
pub const IMPACT_LIMIT: i64 = 500;

/// Stable owner operation name used by the transport and tests.
pub const RULE_IMPACT_OPERATION: &str = "legal.rule-impact.read";

/// Notice returned verbatim by the retired TypeScript operation. Payload
/// parity requires this exact text.
pub const NOTICE: &str = "Only accessible linked records are shown. Existing deadlines remain unchanged until their owner reviews and updates each record.";

/// A registered, exact Holding target. Composition injects these; duplicate or
/// malformed targets are rejected before any route is installed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleImpactTarget {
    pub host: String,
    pub holding_slug: String,
}

/// The owner's point witness for the selected rule. Only `kind` and `version`
/// are read; the record's other data never crosses this boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleHead {
    pub kind: String,
    pub version: i64,
}

/// One impacted linked record. Field names mirror the former SQL projection
/// and are emitted unchanged, so the native client adds no projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuleImpactItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub state: String,
    pub due_date: Option<String>,
    pub version: i64,
}

/// Complete owner-local read result. Every field is authoritative.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuleImpactRead {
    pub rule_id: String,
    pub rule_version: i64,
    pub items: Vec<RuleImpactItem>,
    pub notice: String,
}

/// Registered Holding slugs are exact lowercase tokens, never free request text.
pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Item identities are bounded opaque record ids.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_freeform_targets_and_ids() {
        assert!(valid_slug("acme"));
        assert!(valid_slug("north-star_1"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("a/b"));
        assert!(valid_id("item-1"));
        assert!(valid_id("doc.7"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id(&"x".repeat(201)));
    }

    #[test]
    fn notice_is_payload_parity() {
        assert_eq!(
            NOTICE,
            "Only accessible linked records are shown. Existing deadlines remain unchanged until their owner reviews and updates each record."
        );
    }
}
