//! Native-owned Legal access-guard contract.
//!
//! The visibility member guard, the eligibility probe and the preparation guard
//! were the last TypeScript statement tokens in legal/access.ts. The exact
//! templates now live in this owner's JSON contract, bound into Rust with
//! `include_str!` and imported by the TypeScript adapter. Callers supply only
//! the trusted table alias for the member guard.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned access-guard contract.
pub const LEGAL_ACCESS_SOURCES_JSON: &str = include_str!("legal-access-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalAccessSources {
    #[serde(rename = "visibilityMemberGuard")]
    pub visibility_member_guard: String,
    #[serde(rename = "eligibleProbe")]
    pub eligible_probe: String,
    #[serde(rename = "preparationGuard")]
    pub preparation_guard: String,
}

static SOURCES: OnceLock<LegalAccessSources> = OnceLock::new();

pub fn legal_access_sources() -> &'static LegalAccessSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_ACCESS_SOURCES_JSON)
            .expect("legal access SQL contract must be valid JSON")
    })
}

/// Render the visibility member guard for one trusted alias.
pub fn render_legal_visibility_member_guard(alias: &str) -> String {
    legal_access_sources()
        .visibility_member_guard
        .replace("{alias}", alias)
}

pub fn legal_eligible_probe_sql() -> &'static str {
    &legal_access_sources().eligible_probe
}

pub fn legal_preparation_guard_sql() -> &'static str {
    &legal_access_sources().preparation_guard
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
            digest(LEGAL_ACCESS_SOURCES_JSON),
            "1451a1bbfcdd12d64532b3761c1e417ab65310487ed930d739587c888ceaed0d"
        );
        assert_eq!(
            render_legal_visibility_member_guard("i"),
            "EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=i.id AND lm.user_id=?)"
        );
        assert_eq!(
            render_legal_visibility_member_guard("legal_native"),
            "EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=legal_native.id AND lm.user_id=?)"
        );
        assert_eq!(
            legal_eligible_probe_sql(),
            "SELECT 1 WHERE legal_person_eligible(?,?,?,?,?)"
        );
        assert_eq!(
            legal_preparation_guard_sql(),
            "EXISTS(SELECT 1 FROM legal_items WHERE id=? AND kind IN ('rule','template'))"
        );
        for sql in [
            render_legal_visibility_member_guard("i"),
            legal_eligible_probe_sql().to_owned(),
            legal_preparation_guard_sql().to_owned(),
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
    fn render_preserves_literal_dollar_text_and_leaves_no_placeholder() {
        let guard = render_legal_visibility_member_guard("$&");
        assert_eq!(
            guard,
            "EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=$&.id AND lm.user_id=?)"
        );
        assert!(!guard.contains("{alias}"));
        assert_eq!(legal_eligible_probe_sql().matches('?').count(), 5);
        assert_eq!(legal_preparation_guard_sql().matches('?').count(), 1);
    }
}
