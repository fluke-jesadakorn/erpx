//! Native-owned upload/bind witness heads for the Files read tail.
//!
//! The three TypeScript `*_HEAD_SQL` literals were the shared read-only
//! predicates for upload-intent presence, attachment-by-id and scan-attempt
//! presence. Their exact bytes now live in this owner's JSON contract, bound
//! into Rust with `include_str!` and read by the TypeScript adapter through its
//! native contract port. Each call site keeps its own prefix, alias, state tail,
//! binds and transaction; no statement token remains in TypeScript.

/// Native-owned SQL contract for the three Files witness heads.
pub const WITNESS_HEAD_SOURCES_JSON: &str = include_str!("witness-head-sources.json");

#[derive(serde::Deserialize)]
struct WitnessHeadSources {
    #[serde(rename = "uploadIntentsExists")]
    upload_intents_exists: String,
    #[serde(rename = "attachmentStarId")]
    attachment_star_id: String,
    #[serde(rename = "scanAttemptsOrExists")]
    scan_attempts_or_exists: String,
}

static SOURCES: std::sync::OnceLock<WitnessHeadSources> = std::sync::OnceLock::new();

fn sources() -> &'static WitnessHeadSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(WITNESS_HEAD_SOURCES_JSON)
            .expect("files witness head SQL contract must be valid JSON")
    })
}

/// Frozen upload-intent absence/existence predicate head.
pub fn upload_intents_exists() -> &'static str {
    &sources().upload_intents_exists
}

/// Frozen unqualified attachment-by-id projection head.
pub fn attachment_star_id() -> &'static str {
    &sources().attachment_star_id
}

/// Frozen scan-attempt OR-EXISTS head.
pub fn scan_attempts_or_exists() -> &'static str {
    &sources().scan_attempts_or_exists
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
                upload_intents_exists(),
                "560ec6b0b217da22318a4017afecf1d406a24d831c88963b6e21d506dfae79f2",
                42,
                "EXISTS(SELECT 1 FROM upload_intents WHERE ",
            ),
            (
                attachment_star_id(),
                "7c94985ae3e467cbac01d6528e74a58109ec8da120560b9852d647daf24d567f",
                36,
                "SELECT * FROM attachments WHERE id=?",
            ),
            (
                scan_attempts_or_exists(),
                "98857b768050e0d3446d1957739be4aca508a30073355a49ace70d3fb0901a86",
                49,
                "OR EXISTS(SELECT 1 FROM attachment_scan_attempts ",
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
        let helper = include_str!("read-tail-r44-source-sql.ts");
        for token in ["SELECT", "INSERT", "DELETE", "UPDATE "] {
            assert!(!helper.contains(token));
        }
        assert!(helper.contains("witness-head-sources.json"));
        assert!(!helper.contains("prepare("));
        assert!(!helper.contains(".bind("));
    }

    #[test]
    fn native_port_reexports_the_three_frozen_heads() {
        let port = include_str!("../public/witness_head.rs");
        assert!(port.contains("pub use crate::adapters::witness_head::"));
        for symbol in ["upload_intents_exists", "attachment_star_id", "scan_attempts_or_exists"] {
            assert!(port.contains(symbol));
        }
    }
}
