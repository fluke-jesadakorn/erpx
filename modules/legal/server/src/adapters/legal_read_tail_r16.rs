//! Native-owned Legal read-tail r16 contract.
//!
//! The item-list read prefix was the last TypeScript statement token in the
//! r16 family. The exact template now lives in this owner's JSON contract,
//! bound into Rust with `include_str!` and imported by the TypeScript adapter.
//! Callers still supply their own trusted predicate and binds.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned r16 item-list contract.
pub const LEGAL_READ_TAIL_R16_SOURCES_JSON: &str = include_str!("legal-read-tail-r16-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalReadTailR16Sources {
    #[serde(rename = "itemSelect")]
    pub item_select: String,
}

static SOURCES: OnceLock<LegalReadTailR16Sources> = OnceLock::new();

pub fn legal_read_tail_r16_sources() -> &'static LegalReadTailR16Sources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_READ_TAIL_R16_SOURCES_JSON)
            .expect("legal read-tail r16 SQL contract must be valid JSON")
    })
}

/// Exact item-list read prefix, including its trailing space.
pub fn legal_item_select_sql() -> &'static str {
    &legal_read_tail_r16_sources().item_select
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
        let sql = legal_item_select_sql();
        assert_eq!(
            digest(LEGAL_READ_TAIL_R16_SOURCES_JSON),
            "532f4f3fb14cd17c7c4c3a02ec7c118c21888aa4bb65795cedf6a734d1440ac7"
        );
        assert_eq!(sql, "SELECT i.* FROM legal_items i WHERE ");
        assert!(sql.ends_with(' '));
        assert_eq!(sql.matches('?').count(), 0);
        assert!(!sql.contains("INSERT INTO"));
        assert!(!sql.contains("DELETE FROM"));
        assert!(!sql.contains("UPDATE "));
        assert!(!sql.contains("FOR UPDATE"));
        assert!(!sql.contains("FOR SHARE"));
        assert!(!sql.contains("LOCK TABLE"));
        assert!(!sql.contains("pg_advisory"));
    }
}
