//! Native-owned package-obligation read contract.
//!
//! The exact statement that retained the company-wide Legal obligations lived
//! in TypeScript and was built from `featureSpecs` at call time. The SQL now
//! lives in this owner's JSON contract, bound into Rust with `include_str!` and
//! rendered by the TypeScript adapter through its native contract port. No
//! statement token remains in TypeScript. The caller still prepares the
//! returned statement on its own open Holding transaction, so the read keeps
//! observing uncommitted grants, holds, shares and feature rows under the same
//! source locks as the retired literal.
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use std::sync::OnceLock;

/// Allow-listed package-obligation read kind.
pub const RD_PACKAGE_OBLIGATIONS: &str = "legal_package_obligations";

/// Native-owned SQL contract for the retained package-obligation read.
pub const PACKAGE_OBLIGATION_SOURCES_JSON: &str =
    include_str!("package-obligation-plan-sources.json");

/// Exact opening of the retired statement. The caller binds
/// `[companyId, now]` and rejects any other plan.
pub const PACKAGE_OBLIGATION_PREFIX: &str =
    "WITH scope AS(SELECT ?::text company_id,?::bigint now_ms)";

/// Feature tables in `featureSpecs` declaration order. Compile-owned names; no
/// request value selects a relation.
const FEATURE_TABLES: [&str; 10] = [
    "legal_case_events",
    "legal_compliance_actions",
    "legal_privacy_cases",
    "legal_corporate_actions",
    "legal_ip_actions",
    "legal_counsel_entries",
    "legal_obligation_schedule",
    "legal_custody_events",
    "legal_policy_acknowledgements",
    "legal_intake_questions",
];

#[derive(serde::Deserialize)]
struct PackageObligationSources {
    sql: String,
}

static PACKAGE_OBLIGATIONS: OnceLock<PackageObligationSources> = OnceLock::new();

fn package_obligations() -> &'static PackageObligationSources {
    PACKAGE_OBLIGATIONS.get_or_init(|| {
        serde_json::from_str(PACKAGE_OBLIGATION_SOURCES_JSON)
            .expect("legal package-obligation SQL contract must be valid JSON")
    })
}

/// Exact owner statement for the package-obligation read.
pub fn package_obligation_plan_sql() -> &'static str {
    &package_obligations().sql
}

/// One planned owner read. `sql` is the exact statement the retired TypeScript
/// literal bound and `values` is the positional payload in bind order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalReadStatement {
    pub sql: String,
    pub values: Vec<Value>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "LEGAL_READ_PLAN_INVALID",
        "Use one bounded native Legal read plan.",
    )
}

/// Plan one registered Legal read. Only the allow-listed kind and its exact
/// positional payload are accepted; every statement is read-only.
pub fn plan_legal_read(kind: &str, values: &[Value]) -> Result<LegalReadStatement, BusinessError> {
    match kind {
        RD_PACKAGE_OBLIGATIONS => {
            if values.len() != 2 {
                return Err(invalid());
            }
            values[0]
                .as_str()
                .filter(|value| !value.is_empty() && value.len() <= 200)
                .ok_or_else(invalid)?;
            values[1]
                .as_i64()
                .filter(|value| *value >= 0)
                .ok_or_else(invalid)?;
            Ok(LegalReadStatement {
                sql: package_obligation_plan_sql().to_owned(),
                values: values.to_vec(),
            })
        }
        _ => Err(invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    fn values(value: Value) -> Vec<Value> {
        value.as_array().cloned().unwrap_or_default()
    }

    fn digest(sql: &str) -> String {
        format!("{:x}", Sha256::digest(sql.as_bytes()))
    }

    #[test]
    fn package_obligations_contract_is_bounded_read_only_and_complete() {
        let sql = package_obligation_plan_sql();
        assert_eq!(
            digest(sql),
            "3fdd9e28883538f8789b109976a8f6075cafc4e63e906cf9eb1e42c2fd787ed7"
        );
        assert_eq!(sql.len(), 6505);
        assert_eq!(sql.matches('?').count(), 2);
        assert!(sql.starts_with(PACKAGE_OBLIGATION_PREFIX));
        assert!(sql.ends_with(" ORDER BY kind,id,fact"));
        for table in FEATURE_TABLES {
            assert_eq!(
                sql.matches(&format!("FROM {table} f")).count(),
                1,
                "{table}"
            );
            assert_eq!(
                sql.matches(&format!("'{table}' kind")).count(),
                1,
                "{table}"
            );
        }
        for kind in [
            "'item'",
            "'hold'",
            "'share'",
            "'executed-review'",
            "'approval'",
            "'ai-job'",
            "'processing-job'",
            "'invitation-job'",
            "'destruction'",
            "'intake-form'",
            "'retention-policy'",
        ] {
            assert!(sql.contains(kind), "{kind}");
        }
        assert_eq!(sql.matches(" UNION ALL ").count(), 20);
        for write in [
            "INSERT INTO",
            "UPDATE ",
            "DELETE FROM",
            "LOCK TABLE",
            "FOR UPDATE",
            "FOR SHARE",
        ] {
            assert!(!sql.contains(write), "{write}");
        }
    }

    #[test]
    fn package_obligations_plan_returns_the_contract_and_binds() {
        let statement = plan_legal_read(
            RD_PACKAGE_OBLIGATIONS,
            &values(json!(["company", 1700000000000i64])),
        )
        .unwrap();
        assert_eq!(statement.sql, package_obligation_plan_sql());
        assert_eq!(
            statement.values,
            vec![json!("company"), json!(1700000000000i64)]
        );
    }

    #[test]
    fn package_obligations_plan_rejects_unknown_kinds_and_invalid_values() {
        assert!(plan_legal_read("legal_other", &values(json!(["company", 1]))).is_err());
        assert!(plan_legal_read(RD_PACKAGE_OBLIGATIONS, &[]).is_err());
        assert!(plan_legal_read(RD_PACKAGE_OBLIGATIONS, &values(json!(["company"]))).is_err());
        assert!(
            plan_legal_read(RD_PACKAGE_OBLIGATIONS, &values(json!(["company", 1, 2]))).is_err()
        );
        assert!(plan_legal_read(RD_PACKAGE_OBLIGATIONS, &values(json!([1, 1]))).is_err());
        assert!(plan_legal_read(RD_PACKAGE_OBLIGATIONS, &values(json!(["", 1]))).is_err());
        assert!(plan_legal_read(RD_PACKAGE_OBLIGATIONS, &values(json!(["company", -1]))).is_err());
        assert!(
            plan_legal_read(RD_PACKAGE_OBLIGATIONS, &values(json!(["x".repeat(201), 1]))).is_err()
        );
    }
}
