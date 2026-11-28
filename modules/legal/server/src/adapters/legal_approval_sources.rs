//! Native-owned Legal approval authorization-guard contract.
//!
//! The reviewer-membership read and the current-route guard were the last
//! TypeScript statement tokens in legal/approval.ts. The exact templates now
//! live in this owner's JSON contract, bound into Rust with `include_str!` and
//! imported by the TypeScript adapter. The two cross-owner projections are
//! supplied by their owning packages as trusted fragments.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned approval guard contract.
pub const LEGAL_APPROVAL_SOURCES_JSON: &str = include_str!("legal-approval-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalApprovalSources {
    #[serde(rename = "reviewerMembership")]
    pub reviewer_membership: String,
    #[serde(rename = "currentRoute")]
    pub current_route: String,
}

static SOURCES: OnceLock<LegalApprovalSources> = OnceLock::new();

pub fn legal_approval_sources() -> &'static LegalApprovalSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_APPROVAL_SOURCES_JSON)
            .expect("legal approval SQL contract must be valid JSON")
    })
}

/// Render the reviewer-membership guard with the Identity authority assignment
/// projection.
pub fn render_legal_reviewer_membership(assignments: &str) -> String {
    legal_approval_sources()
        .reviewer_membership
        .replace("{assignments}", assignments)
}

/// Render the current-route guard with the Workflow approval-instance projection.
pub fn render_legal_approval_current_route(approval_instances: &str) -> String {
    legal_approval_sources()
        .current_route
        .replace("{approvalInstances}", approval_instances)
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
            digest(LEGAL_APPROVAL_SOURCES_JSON),
            "286fa722cfa7b01624a03669e3ab6442c9784f6d6ad58fcde4f53cf62530afed"
        );
        let membership = render_legal_reviewer_membership("identity_assignments");
        assert!(membership.starts_with(
            "SELECT 1 FROM legal_members m JOIN (identity_assignments) a ON a.user_id=m.user_id AND a.company_id=?"
        ));
        assert!(membership.ends_with("AND iam_role_allows(a.role,a.company_id,'legal.approve')"));
        assert_eq!(membership.matches('?').count(), 5);
        let route = render_legal_approval_current_route("workflow_approval_instances");
        assert_eq!(
            route,
            "EXISTS(SELECT 1 FROM (workflow_approval_instances) approval_instances WHERE id=? AND state='pending' AND current_step=? AND version=?)"
        );
        assert_eq!(route.matches('?').count(), 3);
        for sql in [membership.as_str(), route.as_str()] {
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
    fn render_preserves_cross_owner_fragments_and_leaves_no_placeholders() {
        let membership = render_legal_reviewer_membership("assignments.$&");
        assert!(membership.contains("(assignments.$&)"));
        assert!(!membership.contains("{assignments}"));
        let route = render_legal_approval_current_route("approval.$1");
        assert!(route.contains("(approval.$1)"));
        assert!(!route.contains("{approvalInstances}"));
    }
}
