//! Native-owned scalar projection heads for the Files read tail.
//!
//! The three TypeScript heads (`DISTINCT set_row.`, `event.id,` and
//! `source_state`) were the last statement tokens in their helpers. Their exact
//! bytes now live in this owner's JSON contract, bound into Rust with
//! `include_str!` and read by the TypeScript adapters through their native
//! contract port. Callers keep their own FROM/WHERE/ORDER BY tails, binds and
//! transaction; no statement token remains in TypeScript.

/// Native-owned SQL contract for the three scalar projection heads.
pub const PROJECTION_HEAD_SOURCES_JSON: &str =
    include_str!("projection-head-sources.json");

#[derive(serde::Deserialize)]
struct ProjectionHeadSources {
    #[serde(rename = "distinctSetRow")]
    distinct_set_row: String,
    #[serde(rename = "eventId")]
    event_id: String,
    #[serde(rename = "sourceState")]
    source_state: String,
}

static SOURCES: std::sync::OnceLock<ProjectionHeadSources> = std::sync::OnceLock::new();

fn sources() -> &'static ProjectionHeadSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(PROJECTION_HEAD_SOURCES_JSON)
            .expect("files scalar projection head SQL contract must be valid JSON")
    })
}

/// Frozen DISTINCT set-row projection head.
pub fn distinct_set_row() -> &'static str {
    &sources().distinct_set_row
}

/// Frozen audit-event id projection head.
pub fn event_id() -> &'static str {
    &sources().event_id
}

/// Frozen attachment source-state projection head.
pub fn source_state() -> &'static str {
    &sources().source_state
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
                distinct_set_row(),
                "e34ef9843e0667106b7a9500ecf72e08ba0173b1449bb1b72ae61ef936fa04ff",
                24,
                "SELECT DISTINCT set_row.",
            ),
            (
                event_id(),
                "6f47869b2f9aeabe91dd699ec8dc50e1faa133bdd178e9623ce840dfc4bcfaf4",
                16,
                "SELECT event.id,",
            ),
            (
                source_state(),
                "192eae6b027b37d4af0d94d61f87b92417780aa3f14ef2e9481834b8f2a7f5e4",
                19,
                "SELECT source_state",
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
    fn typescript_helpers_carry_no_statement_token() {
        for helper in [
            include_str!("read-tail-r50-source-sql.ts"),
            include_str!("read-tail-r52-source-sql.ts"),
            include_str!("read-tail-r53-source-sql.ts"),
        ] {
            for token in ["SELECT", "INSERT", "DELETE", "UPDATE "] {
                assert!(!helper.contains(token));
            }
            assert!(helper.contains("projection-head-sources.json"));
            assert!(!helper.contains("prepare("));
            assert!(!helper.contains(".bind("));
        }
    }

    #[test]
    fn native_port_reexports_the_three_frozen_heads() {
        let port = include_str!("../public/projection_head.rs");
        assert!(port.contains("pub use crate::adapters::projection_head::"));
        for symbol in ["distinct_set_row", "event_id", "source_state"] {
            assert!(port.contains(symbol));
        }
    }
}
