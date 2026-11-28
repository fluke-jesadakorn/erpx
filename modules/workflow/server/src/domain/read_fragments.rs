//! Native-owned Workflow read-fragment contract (owner tail).
//!
//! The remaining Workflow TypeScript statement tokens were fixed assertion,
//! shared-renderer and cross-owner projection fragments, not standalone caller
//! reads. Their exact bytes live in this owner's JSON contract, are bound into
//! Rust with `include_str!`, and are read by the TypeScript adapters. Every
//! statement still prepares, binds and executes in its original caller unit.
//!
//! Trusted substitutions (never SQL statements):
//! - `{table}` / `{tail}`: closed table choice and caller-owned predicate for
//!   the shared table-exists guard.
//! - `{expr}`: closed `erpx_json_each` expression.
//! - `{kind}`, `{alias}`, `{user}`, `{active}`: closed approval identity inputs.
//! - `{index}`: the fixed approval-index subquery below.
//! - `{role}`: `participantRoleCheck`, or empty when no assignment is active.
//!
//! Cross-owner projection contracts stay substituted by their owning package;
//! no owner projection is copied into this contract:
//! - `{union}` (local renderer): `ast.approver_user_id user_id UNION SELECT
//!   value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))`.
//! - `participantRoleCheck.{from}`: server-identity_access
//!   `approval_participant_projection_assignments_aa_from` (`FROM (SELECT ... FROM
//!   assignments) aa`).
//! - `orderFulfilledExists.{from}`: server-commerce
//!   `flow_routing_commerce_fulfillments_f_from` (`FROM (SELECT ... FROM
//!   commerce_fulfillments) f`); the finance journal join remains a separate
//!   server-finance projection appended by the caller.
//! - `payrollPaymentBatch.{from}`: server-people_payroll
//!   `flow_routing_hr_payment_batches_from` (`FROM (SELECT ... FROM
//!   hr_payment_batches) hr_payment_batches`).
//! - `financeAiRequestUsage.{from}`: server-records
//!   `finance_ai_request_sources_request_ai_usage_from` (`FROM (SELECT ... FROM
//!   request_ai_usage) request_ai_usage`).
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Native-owned Workflow read-fragment contract.
pub const WORKFLOW_READ_FRAGMENTS_SQL: &str =
    include_str!("../workflow-read-fragments-sql.json");

/// Every fixed Workflow read fragment, keyed exactly as the TypeScript contract
/// import reads it.
pub fn fragments() -> &'static BTreeMap<String, String> {
    static FRAGMENTS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    FRAGMENTS.get_or_init(|| {
        serde_json::from_str(WORKFLOW_READ_FRAGMENTS_SQL)
            .expect("workflow read-fragment SQL contract must be valid JSON")
    })
}

/// One fixed fragment, or `None` for an unknown key.
pub fn fragment(key: &str) -> Option<&'static str> {
    fragments().get(key).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn digest(value: &str) -> String {
        format!("{:x}", Sha256::digest(value.as_bytes()))
    }

    /// Byte oracles captured from the retired TypeScript literals before the
    /// read-fragment contract moved them into this owner file.
    #[test]
    fn contract_is_byte_exact_to_the_retired_typescript_tokens() {
        let expected = [
            ("selectHead", "976e3c585c9435e5846708abb0cdf6cb9cb4ce733ddfe7924fd29b6e43141ddb", 7, 0),
            ("tableExistsGuard", "4e35fec929aa42bf7155c9a03e7e0632614841ba3a09b22cf40a620ba405b95e", 41, 0),
            ("jsonEach", "a0dac20155c06addc155ecf0cc352a67c453256ce94c82d4a19c931bd8ad8b3d", 40, 0),
            ("approvalInstances", "0ead25b9add24af95a1dbed3c02b4765ee85c4948b1e86d9c981ccce8170fbfe", 251, 0),
            ("approvalSteps", "8ef0cce32ed76153265573b847df84d734e1d1cae633c5e129899479d8ed5cd0", 161, 0),
            ("approvalStepsStar", "22f3316f2549b3bc73815dfccb7e0bf81594845020f5bca761421ff25f4b60a9", 28, 0),
            ("approvalInstancesStarWhere", "713c174272c475236e4401bb8aa09fc19804f1c9ea2d5f5b957e5b818fec6b0e", 39, 0),
            ("auditEvents", "b2cb1c386524f7cc7b2a821639165f1e3b1174b4aee3b1abf4044668b74e34bd", 161, 0),
            ("approvalDecisions", "2ea420e0cfd5d4ee44a434b794bcda4eec8e536734d586c07a81e074d03c4081", 220, 0),
            ("projectWorkerAudit", "89cff893905a6d02c66bba6158532fac81d8831244bf189161a14b6fc6d6cb09", 67, 0),
            ("projectWorkerOutbox", "a3c74f02557852c3f6a06f01f26d29442395c6a4ae3d32d54dcc4b1357430f8c", 94, 0),
            ("workSettlementDecisions", "854d3725b1f5c18276fd4591f900e3e506fae71eba79f4b18d43d21d439118c6", 32, 0),
            ("workSettlementRoutineOutbox", "54abed05de6c04057ff2dc157e15999dd54dcfcff084dd5e660de50f1792fd11", 93, 0),
            ("workSettlementRoutineAudits", "57ce80d075bc36cc4a389c0e2045a8840f9b3ba2c46072a4540761d6a4a481c3", 89, 0),
            ("approvalInstanceStatePrefix", "bd81d7afd58ef3e0724e31ae6b80aa63fb509332eb2e84b8a870a263737fa425", 76, 2),
            ("pendingInstanceAndStep", "4ec2791843403261db2a55cb5b152f4ae22e6ffdd933bedf16178c24296a9903", 160, 4),
            ("noApprovalStepExists", "229084d5029d20e74ab83d16ef5384883a2356b225c2abe78d4d6b648551baae", 39, 0),
            ("participant", "76fac731a6bf439305e45d18131fbcfb30a3d752a98d33f1902dc3ad9eb183b6", 305, 0),
            ("participantRoleCheck", "1eca9519dfe30f53e183c5b38fef98fc3f291f9d720a2f5963b531bbc87b0c57", 666, 0),
            ("orderFulfilledExists", "8c2615e1a97bfa79530badbfde9dbb483a7dcc34611516d6538087703af5467c", 52, 0),
            ("flowDefaultsMaxRevision", "347395e653aa35d75b24906ef1a31338a0e8fdaa5c3eead29ada02a3cbd16d1f", 32, 0),
            ("approvalIndex", "3d7d9d3456bd774c813c84322b5168984518d187c20c19f7e8e17dbe604de721", 132, 0),
            ("expensePolicyExists", "d17b321b24d006fa0cbd8c6ef036e2af42b33a5621bc683f8ac28861a7d39519", 262, 0),
            ("payrollPaymentBatch", "2a8aebed131f5f41f738fd2bb9990739fc7186d7a432b33baf7918cdc5e6c4ff", 255, 0),
            ("financeSourceRevision", "42dd5c64614ccc6ffee4486464e43775ecac48015e4bba59124eccba2faeaadc", 316, 1),
            ("financeAiRequestUsage", "06cf16d1ee1bd3184d75045f923163d0ec01e956f9de51125aa90755743d31a5", 30, 0),
        ];
        assert_eq!(fragments().len(), expected.len());
        for (key, hash, len, binds) in expected {
            let sql = fragment(key).unwrap_or_else(|| panic!("missing fragment {key}"));
            assert_eq!(sql.len(), len, "{key} length");
            assert_eq!(digest(sql), hash, "{key} digest");
            assert_eq!(sql.matches('?').count(), binds, "{key} binds");
        }
        assert!(fragment("unknown").is_none());
    }

    /// Fragments stay read-only and only expose the documented trusted tokens.
    #[test]
    fn contract_is_read_only_and_placeholder_exact() {
        const ALLOWED: &[&str] = &[
            "{table}", "{tail}", "{expr}", "{kind}", "{alias}", "{union}", "{user}", "{role}",
            "{from}", "{active}", "{index}",
        ];
        for (key, sql) in fragments() {
            let upper = sql.to_ascii_uppercase();
            for forbidden in [
                "INSERT INTO",
                "DELETE FROM",
                "UPDATE ",
                "RETURNING",
                "FOR UPDATE",
                "FOR SHARE",
                "LOCK TABLE",
                "PG_ADVISORY",
                ";",
            ] {
                assert!(!upper.contains(forbidden), "{key} contains {forbidden}");
            }
            let mut rest = sql.as_str();
            while let Some(start) = rest.find('{') {
                let end = rest[start..]
                    .find('}')
                    .map(|offset| start + offset)
                    .unwrap_or_else(|| panic!("{key} has an unclosed placeholder"));
                let token = &rest[start..=end];
                assert!(ALLOWED.contains(&token), "{key} has unexpected token {token}");
                rest = &rest[end + 1..];
            }
        }
        assert_eq!(fragment("selectHead"), Some("SELECT "));
        assert_eq!(fragment("tableExistsGuard"), Some("EXISTS(SELECT 1 FROM {table} WHERE {tail}"));
        assert_eq!(fragment("jsonEach"), Some("SELECT value FROM erpx_json_each({expr})"));
    }

    /// Cross-owner projections are substituted by their owning package; the
    /// contract records only the placeholder and never a copied projection.
    #[test]
    fn cross_owner_projection_contracts_are_not_copied() {
        for key in [
            "participantRoleCheck",
            "orderFulfilledExists",
            "payrollPaymentBatch",
            "financeAiRequestUsage",
        ] {
            assert!(
                fragment(key).expect(key).contains("{from}"),
                "{key} must keep its cross-owner {{from}} contract"
            );
        }
        for copied in [
            r#"FROM (SELECT "id","user_id","role""#,
            r#"FROM (SELECT "id","order_id""#,
            r#"FROM (SELECT "id","company_id","source_type""#,
            r#"FROM (SELECT "id","company_id","actor_id""#,
        ] {
            assert!(
                !WORKFLOW_READ_FRAGMENTS_SQL.contains(copied),
                "contract copied a cross-owner projection: {copied}"
            );
        }
        assert!(fragment("participant").expect("participant").contains("{union}"));
        assert!(fragment("participant").expect("participant").contains("{role}"));
        assert!(fragment("expensePolicyExists").expect("expense").contains("{index}"));
    }
}
