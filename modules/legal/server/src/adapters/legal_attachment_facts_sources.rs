//! Native-owned Legal attachment-fact guard contract.
//!
//! The owner version guard and the active source-preservation read were the last
//! TypeScript statement tokens in attachments-service-facts.ts. The exact
//! templates now live in this owner's JSON contract, bound into Rust with
//! `include_str!` and imported by the TypeScript adapter. Callers supply only the
//! trusted target table/extra predicate and the Files attachment projection
//! fragments.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned attachment facts contract.
pub const LEGAL_ATTACHMENT_FACTS_SOURCES_JSON: &str =
    include_str!("legal-attachment-facts-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalAttachmentFactsSources {
    #[serde(rename = "ownerVersionGuard")]
    pub owner_version_guard: String,
    #[serde(rename = "filePreservations")]
    pub file_preservations: String,
}

static SOURCES: OnceLock<LegalAttachmentFactsSources> = OnceLock::new();

pub fn legal_attachment_facts_sources() -> &'static LegalAttachmentFactsSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_ATTACHMENT_FACTS_SOURCES_JSON)
            .expect("legal attachment facts SQL contract must be valid JSON")
    })
}

/// Render the owner version guard for a trusted target table and extra predicate.
pub fn render_legal_owner_version_guard(table: &str, extra: &str) -> String {
    legal_attachment_facts_sources()
        .owner_version_guard
        .replace("{table}", table)
        .replace("{extra}", extra)
}

/// Render the active file-preservation read with the Files projections.
pub fn render_legal_file_preservations(attachment_links: &str, attachment_sets: &str) -> String {
    legal_attachment_facts_sources()
        .file_preservations
        .replace("{attachmentLinks}", attachment_links)
        .replace("{attachmentSets}", attachment_sets)
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
            digest(LEGAL_ATTACHMENT_FACTS_SOURCES_JSON),
            "0461327e3e3335332147a3d01a1e40597ec3a841ce0f7d970afebbf5afb21bca"
        );
        assert_eq!(
            render_legal_owner_version_guard("legal_items", ""),
            "EXISTS(SELECT 1 FROM legal_items WHERE id=? AND company_id=? AND version=?)"
        );
        assert_eq!(
            render_legal_owner_version_guard("legal_items", " AND state='active'"),
            "EXISTS(SELECT 1 FROM legal_items WHERE id=? AND company_id=? AND version=? AND state='active')"
        );
        let preservations = render_legal_file_preservations("attachment_links", "attachment_sets");
        assert!(preservations
            .starts_with("SELECT 1 FROM (\n    SELECT p.item_id FROM legal_file_preservations p"));
        assert!(preservations.contains("FROM (attachment_links) link"));
        assert!(preservations.contains("JOIN (attachment_sets) set_row"));
        assert!(preservations.ends_with("  ) active LIMIT 1"));
        assert_eq!(preservations.matches("SELECT ").count(), 3);
        assert_eq!(preservations.matches('?').count(), 2);
        for sql in [
            render_legal_owner_version_guard("legal_items", ""),
            preservations,
        ] {
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
        let guard = render_legal_owner_version_guard("t.$&", "e.$1");
        assert!(guard.contains("FROM t.$& WHERE"));
        assert!(guard.contains("version=?e.$1)"));
        assert!(!guard.contains("{table}"));
        assert!(!guard.contains("{extra}"));
        let preservations = render_legal_file_preservations("l.$&", "s.$1");
        assert!(preservations.contains("FROM (l.$&) link"));
        assert!(preservations.contains("JOIN (s.$1) set_row"));
        assert!(!preservations.contains("{attachmentLinks}"));
        assert!(!preservations.contains("{attachmentSets}"));
    }
}
