//! Native-owned Legal read-tail r39 contract.
//!
//! The disposed-document guard and the direct-hold witness were the last
//! TypeScript statement tokens in the r39 fragment family. The exact templates
//! now live in this owner's JSON contract, bound into Rust with `include_str!`
//! and imported by the TypeScript adapter. Callers supply only the fixed alias
//! and item reference; every table, join and bind stays owner-controlled.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned r39 read-head contract.
pub const LEGAL_READ_TAIL_R39_SOURCES_JSON: &str = include_str!("legal-read-tail-r39-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalReadTailR39Sources {
    #[serde(rename = "disposedGuard")]
    pub disposed_guard: String,
    #[serde(rename = "holdOwnTarget")]
    pub hold_own_target: String,
}

static SOURCES: OnceLock<LegalReadTailR39Sources> = OnceLock::new();

pub fn legal_read_tail_r39_sources() -> &'static LegalReadTailR39Sources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_READ_TAIL_R39_SOURCES_JSON)
            .expect("legal read-tail r39 SQL contract must be valid JSON")
    })
}

/// Render the disposed-document guard for one fixed alias and item reference.
pub fn render_legal_disposed_guard(alias: &str, reference: &str) -> String {
    legal_read_tail_r39_sources()
        .disposed_guard
        .replace("{alias}", alias)
        .replace("{ref}", reference)
}

/// Render the direct-hold witness for one fixed holding alias.
pub fn render_legal_hold_own_target(alias: &str) -> String {
    legal_read_tail_r39_sources()
        .hold_own_target
        .replace("{alias}", alias)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn digest(value: &str) -> String {
        format!("{:x}", Sha256::digest(value.as_bytes()))
    }

    #[test]
    fn contract_is_byte_exact_and_read_only() {
        assert_eq!(
            digest(LEGAL_READ_TAIL_R39_SOURCES_JSON),
            "670af8e6257d3de6af0b7ade3fddd638f4d935ae545c3a5b781829ad8fb5b74a"
        );
        assert_eq!(
            render_legal_disposed_guard("x", "d.id"),
            "NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=d.id)"
        );
        assert_eq!(
            render_legal_disposed_guard("disposed", "legal_documents.id"),
            "NOT EXISTS(SELECT 1 FROM legal_disposals disposed WHERE disposed.document_id=legal_documents.id)"
        );
        assert_eq!(
            render_legal_hold_own_target("h"),
            "EXISTS(SELECT 1 FROM legal_hold_targets own WHERE own.hold_id=h.id AND own.item_id=?)"
        );
        let sources = legal_read_tail_r39_sources();
        for template in [&sources.disposed_guard, &sources.hold_own_target] {
            assert!(!template.contains("INSERT INTO"));
            assert!(!template.contains("DELETE FROM"));
            assert!(!template.contains("UPDATE "));
            assert!(!template.contains("FOR UPDATE"));
            assert!(!template.contains("FOR SHARE"));
            assert!(!template.contains("LOCK TABLE"));
            assert!(!template.contains("pg_advisory"));
        }
    }

    #[test]
    fn render_preserves_binds_and_literal_dollar_text() {
        assert_eq!(
            render_legal_hold_own_target("hold"),
            "EXISTS(SELECT 1 FROM legal_hold_targets own WHERE own.hold_id=hold.id AND own.item_id=?)"
        );
        assert_eq!(
            render_legal_disposed_guard("$&", "d.$1"),
            "NOT EXISTS(SELECT 1 FROM legal_disposals $& WHERE $&.document_id=d.$1)"
        );
        assert!(!render_legal_disposed_guard("x", "d.id").contains("{alias}"));
        assert!(!render_legal_disposed_guard("x", "d.id").contains("{ref}"));
        assert!(!render_legal_hold_own_target("h").contains("{alias}"));
        assert_eq!(render_legal_hold_own_target("h").matches('?').count(), 1);
    }
}
