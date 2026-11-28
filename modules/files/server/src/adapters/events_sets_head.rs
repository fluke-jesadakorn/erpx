//! Native-owned attachment-events / attachment-sets heads for the Files tail.
//!
//! The two TypeScript heads were the shared read-only prefixes of the event and
//! set witnesses. Their exact bytes now live in this owner's JSON contract,
//! bound into Rust with `include_str!` and read by the TypeScript adapter through
//! its native contract port. Each call site keeps its own predicate/tail, binds
//! and transaction; no statement token remains in TypeScript.

/// Native-owned SQL contract for the two event/set heads.
pub const EVENTS_SETS_HEAD_SOURCES_JSON: &str =
    include_str!("events-sets-head-sources.json");

#[derive(serde::Deserialize)]
struct EventsSetsSources {
    #[serde(rename = "attachmentEventsWhere")]
    attachment_events_where: String,
    #[serde(rename = "attachmentSetsStarWhere")]
    attachment_sets_star_where: String,
}

static SOURCES: std::sync::OnceLock<EventsSetsSources> = std::sync::OnceLock::new();

fn sources() -> &'static EventsSetsSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(EVENTS_SETS_HEAD_SOURCES_JSON)
            .expect("files event/set head SQL contract must be valid JSON")
    })
}

/// Frozen unaliased attachment-events witness head.
pub fn attachment_events_where() -> &'static str {
    &sources().attachment_events_where
}

/// Frozen attachment-sets star projection head.
pub fn attachment_sets_star_where() -> &'static str {
    &sources().attachment_sets_star_where
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn sha(value: &str) -> String {
        hex::encode(Sha256::digest(value.as_bytes()))
    }

    #[test]
    fn contract_is_byte_exact_to_the_retired_typescript_literals() {
        for (sql, hash, len, expected) in [
            (
                attachment_events_where(),
                "7ad49cdd02f1f6e41d2813b289318c83775d66a1face9b0876a708d1f99e0981",
                38,
                "SELECT 1 FROM attachment_events WHERE ",
            ),
            (
                attachment_sets_star_where(),
                "faf86a30a11aeb95423273b1dd3d78a6ecb6d32b94c6c05ccdf3867c5087dccd",
                36,
                "SELECT * FROM attachment_sets WHERE ",
            ),
        ] {
            assert_eq!(sha(sql), hash);
            assert_eq!(sql.len(), len);
            assert_eq!(sql, expected);
            assert_eq!(sql.matches("SELECT ").count(), 1);
            assert!(!sql.contains("INSERT INTO"));
            assert!(!sql.contains("DELETE FROM"));
            assert!(!sql.contains("UPDATE "));
            assert!(!sql.contains("FOR UPDATE"));
        }
    }

    #[test]
    fn typescript_helper_carries_no_statement_token() {
        let helper = include_str!("read-tail-r45-source-sql.ts");
        for token in ["SELECT", "INSERT", "DELETE", "UPDATE "] {
            assert!(!helper.contains(token));
        }
        assert!(helper.contains("events-sets-head-sources.json"));
        assert!(!helper.contains("prepare("));
        assert!(!helper.contains(".bind("));
    }

    #[test]
    fn native_port_reexports_both_frozen_heads() {
        let port = include_str!("../public/events_sets_head.rs");
        assert!(port.contains("pub use crate::adapters::events_sets_head::"));
        assert!(port.contains("attachment_events_where"));
        assert!(port.contains("attachment_sets_star_where"));
    }
}
