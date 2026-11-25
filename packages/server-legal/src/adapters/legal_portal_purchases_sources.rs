//! Native-owned Legal portal purchase/witness contract.
//!
//! The closed portal source-image fields and the portal release guard were the
//! last TypeScript statement tokens in portal-purchases.ts. The exact templates
//! now live in this owner's JSON contract, bound into Rust with `include_str!`
//! and imported by the TypeScript adapter. Callers supply only the owner-built
//! scheduled-feature predicate for the release guard; the image fields are fixed.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned portal purchases contract.
pub const LEGAL_PORTAL_PURCHASES_SOURCES_JSON: &str =
    include_str!("legal-portal-purchases-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalPortalPurchasesSources {
    #[serde(rename = "portalImageFields")]
    pub portal_image_fields: String,
    #[serde(rename = "releaseProbe")]
    pub release_probe: String,
}

static SOURCES: OnceLock<LegalPortalPurchasesSources> = OnceLock::new();

pub fn legal_portal_purchases_sources() -> &'static LegalPortalPurchasesSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_PORTAL_PURCHASES_SOURCES_JSON)
            .expect("legal portal purchases SQL contract must be valid JSON")
    })
}

/// The fixed closed portal source-image fields.
pub fn legal_portal_image_fields_sql() -> &'static str {
    &legal_portal_purchases_sources().portal_image_fields
}

/// Render the portal release guard with the owner scheduled-feature predicate.
pub fn render_legal_portal_release_probe(scheduled_feature_predicate: &str) -> String {
    legal_portal_purchases_sources()
        .release_probe
        .replace("{scheduledFeaturePredicate}", scheduled_feature_predicate)
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
            digest(LEGAL_PORTAL_PURCHASES_SOURCES_JSON),
            "054dba4eb26908ac6ec28cd79413f57a1490f2b6faf394ef207047c9b9b8c889"
        );
        let fields = legal_portal_image_fields_sql();
        assert!(fields
            .starts_with("'settings',(SELECT to_jsonb(settings) FROM legal_settings settings"));
        assert!(fields.contains("'destructions',(SELECT jsonb_agg(to_jsonb(destruction)"));
        assert!(fields.contains("'comments',(SELECT jsonb_agg(to_jsonb(comment)"));
        assert!(fields.contains("'envelopes',(SELECT jsonb_agg(jsonb_build_object('envelope'"));
        assert!(fields.contains("'billing',(SELECT jsonb_agg(to_jsonb(entry) ORDER BY entry.id) FROM legal_counsel_entries entry WHERE entry.item_id=i.id AND entry.created_by=?)"));
        assert_eq!(fields.matches("SELECT ").count(), 5);
        assert_eq!(fields.matches('?').count(), 2);
        assert_eq!(fields.lines().count(), 5);
        assert_eq!(
            render_legal_portal_release_probe("scheduled_feature('legal')"),
            "SELECT 1 FROM (SELECT ?::text company_id) legal_portal_release WHERE scheduled_feature('legal')"
        );
        for sql in [fields, render_legal_portal_release_probe("f").as_str()] {
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
        let probe = render_legal_portal_release_probe("feature.$&");
        assert!(probe.contains("feature.$&"));
        assert!(!probe.contains("{scheduledFeaturePredicate}"));
        assert!(!legal_portal_image_fields_sql().contains('{'));
    }
}
