//! Native-owned Legal operations read contract.
//!
//! The register read and the recursive hold-preview read were the last
//! TypeScript statement tokens in legal/operations.ts. The exact
//! templates now live in this owner's JSON contract, bound into Rust with
//! `include_str!` and imported by the TypeScript adapter. Callers supply only
//! trusted owner fragments: the register table, the visibility predicate and the
//! purchase predicate built by the Legal adapters.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned operations read contract.
pub const LEGAL_OPERATIONS_SOURCES_JSON: &str = include_str!("legal-operations-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalOperationsSources {
    #[serde(rename = "registerRows")]
    pub register_rows: String,
    #[serde(rename = "holdPreview")]
    pub hold_preview: String,
}

static SOURCES: OnceLock<LegalOperationsSources> = OnceLock::new();

pub fn legal_operations_sources() -> &'static LegalOperationsSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_OPERATIONS_SOURCES_JSON)
            .expect("legal operations SQL contract must be valid JSON")
    })
}

/// Render the register read for one trusted kind table.
pub fn render_legal_register_rows(table: &str, visibility: &str, purchase: &str) -> String {
    legal_operations_sources()
        .register_rows
        .replace("{table}", table)
        .replace("{visibility}", visibility)
        .replace("{purchase}", purchase)
}

/// Render the recursive hold-preview read.
pub fn render_legal_hold_preview(visibility: &str, purchase: &str) -> String {
    legal_operations_sources()
        .hold_preview
        .replace("{visibility}", visibility)
        .replace("{purchase}", purchase)
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
            digest(LEGAL_OPERATIONS_SOURCES_JSON),
            "3016fe25f76067a306bc08dab6fa11187a892ebb6dd29f647b4a26b1df507877"
        );
        let register = render_legal_register_rows("legal_rules", "i.company_id=?", "(TRUE)");
        assert_eq!(
            register,
            "SELECT i.*,t.data FROM legal_items i JOIN legal_rules t ON t.item_id=i.id WHERE i.company_id=? AND (TRUE) AND i.kind=? ORDER BY i.id LIMIT 10001"
        );
        let hold = render_legal_hold_preview("i.company_id=?", "(TRUE)");
        assert_eq!(
            hold,
            "WITH RECURSIVE linked(id) AS (SELECT ?::text UNION SELECT l.target_id FROM linked t JOIN legal_links l ON l.source_id=t.id WHERE l.relation IN ('evidence','derived','amends','obligation')) SELECT i.id,i.title FROM linked t JOIN legal_items i ON i.id=t.id WHERE i.company_id=? AND (TRUE)"
        );
        assert_eq!(register.matches('?').count(), 2);
        assert_eq!(hold.matches('?').count(), 2);
        for sql in [register.as_str(), hold.as_str()] {
            for write in [
                "INSERT INTO",
                "DELETE FROM",
                "UPDATE ",
                "FOR UPDATE",
                "FOR SHARE",
                "LOCK TABLE",
                "pg_advisory",
            ] {
                assert!(!sql.contains(write), "{write}");
            }
        }
    }

    #[test]
    fn render_preserves_trusted_fragments_and_leaves_no_placeholders() {
        let register = render_legal_register_rows("t.$&", "v.$1", "p.$2");
        assert!(register.contains("JOIN t.$& t"));
        assert!(register.contains("v.$1"));
        assert!(register.contains("p.$2"));
        for token in ["{table}", "{visibility}", "{purchase}"] {
            assert!(!register.contains(token), "{token}");
        }
        let hold = render_legal_hold_preview("hv.$&", "hp.$1");
        assert!(hold.contains("hv.$&"));
        assert!(hold.contains("hp.$1"));
        assert!(!hold.contains("{visibility}"));
        assert!(!hold.contains("{purchase}"));
    }
}
