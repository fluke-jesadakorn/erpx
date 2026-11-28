//! Native-owned specialized-attachment backfill read contract.
//!
//! The hold-existence witness and the legal-document backfill statement were
//! the last TypeScript statement tokens in attachment-migration-facts.ts. The
//! exact templates now live in this owner's JSON contract, bound into Rust with
//! `include_str!` and imported by the TypeScript adapter. The external backfill
//! keeps using the already-contracted r44 join head with this hold witness.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned specialized-attachment read contract.
pub const ATTACHMENT_MIGRATION_FACTS_SOURCES_JSON: &str =
    include_str!("attachment-migration-facts-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct AttachmentMigrationFactsSources {
    #[serde(rename = "holdExists")]
    pub hold_exists: String,
    pub documents: String,
}

static SOURCES: OnceLock<AttachmentMigrationFactsSources> = OnceLock::new();

pub fn attachment_migration_facts_sources() -> &'static AttachmentMigrationFactsSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(ATTACHMENT_MIGRATION_FACTS_SOURCES_JSON)
            .expect("legal attachment-migration SQL contract must be valid JSON")
    })
}

/// Exact active-hold witness used by both specialized backfill projections.
pub fn attachment_hold_exists_sql() -> &'static str {
    &attachment_migration_facts_sources().hold_exists
}

/// Render the legal-document backfill read with the owner hold witness.
pub fn render_attachment_migration_documents() -> String {
    attachment_migration_facts_sources()
        .documents
        .replace("{holdExists}", attachment_hold_exists_sql())
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
            digest(ATTACHMENT_MIGRATION_FACTS_SOURCES_JSON),
            "de4d0ff01947d5f104c1fafa40c26bcc5bde7768868013deb99e19594c67a03d"
        );
        assert_eq!(
            attachment_hold_exists_sql(),
            "EXISTS(SELECT 1 FROM legal_holds h WHERE h.item_id=i.id AND h.state<>'released') legal_hold"
        );
        let documents = render_attachment_migration_documents();
        assert!(documents.starts_with("SELECT d.id,i.company_id,i.branch_id,i.department_id,i.id target_id,i.version target_version,d.id component_ref, "));
        assert!(documents.contains(attachment_hold_exists_sql()));
        assert!(documents.ends_with("WHERE d.object_key IS NOT NULL AND d.attachment_source_id IS NULL ORDER BY d.created_at,d.id"));
        assert!(!documents.contains("{holdExists}"));
        for write in [
            "INSERT INTO",
            "DELETE FROM",
            "UPDATE ",
            "FOR UPDATE",
            "FOR SHARE",
            "LOCK TABLE",
            "pg_advisory",
        ] {
            assert!(!documents.contains(write), "{write}");
        }
    }

    #[test]
    fn hold_witness_is_read_only_and_keeps_the_fixed_alias() {
        let sql = attachment_hold_exists_sql();
        assert!(sql.starts_with("EXISTS(SELECT 1 FROM legal_holds h"));
        assert!(sql.ends_with("legal_hold"));
        assert_eq!(sql.matches("legal_holds").count(), 1);
        assert!(!sql.contains("INSERT INTO"));
        assert!(!sql.contains("DELETE FROM"));
        assert!(!sql.contains("UPDATE "));
    }
}
