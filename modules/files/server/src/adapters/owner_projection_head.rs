//! Native-owned owner-projection heads for the Files read tail.
//!
//! The three TypeScript `*_OWNER_PROJECTION_SQL` literals were the canonical
//! column projections for attachments, links and sets. Their exact bytes now
//! live in this owner's JSON contract, bound into Rust with `include_str!` and
//! read by the TypeScript adapter through its native contract port. No
//! statement token remains in TypeScript; each consumer still prepares its own
//! statement with its own guards and binds on its caller's transaction.

/// Native-owned SQL contract for the three Files owner projections.
pub const OWNER_PROJECTION_HEAD_SOURCES_JSON: &str =
    include_str!("owner-projection-head-sources.json");

#[derive(serde::Deserialize)]
struct OwnerProjectionSources {
    attachments: String,
    #[serde(rename = "attachmentLinks")]
    attachment_links: String,
    #[serde(rename = "attachmentSets")]
    attachment_sets: String,
}

static SOURCES: std::sync::OnceLock<OwnerProjectionSources> = std::sync::OnceLock::new();

fn sources() -> &'static OwnerProjectionSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(OWNER_PROJECTION_HEAD_SOURCES_JSON)
            .expect("files owner projection head SQL contract must be valid JSON")
    })
}

/// Frozen attachments column projection.
pub fn attachments_projection() -> &'static str {
    &sources().attachments
}

/// Frozen attachment_links column projection.
pub fn attachment_links_projection() -> &'static str {
    &sources().attachment_links
}

/// Frozen attachment_sets column projection.
pub fn attachment_sets_projection() -> &'static str {
    &sources().attachment_sets
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    const TS_HELPER: &str = include_str!("owner-projection-dedupe-sql.ts");

    fn sha(value: &str) -> String {
        hex::encode(Sha256::digest(value.as_bytes()))
    }

    #[test]
    fn contract_is_byte_exact_to_the_retired_typescript_literals() {
        for (sql, hash, len, table) in [
            (
                attachments_projection(),
                "f6866e2eeec1ac46c5b12da98afcd528cda55808f7fbabfb51c4bbb0e6fbf7d6",
                415,
                "attachments",
            ),
            (
                attachment_links_projection(),
                "083488efc2f6fb5b7000edef251890fba67770fefd6e8819f3b2623b1a4171dd",
                244,
                "attachment_links",
            ),
            (
                attachment_sets_projection(),
                "3b9871ec69b9e9746557a4b7c7b2a595e8f1af83f9a1427d12774e43f79582b4",
                412,
                "attachment_sets",
            ),
        ] {
            assert_eq!(sha(sql), hash);
            assert_eq!(sql.len(), len);
            assert_eq!(sql.matches("SELECT ").count(), 1);
            assert!(sql.starts_with("SELECT \"id\","));
            assert!(sql.ends_with(&format!(" FROM {table}")));
            assert!(!sql.contains("INSERT INTO"));
            assert!(!sql.contains("DELETE FROM"));
            assert!(!sql.contains("FOR UPDATE"));
            assert!(!sql.contains("LOCK TABLE"));
        }
    }

    #[test]
    fn typescript_helper_carries_no_statement_token() {
        for token in ["SELECT", "INSERT", "DELETE", "UPDATE "] {
            assert!(!TS_HELPER.contains(token));
        }
        assert!(TS_HELPER.contains("owner-projection-head-sources.json"));
        assert!(TS_HELPER.contains("export const ATTACHMENTS_OWNER_PROJECTION_SQL"));
        assert!(TS_HELPER.contains("export const ATTACHMENT_LINKS_OWNER_PROJECTION_SQL"));
        assert!(TS_HELPER.contains("export const ATTACHMENT_SETS_OWNER_PROJECTION_SQL"));
        assert!(!TS_HELPER.contains("prepare("));
        assert!(!TS_HELPER.contains(".bind("));
    }

    #[test]
    fn native_port_reexports_the_three_frozen_projections() {
        let port = include_str!("../public/owner_projection_head.rs");
        assert!(port.contains("pub use crate::adapters::owner_projection_head::"));
        assert!(port.contains("attachments_projection"));
        assert!(port.contains("attachment_links_projection"));
        assert!(port.contains("attachment_sets_projection"));
    }
}
