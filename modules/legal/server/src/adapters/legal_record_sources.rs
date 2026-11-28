//! Native-owned Legal record-source guard contract.
//!
//! The native-source EXISTS guard, the actual-source EXISTS guard and the
//! workspace-record guard were the last TypeScript statement tokens in
//! record-purchases.ts. The exact templates now live in this owner's JSON
//! contract, bound into Rust with `include_str!` and imported by the TypeScript
//! adapter. Callers supply only trusted surface expressions and the owner-built
//! scope, assignment, release and purchase fragments.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned record-source guard contract.
pub const LEGAL_RECORD_SOURCES_JSON: &str = include_str!("legal-record-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalRecordSources {
    #[serde(rename = "nativeSource")]
    pub native_source: String,
    #[serde(rename = "actualSource")]
    pub actual_source: String,
    #[serde(rename = "workspaceGuard")]
    pub workspace_guard: String,
}

static SOURCES: OnceLock<LegalRecordSources> = OnceLock::new();

pub fn legal_record_sources() -> &'static LegalRecordSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_RECORD_SOURCES_JSON)
            .expect("legal record-source SQL contract must be valid JSON")
    })
}

/// Render the native Legal source EXISTS guard.
#[allow(clippy::too_many_arguments)]
pub fn render_legal_native_source(
    id: &str,
    company: &str,
    scope: &str,
    permission: &str,
    assignment: &str,
    release: &str,
    purchase: &str,
) -> String {
    legal_record_sources()
        .native_source
        .replace("{id}", id)
        .replace("{company}", company)
        .replace("{scope}", scope)
        .replace("{permission}", permission)
        .replace("{assignment}", assignment)
        .replace("{release}", release)
        .replace("{purchase}", purchase)
}

/// Render the actual-source EXISTS guard for any Legal record reference.
pub fn render_legal_actual_source(id: &str) -> String {
    legal_record_sources().actual_source.replace("{id}", id)
}

/// Render the Legal workspace-record guard.
pub fn render_legal_workspace_guard(scope: &str) -> String {
    legal_record_sources()
        .workspace_guard
        .replace("{scope}", scope)
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
            digest(LEGAL_RECORD_SOURCES_JSON),
            "87383a387e26c6a0b271882ec448a6168421c23e7f111ebc3ab672171db64038"
        );
        let native = render_legal_native_source(
            "legal_native.id",
            "legal_native.company_id",
            "legal_native.company_id=?",
            "TRUE",
            "assignment=?",
            "release=?",
            "AND purchase=?",
        );
        assert_eq!(
            native,
            "EXISTS(SELECT 1 FROM legal_items legal_native WHERE legal_native.id=legal_native.id AND legal_native.company_id=legal_native.company_id AND legal_native.company_id=? AND TRUE AND assignment=? AND release=? AND purchase=?)"
        );
        let unpurchased = render_legal_native_source(
            "n.id",
            "n.company_id",
            "scope",
            "FALSE",
            "assignment",
            "release",
            "",
        );
        assert_eq!(
            unpurchased,
            "EXISTS(SELECT 1 FROM legal_items legal_native WHERE legal_native.id=n.id AND legal_native.company_id=n.company_id AND scope AND FALSE AND assignment AND release )"
        );
        assert_eq!(
            render_legal_actual_source("workspace_records.id"),
            "EXISTS(SELECT 1 FROM legal_items actual_legal_source WHERE actual_legal_source.id=workspace_records.id)"
        );
        assert_eq!(
            render_legal_workspace_guard("workspace_records.company_id=?"),
            "SELECT 1 FROM workspace_records WHERE module_id='legal' AND id=? AND workspace_records.company_id=?"
        );
        assert_eq!(native.matches('?').count(), 4);
        for sql in [native.as_str(), unpurchased.as_str()] {
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
        let native =
            render_legal_native_source("i.$&", "c.$1", "s.$2", "p.$3", "a.$4", "r.$5", "u.$6");
        assert!(native.contains("legal_native.id=i.$&"));
        assert!(native.contains("s.$2"));
        assert!(native.contains("a.$4"));
        assert!(native.contains("r.$5"));
        assert!(native.contains("u.$6"));
        for token in [
            "{id}",
            "{company}",
            "{scope}",
            "{permission}",
            "{assignment}",
            "{release}",
            "{purchase}",
        ] {
            assert!(!native.contains(token), "{token}");
        }
        let actual = render_legal_actual_source("$&");
        assert!(actual.contains("actual_legal_source.id=$&"));
        assert!(!actual.contains("{id}"));
        let guard = render_legal_workspace_guard("$1");
        assert!(guard.contains("AND $1"));
        assert!(!guard.contains("{scope}"));
    }
}
