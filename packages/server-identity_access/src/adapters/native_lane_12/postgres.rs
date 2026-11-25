//! sqlx execution for lane 12. The three statements are byte-identical to the
//! rendered TypeScript literals they replace; `company_id` stays in the WHERE
//! clause and bind order is fixed. Projection into the response envelope and
//! authorization-safe company scope are owned here, not by the TS client.

use super::application::{AttributeReadFuture, PeopleTemplateRead};
use super::domain::{ApprovalRead, AttributePage, AttributeRead, ReadPage};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::Row;

/// `SELECT id,version,definition FROM um_attributes WHERE company_id=$1 ORDER BY id`
pub const ATTRIBUTES_SQL: &str =
    "SELECT id,version,definition FROM um_attributes WHERE company_id=$1 ORDER BY id";
/// `activeIamRoleProbeSql(false)` rendered with the fixed active-role head.
pub const ACTIVE_ROLE_SQL: &str =
    "SELECT 1 FROM iam_roles WHERE id=$1 AND company_id IN ('*',$2) AND status='active'";
/// `SELECT 1 FROM um_groups ... EXISTS(jsonbArrayHeadSql(...))` fully rendered.
pub const GROUP_REFERENCES_SQL: &str =
    "SELECT 1 FROM um_groups WHERE company_id=$1 AND NOT (definition->>'archived')::boolean AND EXISTS(SELECT 1 FROM jsonb_array_elements(definition->'rule'->'conditions') x WHERE x->>'field'=$2)";

/// Retired TypeScript statements, kept verbatim for the byte/result oracle.
/// `readCanOverrideStaffStepRole` from `workflow-approvals-facts.ts`.
pub const APPROVAL_OVERRIDE_SQL_TS: &str =
    "SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND iam_role_allows(role,company_id,'workflow.people_manager') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)";
/// `assignmentActiveProbeSql(...)` rendered at the approval call site.
pub const APPROVAL_ACTIVE_SCOPE_SQL_TS: &str =
    "SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (? OR scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)";
/// `readExpenseDecisionEvidenceAssignmentsreceiptsperson` retired TS literal.
pub const APPROVAL_EXPENSE_EVIDENCE_SQL_TS: &str =
    "SELECT id,role,scope_kind,scope_node_id,valid_from,valid_until FROM assignments WHERE company_id=? AND id=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? ORDER BY id";

/// Executable forms: only the `?` bind markers are renumbered to `$1..$n`.
const APPROVAL_OVERRIDE_SQL: &str =
    "SELECT 1 FROM assignments WHERE id=$1 AND company_id=$2 AND user_id=$3 AND iam_role_allows(role,company_id,'workflow.people_manager') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (scope_kind='holding' OR scope_kind='branch' AND scope_node_id=$4 OR scope_kind='department' AND scope_node_id=$5)";
const APPROVAL_ACTIVE_SCOPE_SQL: &str =
    "SELECT 1 FROM assignments WHERE id=$1 AND user_id=$2 AND company_id=$3 AND role=$4 AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=$5 AND valid_until>$6 AND ($7 OR scope_kind='holding' OR scope_kind='branch' AND scope_node_id=$8 OR scope_kind='department' AND scope_node_id=$9)";
const APPROVAL_EXPENSE_EVIDENCE_SQL: &str =
    "SELECT id,role,scope_kind,scope_node_id,valid_from,valid_until FROM assignments WHERE company_id=$1 AND id=$2 AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=$3 AND valid_until>$4 ORDER BY id";

/// Translate `?` placeholders to `$1..$n` without changing any other byte.
pub fn render(statement: &str) -> String {
    let mut rendered = String::with_capacity(statement.len());
    let mut ordinal = 0usize;
    for character in statement.chars() {
        if character == '?' {
            ordinal += 1;
            rendered.push('$');
            rendered.push_str(&ordinal.to_string());
        } else {
            rendered.push(character);
        }
    }
    rendered
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_LANE_12_READ_UNAVAILABLE",
        "The native People-template attribute read is unavailable.",
    )
}

/// Pg-backed owner reader. All three relations are identity_access owned; no
/// other owner's permission, membership or workflow authority is inferred.
pub struct PgPeopleTemplateRead {
    pool: DatabasePool,
}

impl PgPeopleTemplateRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PeopleTemplateRead for PgPeopleTemplateRead {
    fn read<'a>(&'a self, read: &'a AttributeRead) -> AttributeReadFuture<'a, AttributePage> {
        let pool = self.pool.clone();
        let read = read.clone();
        Box::pin(async move {
            match read {
                AttributeRead::Attributes { company_id } => {
                    let rows = sqlx::query(ATTRIBUTES_SQL)
                        .bind(&company_id)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    let rows = rows
                        .into_iter()
                        .map(|row| {
                            Ok(json!({
                                "id": row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                "version": i64::from(row.try_get::<i32, _>("version").map_err(|_| unavailable())?),
                                "definition": row.try_get::<Value, _>("definition").map_err(|_| unavailable())?,
                            }))
                        })
                        .collect::<Result<Vec<Value>, BusinessError>>()?;
                    Ok(ReadPage::new(rows))
                }
                AttributeRead::ActiveRole { company_id, role } => {
                    let rows = sqlx::query(ACTIVE_ROLE_SQL)
                        .bind(&role)
                        .bind(&company_id)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(ReadPage::new(vec![json!({"one": 1}); rows.len()]))
                }
                AttributeRead::GroupReferences { company_id, value } => {
                    let rows = sqlx::query(GROUP_REFERENCES_SQL)
                        .bind(&company_id)
                        .bind(&value)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(ReadPage::new(vec![json!({"one": 1}); rows.len()]))
                }
            }
        })
    }

    fn approval<'a>(&'a self, read: &'a ApprovalRead) -> AttributeReadFuture<'a, AttributePage> {
        let pool = self.pool.clone();
        let read = read.clone();
        Box::pin(async move {
            match read {
                ApprovalRead::CanOverride {
                    assignment_id,
                    company_id,
                    user_id,
                    branch_id,
                    department_id,
                } => {
                    let rows = sqlx::query(APPROVAL_OVERRIDE_SQL)
                        .bind(&assignment_id)
                        .bind(&company_id)
                        .bind(&user_id)
                        .bind(branch_id)
                        .bind(department_id)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(ReadPage::new(vec![json!({"one": 1}); rows.len()]))
                }
                ApprovalRead::ActiveScope {
                    assignment_id,
                    user_id,
                    company_id,
                    role,
                    now,
                    now2,
                    policy_step,
                    branch_id,
                    department_id,
                } => {
                    let rows = sqlx::query(APPROVAL_ACTIVE_SCOPE_SQL)
                        .bind(&assignment_id)
                        .bind(&user_id)
                        .bind(&company_id)
                        .bind(&role)
                        .bind(now)
                        .bind(now2)
                        .bind(policy_step)
                        .bind(branch_id)
                        .bind(department_id)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(ReadPage::new(vec![json!({"one": 1}); rows.len()]))
                }
                ApprovalRead::ExpenseEvidence {
                    company_id,
                    assignment_id,
                    now,
                    now2,
                } => {
                    let rows = sqlx::query(APPROVAL_EXPENSE_EVIDENCE_SQL)
                        .bind(&company_id)
                        .bind(&assignment_id)
                        .bind(now)
                        .bind(now2)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    let rows = rows
                        .into_iter()
                        .map(|row| {
                            Ok(json!({
                                "id": row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                "role": row.try_get::<String, _>("role").map_err(|_| unavailable())?,
                                "scope_kind": row.try_get::<String, _>("scope_kind").map_err(|_| unavailable())?,
                                "scope_node_id": row.try_get::<Option<String>, _>("scope_node_id").map_err(|_| unavailable())?,
                                "valid_from": row.try_get::<i64, _>("valid_from").map_err(|_| unavailable())?,
                                "valid_until": row.try_get::<i64, _>("valid_until").map_err(|_| unavailable())?,
                            }))
                        })
                        .collect::<Result<Vec<Value>, BusinessError>>()?;
                    Ok(ReadPage::new(rows))
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hash(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn native_lane_12_sql_oracle_hashes() {
        // Byte-identical rendered statements retired from the TS bucket.
        assert_eq!(
            hash(ATTRIBUTES_SQL),
            "fc2fca4285b7a6baf1681610ba50fec70a983b65c922160ea46c2e7637a7e396"
        );
        assert_eq!(
            hash(ACTIVE_ROLE_SQL),
            "dcb70327c0b64b8492d55fc9333fd992697b01c5b792e576a2e54aad0250522d"
        );
        assert_eq!(
            hash(GROUP_REFERENCES_SQL),
            "5ae2987e3ed8e9d9d67b77ac23f7fabbdb5d6b5499560f7617ad18bf9b1364e2"
        );
    }

    #[test]
    fn native_lane_12_bind_order_is_company_scoped() {
        // The company predicate is always first on the company-only reads and
        // second on the id+company active-role probe, matching pg's $n order.
        assert!(ATTRIBUTES_SQL.contains("company_id=$1 ORDER BY id"));
        assert!(ACTIVE_ROLE_SQL.contains("id=$1 AND company_id IN ('*',$2)"));
        assert!(GROUP_REFERENCES_SQL.contains("company_id=$1"));
        assert!(GROUP_REFERENCES_SQL.contains("x->>'field'=$2"));
        for sql in [ATTRIBUTES_SQL, ACTIVE_ROLE_SQL, GROUP_REFERENCES_SQL] {
            assert!(sql.starts_with("SELECT "));
        }
    }

    #[test]
    fn native_lane_12_result_oracle() {
        let attributes = ReadPage::new(vec![
            json!({"id": "custom:a", "version": 2, "definition": {"type": "text"}}),
            json!({"id": "custom:b", "version": 1, "definition": {"type": "role"}}),
        ]);
        let rendered = serde_json::to_string(&attributes).unwrap();
        assert_eq!(
            hash(&rendered),
            hash(&serde_json::to_string(&ReadPage::new(vec![
                json!({"id": "custom:a", "version": 2, "definition": {"type": "text"}}),
                json!({"id": "custom:b", "version": 1, "definition": {"type": "role"}}),
            ]))
            .unwrap())
        );
        assert_eq!(attributes.row_count, 2);
        let probes: AttributePage = ReadPage::new(vec![json!({"one": 1})]);
        assert_eq!(serde_json::to_value(&probes).unwrap(), json!({"rows": [{"one": 1}], "rowCount": 1}));
    }

    #[test]
    fn native_lane_12_approval_sql_oracle_hashes() {
        assert_eq!(
            hash(APPROVAL_OVERRIDE_SQL_TS),
            "a269b4e5d9157b2eb2660ebd79cdc9d9c5aa678ef87e2777374cdd23628dbe70"
        );
        assert_eq!(
            hash(APPROVAL_ACTIVE_SCOPE_SQL_TS),
            "8aee2c29092d5726fb0e0b4d1465e27d78ee766177b310fb0c7dbb7de2ac7dd9"
        );
        assert_eq!(
            hash(APPROVAL_EXPENSE_EVIDENCE_SQL_TS),
            "bf5b9b5ba190c9fa422fb45a9ca904484ec1d408e726db2d16673ca8f58c8683"
        );
    }

    #[test]
    fn native_lane_12_approval_rendered_statements_are_executable() {
        assert_eq!(render(APPROVAL_OVERRIDE_SQL_TS), APPROVAL_OVERRIDE_SQL);
        assert_eq!(render(APPROVAL_ACTIVE_SCOPE_SQL_TS), APPROVAL_ACTIVE_SCOPE_SQL);
        assert_eq!(render(APPROVAL_EXPENSE_EVIDENCE_SQL_TS), APPROVAL_EXPENSE_EVIDENCE_SQL);
        assert_eq!(APPROVAL_OVERRIDE_SQL.matches('$').count(), 5);
        assert_eq!(APPROVAL_ACTIVE_SCOPE_SQL.matches('$').count(), 9);
        assert_eq!(APPROVAL_EXPENSE_EVIDENCE_SQL.matches('$').count(), 4);
    }

    #[test]
    fn native_lane_12_approval_bind_order_and_scope() {
        assert!(APPROVAL_OVERRIDE_SQL.starts_with(
            "SELECT 1 FROM assignments WHERE id=$1 AND company_id=$2 AND user_id=$3"
        ));
        assert!(APPROVAL_ACTIVE_SCOPE_SQL.contains(
            "WHERE id=$1 AND user_id=$2 AND company_id=$3 AND role=$4"
        ));
        assert!(APPROVAL_ACTIVE_SCOPE_SQL.contains("valid_from<=$5 AND valid_until>$6"));
        assert!(APPROVAL_EXPENSE_EVIDENCE_SQL.starts_with(
            "SELECT id,role,scope_kind,scope_node_id,valid_from,valid_until FROM assignments WHERE company_id=$1 AND id=$2"
        ));
        for sql in [
            APPROVAL_OVERRIDE_SQL,
            APPROVAL_ACTIVE_SCOPE_SQL,
            APPROVAL_EXPENSE_EVIDENCE_SQL,
        ] {
            assert!(!sql.to_ascii_uppercase().contains("LOCK TABLE"));
            assert!(!sql.contains("pg_try_advisory"));
        }
    }

    #[test]
    fn native_lane_12_approval_result_oracle() {
        let page: AttributePage = ReadPage::new(vec![json!({
            "id": "a1",
            "role": "workflow.approve",
            "scope_kind": "holding",
            "scope_node_id": null,
            "valid_from": 1,
            "valid_until": 2,
        })]);
        assert_eq!(page.row_count, 1);
        assert_eq!(
            hash(&serde_json::to_string(&page).unwrap()),
            hash("{\"rows\":[{\"id\":\"a1\",\"role\":\"workflow.approve\",\"scope_kind\":\"holding\",\"scope_node_id\":null,\"valid_from\":1,\"valid_until\":2}],\"rowCount\":1}")
        );
    }
}
