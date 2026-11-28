//! Native-owned Legal purchase/authority/read contract.
//!
//! The current-authority probe, configuration image, source witness, feature
//! image and replay-receipt reads were the last TypeScript statement tokens in
//! purchases.ts. The exact templates now live in this owner's JSON contract,
//! bound into Rust with `include_str!` and imported by the TypeScript adapter.
//! Callers supply only owner-built projections and trusted feature tables.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned purchases read/witness contract.
pub const LEGAL_PURCHASES_SOURCES_JSON: &str = include_str!("legal-purchases-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalPurchasesSources {
    pub authority: String,
    #[serde(rename = "configurationImage")]
    pub configuration_image: String,
    #[serde(rename = "sourceWitness")]
    pub source_witness: String,
    #[serde(rename = "featureImage")]
    pub feature_image: String,
    #[serde(rename = "replayReceipt")]
    pub replay_receipt: String,
}

static SOURCES: OnceLock<LegalPurchasesSources> = OnceLock::new();

pub fn legal_purchases_sources() -> &'static LegalPurchasesSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_PURCHASES_SOURCES_JSON)
            .expect("legal purchases SQL contract must be valid JSON")
    })
}

/// Render the current-authority probe.
pub fn render_legal_authority(assignment: &str, actor_scope: &str) -> String {
    legal_purchases_sources()
        .authority
        .replace("{assignment}", assignment)
        .replace("{actorScope}", actor_scope)
}

/// Render the configuration image read for one trusted source table/key.
pub fn render_legal_configuration_image(table: &str, key: &str) -> String {
    legal_purchases_sources()
        .configuration_image
        .replace("{table}", table)
        .replace("{key}", key)
}

/// Render the full source-witness projection from the contracted fragments.
#[allow(clippy::too_many_arguments)]
pub fn render_legal_source_witness(
    fact_data: &str,
    members: &str,
    documents: &str,
    shares: &str,
    executed: &str,
    approvals: &str,
    authorities: &str,
    destructions: &str,
    features: &str,
) -> String {
    legal_purchases_sources()
        .source_witness
        .replace("{factData}", fact_data)
        .replace("{members}", members)
        .replace("{documents}", documents)
        .replace("{shares}", shares)
        .replace("{executed}", executed)
        .replace("{approvals}", approvals)
        .replace("{authorities}", authorities)
        .replace("{destructions}", destructions)
        .replace("{features}", features)
}

/// Render one feature witness source for a trusted feature table.
pub fn render_legal_feature_image(table: &str) -> String {
    legal_purchases_sources()
        .feature_image
        .replace("{table}", table)
}

pub fn legal_replay_receipt_sql() -> &'static str {
    &legal_purchases_sources().replay_receipt
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
            digest(LEGAL_PURCHASES_SOURCES_JSON),
            "9f68725068bab609237f673d18b0f7cd585493ba35799ba2e228dc8ae94f0fef"
        );
        assert_eq!(
            render_legal_authority("assignment_project", "actor_scope"),
            "SELECT assignment_project allowed FROM (actor_scope) legal_current_scope"
        );
        assert_eq!(
            render_legal_configuration_image("legal_settings", "company_id"),
            "SELECT to_jsonb(configuration)::text image FROM legal_settings configuration WHERE company_id=? AND company_id=?"
        );
        let witness = render_legal_source_witness(
            "data_fragment",
            "members_fragment",
            "documents_fragment",
            "shares_fragment",
            "executed_fragment",
            "approvals_fragment",
            "authorities_fragment",
            "destructions_fragment",
            "features_fragment",
        );
        assert!(witness.starts_with("jsonb_build_object('item',to_jsonb(i),'data',(data_fragment),'members',members_fragment"));
        assert!(witness.contains(
            "'route',(SELECT to_jsonb(ar) FROM legal_approval_routes ar WHERE ar.item_id=i.id)"
        ));
        assert!(witness.contains("'retention',(SELECT to_jsonb(ra)||jsonb_build_object('policy',(SELECT to_jsonb(rp) FROM legal_retention_policies rp WHERE rp.id=ra.policy_id)) FROM legal_retention_assignments ra WHERE ra.item_id=i.id)"));
        assert!(witness.contains("'features',(SELECT jsonb_agg(feature.image ORDER BY feature.image::text) FROM (features_fragment) feature)"));
        assert!(witness.ends_with("'destructions',destructions_fragment,'features',(SELECT jsonb_agg(feature.image ORDER BY feature.image::text) FROM (features_fragment) feature))::text image"));
        assert_eq!(
            render_legal_feature_image("legal_case_events"),
            "SELECT to_jsonb(f) image FROM legal_case_events f WHERE f.item_id=i.id"
        );
        assert_eq!(
            legal_replay_receipt_sql(),
            "SELECT operation FROM command_receipts WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?"
        );
        for sql in [
            render_legal_authority("a", "s"),
            render_legal_configuration_image("legal_settings", "company_id"),
            witness,
            render_legal_feature_image("legal_matters"),
            legal_replay_receipt_sql().to_owned(),
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
        let authority = render_legal_authority("a.$&", "s.$1");
        assert!(authority.contains("SELECT a.$& allowed FROM (s.$1)"));
        assert!(!authority.contains("{assignment}"));
        assert!(!authority.contains("{actorScope}"));
        let config = render_legal_configuration_image("t.$&", "k.$1");
        assert!(config.contains("FROM t.$& configuration WHERE k.$1=?"));
        assert!(!config.contains("{table}"));
        assert!(!config.contains("{key}"));
        let witness = render_legal_source_witness(
            "f.$&", "m.$1", "d.$2", "s.$3", "e.$4", "a.$5", "au.$6", "dx.$7", "ft.$8",
        );
        for fragment in [
            "f.$&", "m.$1", "d.$2", "s.$3", "e.$4", "a.$5", "au.$6", "dx.$7", "ft.$8",
        ] {
            assert!(witness.contains(fragment), "{fragment}");
        }
        for token in [
            "{factData}",
            "{members}",
            "{documents}",
            "{shares}",
            "{executed}",
            "{approvals}",
            "{authorities}",
            "{destructions}",
            "{features}",
        ] {
            assert!(!witness.contains(token), "{token}");
        }
        assert!(!render_legal_feature_image("t.$&").contains("{table}"));
    }
}
