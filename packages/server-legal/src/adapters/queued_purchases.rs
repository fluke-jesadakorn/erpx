//! Native-owned Legal queued-purchase source contract.
//!
//! The queued job source read and the executing-only destruction guard were the
//! last TypeScript statement tokens in queued-purchases.ts. The exact templates
//! now live in this owner's JSON contract, bound into Rust with `include_str!`
//! and imported by the TypeScript adapter. Callers supply only the fixed job
//! table and its trusted document-column fragment.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned queued-purchase read contract.
pub const QUEUED_PURCHASES_SOURCES_JSON: &str = include_str!("queued-purchases-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct QueuedPurchasesSources {
    #[serde(rename = "jobSource")]
    pub job_source: String,
    #[serde(rename = "processingSourceGuard")]
    pub processing_source_guard: String,
}

static SOURCES: OnceLock<QueuedPurchasesSources> = OnceLock::new();

pub fn queued_purchases_sources() -> &'static QueuedPurchasesSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(QUEUED_PURCHASES_SOURCES_JSON)
            .expect("legal queued-purchases SQL contract must be valid JSON")
    })
}

/// Render the queued job source read for one fixed job table and document
/// column fragment.
pub fn render_queued_job_source(table: &str, document_columns: &str) -> String {
    queued_purchases_sources()
        .job_source
        .replace("{table}", table)
        .replace("{documentColumns}", document_columns)
}

/// Exact executing-only destruction guard used before completion.
pub fn queued_processing_source_guard() -> &'static str {
    &queued_purchases_sources().processing_source_guard
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
            digest(QUEUED_PURCHASES_SOURCES_JSON),
            "6c3f2c717b8f1a81355e5f51fac148bed137a22d0604773e9284e2b24ba69e41"
        );
        assert_eq!(
            render_queued_job_source("legal_ai_proposals", "NULL::text document_id,NULL::text checksum"),
            "SELECT p.item_id,i.company_id,p.actor_json,to_jsonb(p)::text image,NULL::text document_id,NULL::text checksum FROM legal_ai_proposals p JOIN legal_items i ON i.id=p.item_id WHERE p.id=?"
        );
        assert_eq!(
            render_queued_job_source("legal_processing_jobs", "p.document_id,p.checksum"),
            "SELECT p.item_id,i.company_id,p.actor_json,to_jsonb(p)::text image,p.document_id,p.checksum FROM legal_processing_jobs p JOIN legal_items i ON i.id=p.item_id WHERE p.id=?"
        );
        assert_eq!(
            queued_processing_source_guard(),
            "NOT EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=d.item_id AND x.state='executing')"
        );
        assert!(!render_queued_job_source(
            "legal_ai_proposals",
            "NULL::text document_id,NULL::text checksum"
        )
        .contains("{table}"));
        assert!(!render_queued_job_source(
            "legal_ai_proposals",
            "NULL::text document_id,NULL::text checksum"
        )
        .contains("{documentColumns}"));
        for write in [
            "INSERT INTO",
            "DELETE FROM",
            "UPDATE ",
            "FOR UPDATE",
            "FOR SHARE",
            "LOCK TABLE",
            "pg_advisory",
        ] {
            assert!(!queued_processing_source_guard().contains(write), "{write}");
        }
    }

    #[test]
    fn job_source_keeps_its_single_bind_and_read_only_shape() {
        let sql = render_queued_job_source("legal_processing_jobs", "p.document_id,p.checksum");
        assert_eq!(sql.matches('?').count(), 1);
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
