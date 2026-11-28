//! Native-owned executive main-evidence projection contract.
//!
//! The obligation and hold projection prefixes were the last TypeScript
//! statement tokens in executive-main-evidence-sources.ts. The exact projections
//! now live in this owner's JSON contract, bound into Rust with `include_str!`
//! and imported by the TypeScript adapter. The r46 owner spine heads still own
//! the FROM/JOIN/WHERE shape and aliases.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned executive main-evidence projection contract.
pub const EXECUTIVE_MAIN_EVIDENCE_SOURCES_JSON: &str =
    include_str!("executive-main-evidence-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct ExecutiveMainEvidenceSources {
    #[serde(rename = "obligationProjection")]
    pub obligation_projection: String,
    #[serde(rename = "holdProjection")]
    pub hold_projection: String,
}

static SOURCES: OnceLock<ExecutiveMainEvidenceSources> = OnceLock::new();

pub fn executive_main_evidence_sources() -> &'static ExecutiveMainEvidenceSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(EXECUTIVE_MAIN_EVIDENCE_SOURCES_JSON)
            .expect("legal executive main-evidence SQL contract must be valid JSON")
    })
}

pub fn executive_obligation_projection() -> &'static str {
    &executive_main_evidence_sources().obligation_projection
}

pub fn executive_hold_projection() -> &'static str {
    &executive_main_evidence_sources().hold_projection
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
            digest(EXECUTIVE_MAIN_EVIDENCE_SOURCES_JSON),
            "2a9a69668bc84df0ee2a194bb97f596ca7d452f0a26c8d135673f78d868bbd5e"
        );
        assert_eq!(
            executive_obligation_projection(),
            "SELECT s.id,s.item_id,s.title,s.category,s.due_date,s.state,s.version,\n          i.title item_title,i.kind item_kind,i.version item_version"
        );
        assert_eq!(
            executive_hold_projection(),
            "SELECT h.id,h.item_id,h.state,h.created_at,\n          i.title item_title,i.kind item_kind,i.version item_version"
        );
        for projection in [
            executive_obligation_projection(),
            executive_hold_projection(),
        ] {
            assert!(projection.starts_with("SELECT "));
            for write in [
                "INSERT INTO",
                "DELETE FROM",
                "UPDATE ",
                "FOR UPDATE",
                "FOR SHARE",
                "LOCK TABLE",
                "pg_advisory",
            ] {
                assert!(!projection.contains(write), "{write}");
            }
        }
    }

    #[test]
    fn projections_keep_their_aliases_and_columns() {
        let obligation = executive_obligation_projection();
        assert!(obligation.contains("s.version"));
        assert!(obligation.contains("i.title item_title"));
        assert!(obligation.contains("i.kind item_kind"));
        assert!(obligation.contains("i.version item_version"));
        let hold = executive_hold_projection();
        assert!(hold.contains("h.created_at"));
        assert!(hold.contains("i.title item_title"));
        assert!(hold.contains("i.version item_version"));
    }
}
