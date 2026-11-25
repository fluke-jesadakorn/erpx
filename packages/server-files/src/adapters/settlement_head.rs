//! Native-owned settlement/existence projection heads for the Files read tail.
//!
//! The three TypeScript heads were the shared read-only prefixes of the scoped
//! attachment preflight, the messaging settlement witness and the package
//! file-link obligation source. Their exact bytes now live in this owner's JSON
//! contract, bound into Rust with `include_str!` and read by the TypeScript
//! adapter through its native contract port. Each call site keeps its own
//! alias, column/WHERE tails, binds and transaction; no statement token remains
//! in TypeScript.

/// Native-owned SQL contract for the three settlement/projection heads.
pub const SETTLEMENT_HEAD_SOURCES_JSON: &str =
    include_str!("settlement-head-sources.json");

#[derive(serde::Deserialize)]
struct SettlementHeadSources {
    #[serde(rename = "attachmentExists")]
    attachment_exists: String,
    #[serde(rename = "messagingSettlement")]
    messaging_settlement: String,
    #[serde(rename = "packageFileLinkTarget")]
    package_file_link_target: String,
}

static SOURCES: std::sync::OnceLock<SettlementHeadSources> = std::sync::OnceLock::new();

fn sources() -> &'static SettlementHeadSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(SETTLEMENT_HEAD_SOURCES_JSON)
            .expect("files settlement head SQL contract must be valid JSON")
    })
}

/// Frozen scoped-attachment existence projection head.
pub fn attachment_exists() -> &'static str {
    &sources().attachment_exists
}

/// Frozen messaging-settlement column projection head.
pub fn messaging_settlement() -> &'static str {
    &sources().messaging_settlement
}

/// Frozen package file-link target projection head.
pub fn package_file_link_target() -> &'static str {
    &sources().package_file_link_target
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
        for (sql, hash, len, expected, selects) in [
            (
                attachment_exists(),
                "08b4c26e14a85262ac9a177a9754f5203011dfaa51aae87e5b9ba59976c53e6c",
                42,
                "SELECT EXISTS(SELECT 1 FROM attachment",
                2,
            ),
            (
                messaging_settlement(),
                "063e76012a14e6e8e4e5cd5f17144368ba286029f267fe5b4e6298f69d5c2ad5",
                32,
                "SELECT l.id link_id,s.id set_id,",
                1,
            ),
            (
                package_file_link_target(),
                "2097bd180887f8b55c61c79fcd703dd91e700e61195ae872bbed88d08bb7f01d",
                39,
                "SELECT 'file-link' kind,l.id,s.target_",
                1,
            ),
        ] {
            assert_eq!(sha(sql), hash);
            assert_eq!(sql.len(), len);
            assert_eq!(sql, expected);
            assert_eq!(sql.matches("SELECT ").count(), selects);
            assert!(!sql.contains("INSERT INTO"));
            assert!(!sql.contains("DELETE FROM"));
            assert!(!sql.contains("UPDATE "));
            assert!(!sql.contains("FOR UPDATE"));
        }
    }

    #[test]
    fn typescript_helper_carries_no_statement_token() {
        let helper = include_str!("read-tail-r48-source-sql.ts");
        for token in ["SELECT", "INSERT", "DELETE", "UPDATE "] {
            assert!(!helper.contains(token));
        }
        assert!(helper.contains("settlement-head-sources.json"));
        assert!(!helper.contains("prepare("));
        assert!(!helper.contains(".bind("));
    }

    #[test]
    fn native_port_reexports_the_three_frozen_heads() {
        let port = include_str!("../public/settlement_head.rs");
        assert!(port.contains("pub use crate::adapters::settlement_head::"));
        for symbol in ["attachment_exists", "messaging_settlement", "package_file_link_target"] {
            assert!(port.contains(symbol));
        }
    }
}
