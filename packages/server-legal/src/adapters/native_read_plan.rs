//! Owner-local caller-unit read plans for the Legal package-obligation family.
//!
//! The mounted `legal/command-plan` route resolves one allow-listed kind here
//! and returns the exact statement for the caller to prepare on its own open
//! Holding transaction. No SQL is accepted from the wire and no TypeScript
//! literal remains for this family, so the retention read keeps observing the
//! caller's uncommitted grants, holds, shares and feature rows under the same
//! source locks as the retired statement.
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

/// Allow-listed package-obligation read kind.
pub const RD_PACKAGE_OBLIGATIONS: &str = "legal_package_obligations";

/// Exact opening of the retired statement. The caller binds
/// `[companyId, now]` and rejects any other plan.
pub const PACKAGE_OBLIGATION_PREFIX: &str =
    "WITH scope AS(SELECT ?::text company_id,?::bigint now_ms)";
const PACKAGE_OBLIGATION_SUFFIX: &str = " ORDER BY kind,id,fact";

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

fn feature_union() -> String {
    FEATURE_TABLES
        .iter()
        .map(|table| {
            format!(
                "SELECT '{table}' kind,f.id,to_jsonb(f)::text fact FROM {table} f \
                 LEFT JOIN legal_items i ON i.id=f.item_id,scope \
                 WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND \
                 (i.id IS NULL OR f.state NOT IN ('completed','closed','cancelled','acknowledged','waived','accepted','billed') \
                 AND NOT(f.category='custody' AND f.state='received'))"
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ")
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
            let sql = format!(
                "{PACKAGE_OBLIGATION_PREFIX} \
                 SELECT 'item' kind,i.id,to_jsonb(i)::text fact FROM legal_items i,scope \
                 WHERE i.company_id=scope.company_id AND i.state NOT IN ('archived','closed','completed','expired','terminated','cancelled','rejected') \
                 UNION ALL {features} \
                 UNION ALL SELECT 'hold',h.id,to_jsonb(h)::text FROM legal_holds h LEFT JOIN legal_items i ON i.id=h.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND h.state<>'released' \
                 UNION ALL SELECT 'share',s.id,to_jsonb(s)::text FROM legal_shares s LEFT JOIN legal_items i ON i.id=s.item_id LEFT JOIN legal_documents d ON d.id=s.document_id AND d.item_id=s.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR d.id IS NULL OR s.revoked_at IS NULL AND s.expires_at>scope.now_ms) \
                 UNION ALL SELECT 'executed-review',e.id,to_jsonb(e)::text FROM legal_executed_copies e LEFT JOIN legal_items i ON i.id=e.item_id LEFT JOIN legal_documents d ON d.id=e.document_id AND d.item_id=e.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR d.id IS NULL OR e.state<>'confirmed') \
                 UNION ALL SELECT 'approval',r.item_id,to_jsonb(r)::text FROM legal_approval_routes r LEFT JOIN legal_items i ON i.id=r.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR i.state IN ('review','approval','signing')) \
                 UNION ALL SELECT 'ai-job',j.id,to_jsonb(j)::text FROM legal_ai_proposals j LEFT JOIN legal_items i ON i.id=j.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR j.state IN ('queued','running','proposed')) \
                 UNION ALL SELECT 'processing-job',j.id,to_jsonb(j)::text FROM legal_processing_jobs j LEFT JOIN legal_items i ON i.id=j.item_id,scope WHERE (j.company_id=scope.company_id OR i.company_id=scope.company_id) AND (i.id IS NULL OR i.company_id<>j.company_id OR j.state IN ('queued','running')) \
                 UNION ALL SELECT 'invitation-job',j.id,to_jsonb(j)::text FROM legal_invitation_jobs j LEFT JOIN legal_shares s ON s.id=j.share_id LEFT JOIN legal_items i ON i.id=s.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR j.state IN ('queued','running')) \
                 UNION ALL SELECT 'destruction',d.id,to_jsonb(d)::text FROM legal_destructions d LEFT JOIN legal_items i ON i.id=d.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR d.state NOT IN ('completed','cancelled','rejected')) \
                 UNION ALL SELECT 'intake-form',f.id,to_jsonb(f)::text FROM legal_intake_forms f,scope WHERE f.company_id=scope.company_id AND f.state='draft' \
                 UNION ALL SELECT 'retention-policy',p.id,to_jsonb(p)::text FROM legal_retention_policies p,scope WHERE p.company_id=scope.company_id AND p.state='draft'\
                 {PACKAGE_OBLIGATION_SUFFIX}",
                features = feature_union(),
            );
            Ok(LegalReadStatement {
                sql,
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

    fn values(value: Value) -> Vec<Value> {
        value.as_array().cloned().unwrap_or_default()
    }

    #[test]
    fn package_obligations_plan_is_bounded_read_only_and_complete() {
        let statement = plan_legal_read(
            RD_PACKAGE_OBLIGATIONS,
            &values(json!(["company", 1700000000000])),
        )
        .unwrap();
        assert_eq!(statement.sql.matches('?').count(), 2);
        assert!(statement
            .sql
            .starts_with("WITH scope AS(SELECT ?::text company_id,?::bigint now_ms)"));
        assert!(statement.sql.ends_with("ORDER BY kind,id,fact"));
        for table in FEATURE_TABLES {
            assert_eq!(statement.sql.matches(table).count(), 1, "{table}");
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
            assert!(statement.sql.contains(kind), "{kind}");
        }
        assert_eq!(statement.sql.matches(" UNION ALL ").count(), 20);
        for write in [
            "INSERT INTO",
            "UPDATE ",
            "DELETE FROM",
            "LOCK TABLE",
            "FOR UPDATE",
            "FOR SHARE",
        ] {
            assert!(!statement.sql.contains(write), "{write}");
        }
        assert_eq!(
            statement.values,
            vec![json!("company"), json!(1700000000000)]
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
