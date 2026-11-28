//! Native-owned orphan-preservation presence heads for the Files read tail.
//!
//! The two TypeScript `ORPHAN_PRESERVATION_*_SQL` literals were the read-only
//! witnesses for the preservation table and its pending-restoration state.
//! Their exact bytes now live in this owner's JSON contract, bound into Rust
//! with `include_str!` and read by the TypeScript adapter through its native
//! contract port. No statement token remains in TypeScript; every call site
//! still prepares its own statement with its own bind on its caller's database.

/// Native-owned SQL contract for the orphan-preservation witnesses.
pub const ORPHAN_PRESERVATION_HEAD_SOURCES_JSON: &str =
    include_str!("orphan-preservation-head-sources.json");

#[derive(serde::Deserialize)]
struct OrphanPreservationSources {
    presence: String,
    pending: String,
}

static SOURCES: std::sync::OnceLock<OrphanPreservationSources> = std::sync::OnceLock::new();

fn sources() -> &'static OrphanPreservationSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(ORPHAN_PRESERVATION_HEAD_SOURCES_JSON)
            .expect("files orphan-preservation head SQL contract must be valid JSON")
    })
}

/// Frozen preservation-table presence probe.
pub fn preservation_present() -> &'static str {
    &sources().presence
}

/// Frozen pending-restoration witness (one `object_key` bind at each call site).
pub fn preservation_pending() -> &'static str {
    &sources().pending
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    const TS_HELPER: &str = include_str!("preservation-presence-dedupe-sql.ts");

    fn sha(value: &str) -> String {
        hex::encode(Sha256::digest(value.as_bytes()))
    }

    #[test]
    fn contract_is_byte_exact_to_the_retired_typescript_literals() {
        for (sql, hash, len, selects) in [
            (
                preservation_present(),
                "9022c17669f8f302c5338cff0b0ced7b493c81e7e77de316e1fced2f6f7b8a9e",
                61,
                1,
            ),
            (
                preservation_pending(),
                "6b250eb0f749824ae50868d942a53e34b8dfbcc6a1271551b9510d965ecead73",
                195,
                2,
            ),
        ] {
            assert_eq!(sha(sql), hash);
            assert_eq!(sql.len(), len);
            assert_eq!(sql.matches("SELECT ").count(), selects);
            assert!(sql.starts_with("SELECT "));
            assert!(!sql.contains("INSERT INTO"));
            assert!(!sql.contains("DELETE FROM"));
            assert!(!sql.contains("UPDATE "));
            assert!(!sql.contains("FOR UPDATE"));
        }
    }

    #[test]
    fn typescript_helper_carries_no_statement_token() {
        for token in ["SELECT", "INSERT", "DELETE", "UPDATE "] {
            assert!(!TS_HELPER.contains(token));
        }
        assert!(TS_HELPER.contains("orphan-preservation-head-sources.json"));
        assert!(TS_HELPER.contains("export const ORPHAN_PRESERVATION_PRESENT_SQL"));
        assert!(TS_HELPER.contains("export const ORPHAN_PRESERVATION_PENDING_SQL"));
        assert!(!TS_HELPER.contains("prepare("));
        assert!(!TS_HELPER.contains(".bind("));
    }

    #[test]
    fn native_port_reexports_both_frozen_witnesses() {
        let port = include_str!("../public/orphan_preservation_head.rs");
        assert!(port.contains("pub use crate::adapters::orphan_preservation_head::"));
        assert!(port.contains("preservation_present"));
        assert!(port.contains("preservation_pending"));
    }
}
