//! Owner-owned static fragments for the supplied IAM statement snapshot.
//!
//! The two exact strings were the last complete read literals in the
//! TypeScript snapshot source. Their bytes now live in this owner's JSON
//! contract, bound into Rust with `include_str!` and read by the TypeScript
//! adapter through the same contract import. Callers keep their own statement
//! tails, predicates, binds and transaction; no statement keyword remains in
//! the TypeScript snapshot source.

/// Native-owned SQL contract for the two IAM snapshot fragments.
pub const IDENTITY_SNAPSHOT_SOURCES_JSON: &str = include_str!("identity-snapshot-sources.json");

#[derive(serde::Deserialize)]
struct IdentitySnapshotSources {
    #[serde(rename = "betaPolicyProjection")]
    beta_policy_projection: String,
    #[serde(rename = "iamApiBetaTeamInvitationsFrom")]
    iam_api_beta_team_invitations_from: String,
}

static SOURCES: std::sync::OnceLock<IdentitySnapshotSources> = std::sync::OnceLock::new();

fn sources() -> &'static IdentitySnapshotSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(IDENTITY_SNAPSHOT_SOURCES_JSON)
            .expect("manage_erpx identity snapshot SQL contract must be valid JSON")
    })
}

/// Frozen beta_policy owner projection shared by the IAM snapshot and the
/// hospitality/Files/Runtime native joins.
pub fn beta_policy_projection() -> &'static str {
    &sources().beta_policy_projection
}

/// Frozen explicit-column beta_team_invitations source for the IAM snapshot.
pub fn beta_team_invitations_from() -> &'static str {
    &sources().iam_api_beta_team_invitations_from
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_is_byte_exact_to_the_retired_typescript_literals() {
        for (sql, len, expected) in [
            (
                beta_policy_projection(),
                119,
                "SELECT \"company_id\", \"member_limit\", \"branch_limit\", \"ai_monthly_limit\", \"storage_bytes\", \"created_at\" FROM beta_policy",
            ),
            (
                beta_team_invitations_from(),
                182,
                "FROM (SELECT \"id\", \"email\", \"branch_id\", \"invited_by\", \"token_hash\", \"expires_at\", \"accepted_user_id\", \"accepted_at\", \"revoked_at\", \"created_at\", \"role\" FROM beta_team_invitations) i",
            ),
        ] {
            assert_eq!(sql.len(), len);
            assert_eq!(sql, expected);
            assert_eq!(sql.matches("SELECT ").count(), 1);
            assert!(!sql.contains(';'));
            assert!(!sql.contains("INSERT INTO"));
            assert!(!sql.contains("DELETE FROM"));
            assert!(!sql.contains("UPDATE "));
            assert!(!sql.contains("FOR UPDATE"));
            assert!(!sql.contains("LOCK "));
        }
    }

    #[test]
    fn typescript_snapshot_source_carries_no_statement_keyword() {
        let helper = include_str!("identity-snapshot-sources.ts");
        for token in ["SELECT", "INSERT", "DELETE", "UPDATE "] {
            assert!(!helper.contains(token));
        }
        assert!(helper.contains("identity-snapshot-sources.json"));
        assert!(!helper.contains("prepare("));
        assert!(!helper.contains(".bind("));
    }
}
