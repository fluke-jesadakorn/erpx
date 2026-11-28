//! Native-owned Legal work-purchases filter contract.
//!
//! The work source union (record, approval and feature sources) and the final
//! filtered row read were the last TypeScript statement tokens in
//! work-purchases.ts. The exact templates now live in this owner's JSON
//! contract, bound into Rust with `include_str!` and imported by the TypeScript
//! adapter. Callers supply only trusted feature tables, the contracted
//! member-edit witness and the existing visibility/purchase predicates.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned work-purchases filter contract.
pub const WORK_PURCHASES_SOURCES_JSON: &str = include_str!("work-purchases-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct WorkPurchasesSources {
    #[serde(rename = "recordSource")]
    pub record_source: String,
    #[serde(rename = "approvalSource")]
    pub approval_source: String,
    #[serde(rename = "featureSource")]
    pub feature_source: String,
    pub rows: String,
}

static SOURCES: OnceLock<WorkPurchasesSources> = OnceLock::new();

pub fn work_purchases_sources() -> &'static WorkPurchasesSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(WORK_PURCHASES_SOURCES_JSON)
            .expect("legal work-purchases SQL contract must be valid JSON")
    })
}

pub fn work_record_source() -> &'static str {
    &work_purchases_sources().record_source
}

pub fn work_approval_source() -> &'static str {
    &work_purchases_sources().approval_source
}

/// Render one feature source for a trusted feature-table name.
pub fn render_work_feature_source(table: &str) -> String {
    work_purchases_sources()
        .feature_source
        .replace("{table}", table)
}

/// Render the final filtered work read from the already-built source union and
/// the caller's trusted member-edit/visibility/purchase fragments.
pub fn render_work_rows(
    member_edit: &str,
    sources: &str,
    in_list: &str,
    scope: &str,
    purchase: &str,
) -> String {
    work_purchases_sources()
        .rows
        .replace("{memberEdit}", member_edit)
        .replace("{sources}", sources)
        .replace("{inList}", in_list)
        .replace("{scope}", scope)
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
            digest(WORK_PURCHASES_SOURCES_JSON),
            "36313aadc7878271d3b810421a6d31d7ca59e5e260c7d9a5568f064cb81c9ace"
        );
        assert_eq!(
            work_record_source(),
            "SELECT id work_id,id item_id,'record' source FROM legal_items"
        );
        assert_eq!(
            work_approval_source(),
            "SELECT 'legal-approval:'||id,id,'approval' FROM legal_items"
        );
        assert_eq!(
            render_work_feature_source("legal_obligation_schedule"),
            "SELECT 'legal-task:'||id,item_id,'task' FROM legal_obligation_schedule"
        );
        let rows = render_work_rows(
            "EXISTS(legal_members editor)",
            "record_source UNION ALL approval_source",
            "?,?",
            "i.company_id=?",
            "TRUE",
        );
        assert_eq!(
            rows,
            "SELECT source.work_id,source.source,i.kind,EXISTS(legal_members editor) can_edit FROM (record_source UNION ALL approval_source) source JOIN legal_items i ON i.id=source.item_id WHERE source.work_id IN (?,?) AND i.company_id=? AND TRUE"
        );
        for sql in [
            work_record_source(),
            work_approval_source(),
            &render_work_feature_source("legal_obligation_schedule"),
            &rows,
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
        let rows = render_work_rows("member.$&", "record.$1", "?", "scope.$2", "purchase.$3");
        assert!(rows.contains("member.$&"));
        assert!(rows.contains("record.$1"));
        assert!(rows.contains("scope.$2"));
        assert!(rows.contains("purchase.$3"));
        for token in [
            "{memberEdit}",
            "{sources}",
            "{inList}",
            "{scope}",
            "{purchase}",
        ] {
            assert!(!rows.contains(token), "{token}");
        }
        assert!(!render_work_feature_source("legal_matters").contains("{table}"));
    }
}
