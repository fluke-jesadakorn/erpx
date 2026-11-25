//! Native-owned Legal read-tail r25 contract.
//!
//! The edit-member existence witness and the item-scoped jsonb aggregate
//! projection were the last TypeScript statement tokens in the r25 fragment
//! family. The exact templates now live in this owner's JSON contract, bound
//! into Rust with `include_str!` and imported by the TypeScript adapter.
//! Callers supply only fixed aliases, tables and order columns; no request text
//! is interpolated.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned r25 read-head contract.
pub const LEGAL_READ_TAIL_R25_SOURCES_JSON: &str = include_str!("legal-read-tail-r25-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalReadTailR25Sources {
    #[serde(rename = "memberEditExists")]
    pub member_edit_exists: String,
    #[serde(rename = "jsonbAggByItem")]
    pub jsonb_agg_by_item: String,
}

static SOURCES: OnceLock<LegalReadTailR25Sources> = OnceLock::new();

pub fn legal_read_tail_r25_sources() -> &'static LegalReadTailR25Sources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_READ_TAIL_R25_SOURCES_JSON)
            .expect("legal read-tail r25 SQL contract must be valid JSON")
    })
}

/// Render the edit-member existence witness for one fixed table alias.
pub fn render_legal_member_edit_exists(alias: &str) -> String {
    legal_read_tail_r25_sources()
        .member_edit_exists
        .replace("{alias}", alias)
}

/// Render the item-scoped jsonb aggregate for one fixed alias/table/order.
pub fn render_legal_jsonb_agg_by_item(alias: &str, table: &str, order: &str) -> String {
    legal_read_tail_r25_sources()
        .jsonb_agg_by_item
        .replace("{alias}", alias)
        .replace("{table}", table)
        .replace("{order}", order)
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
            digest(LEGAL_READ_TAIL_R25_SOURCES_JSON),
            "435206d2b50d517fc159f385c28867f259b718f6d5a52005431e4d8c4480ee50"
        );
        assert_eq!(
            render_legal_member_edit_exists("em"),
            "EXISTS(SELECT 1 FROM legal_members em WHERE em.item_id=i.id AND em.user_id=? AND em.access='edit')"
        );
        assert_eq!(
            render_legal_jsonb_agg_by_item("d", "legal_documents", "id"),
            "(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM legal_documents d WHERE d.item_id=i.id)"
        );
        assert_eq!(
            render_legal_jsonb_agg_by_item("ap", "legal_approvals", "cycle"),
            "(SELECT jsonb_agg(to_jsonb(ap) ORDER BY ap.cycle) FROM legal_approvals ap WHERE ap.item_id=i.id)"
        );
        let sources = legal_read_tail_r25_sources();
        for template in [&sources.member_edit_exists, &sources.jsonb_agg_by_item] {
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
            render_legal_member_edit_exists("$&"),
            "EXISTS(SELECT 1 FROM legal_members $& WHERE $&.item_id=i.id AND $&.user_id=? AND $&.access='edit')"
        );
        assert_eq!(
            render_legal_jsonb_agg_by_item("$&", "legal_$1", "$2"),
            "(SELECT jsonb_agg(to_jsonb($&) ORDER BY $&.$2) FROM legal_$1 $& WHERE $&.item_id=i.id)"
        );
        assert!(!render_legal_member_edit_exists("em").contains("{alias}"));
        assert!(!render_legal_jsonb_agg_by_item("d", "legal_documents", "id").contains("{alias}"));
        assert!(!render_legal_jsonb_agg_by_item("d", "legal_documents", "id").contains("{table}"));
        assert!(!render_legal_jsonb_agg_by_item("d", "legal_documents", "id").contains("{order}"));
        assert_eq!(
            render_legal_member_edit_exists("em").matches('?').count(),
            1
        );
    }
}
