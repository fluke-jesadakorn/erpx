//! Identity-owned recipients for the Workflow PMS social projector.
//!
//! Exact port of `packages/server-identity_access/src/adapters/pms-social-event-recipients.ts`
//! plus `pmsSourceReadableSql` from its PMS source authorization adapter.
//!
//! The retained `pmsSourcesSql` union is owned by PMS, but Identity cannot
//! depend on PMS without a package cycle (PMS reaches Identity through Platform
//! Administration). This module therefore keeps a byte-for-byte copy of the
//! union; keep it equal to `packages/server-pms/src/domain/social_sources.rs`.
use erpx_server_runtime::public::postgres::PersistenceError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;

const PMS_SOURCES_SQL: &str = "SELECT r.id,r.property_id,r.kind,r.data_json::jsonb data,r.version,r.updated_at,\n    p.company_id,p.branch_id,p.name property_name,p.business_date\n  FROM pms_records r JOIN pms_properties p ON p.id=r.property_id\n  UNION ALL\n  SELECT t.id,t.property_id,'transfer',jsonb_build_object('state',t.state,'recordedBy',t.recorded_by,\n    'guestAccountId',t.guest_account_id),t.version,t.updated_at,p.company_id,p.branch_id,p.name,p.business_date\n  FROM pms_transfer_requests t JOIN pms_properties p ON p.id=t.property_id AND p.company_id=t.company_id\n  UNION ALL\n  SELECT c.id,c.property_id,'correction',jsonb_build_object('state',c.status,'requestedBy',c.requested_by,\n    'guestAccountId',c.guest_account_id),c.version,c.updated_at,p.company_id,p.branch_id,p.name,p.business_date\n  FROM pms_charge_corrections c JOIN pms_properties p ON p.id=c.property_id AND p.company_id=c.company_id";

/// Exact `pmsSourceReadableSql("pr", "a.user_id", "a.id")` expansion.
fn pms_source_readable_sql() -> String {
    let allows =
        |permission: &str| format!("iam_role_allows(pa.role,pa.company_id,'{permission}')");
    format!(
        "EXISTS(SELECT 1 FROM assignments pa WHERE pa.user_id=a.user_id AND pa.company_id=pr.company_id\n\
         AND pa.id=a.id\n\
         AND pa.revoked_at IS NULL AND iam_assignment_active(pa.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n\
         AND iam_principal_active(pa.user_id,pa.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n\
         AND pa.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint\n\
         AND pa.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint\n\
         AND (pa.scope_kind='holding' OR (pa.scope_kind='branch' AND pa.scope_node_id=pr.branch_id))\n\
         AND EXISTS(SELECT 1 FROM organization_nodes pn WHERE pn.id=pr.branch_id AND pn.company_id=pr.company_id AND pn.status='active')\n\
         AND ({pms_read} OR ({finance_read} AND pr.kind IN ('guest_account','transfer'))) AND (\n\
           (pr.kind IN ('room','task','block') AND ({housekeeping} OR {inspect} OR {maintenance}))\n\
           OR (pr.kind IN ('stay','reservation') AND ({frontdesk} OR {reservations} OR {cashier}))\n\
           OR (pr.kind='guest_account' AND ({cashier} OR {reports}))\n\
           OR (pr.kind IN ('refund','correction') AND ({approve} OR ({cashier} AND pr.data->>'requestedBy'=pa.user_id)))\n\
           OR (pr.kind='transfer' AND ({finance_approve} OR ({cashier} AND pr.data->>'recordedBy'=pa.user_id)))\n\
           OR (pr.kind='shift' AND ({approve} OR {audit} OR ({cashier} AND pr.data->>'userId'=pa.user_id)))\n\
           OR (pr.kind='night' AND ({audit} OR {reports}))\n\
         ))",
        pms_read = allows("pms.read"),
        finance_read = allows("finance.read"),
        housekeeping = allows("pms.housekeeping"),
        inspect = allows("pms.inspect"),
        maintenance = allows("pms.maintenance"),
        frontdesk = allows("pms.frontdesk"),
        reservations = allows("pms.reservations"),
        cashier = allows("pms.cashier"),
        reports = allows("pms.reports"),
        approve = allows("pms.approve"),
        finance_approve = allows("finance.approve"),
        audit = allows("pms.audit"),
    )
}

/// Exact retained recipient selection. `review` and `recipient` are exclusive:
/// a review permission filters by role, otherwise the recipient is the same
/// person as the actor's recorded/requested/assigned user.
pub async fn pms_social_recipients(
    connection: &mut PgConnection,
    id: &str,
    company_id: &str,
    actor_id: &str,
    review: Option<&str>,
    recipient: Option<&str>,
) -> Result<Vec<String>, BusinessError> {
    let sql = format!(
        "SELECT DISTINCT a.user_id FROM assignments a\n\
         JOIN ({PMS_SOURCES_SQL}) pr ON pr.id=$1 AND pr.company_id=a.company_id\n\
         WHERE a.company_id=$2 AND {readable}\n\
         AND NOT iam_same_person(a.user_id,$3,a.company_id)\n\
         AND (($4::text IS NOT NULL AND iam_role_allows(a.role,a.company_id,$5)) OR ($6::text IS NULL AND iam_same_person(a.user_id,$7,a.company_id)))",
        readable = pms_source_readable_sql(),
    );
    sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(company_id)
        .bind(actor_id)
        .bind(review)
        .bind(review)
        .bind(review)
        .bind(recipient)
        .fetch_all(&mut *connection)
        .await
        .map_err(read_failed)
}

fn read_failed(error: sqlx::Error) -> BusinessError {
    let persistence = PersistenceError::from(error);
    if persistence.retryable() {
        BusinessError::new(
            FailureKind::Conflict,
            "IAM_SOCIAL_RECIPIENTS_RETRY_REQUIRED",
            "The PMS social recipients must be retried.",
        )
    } else {
        BusinessError::new(
            FailureKind::Unavailable,
            "IAM_SOCIAL_RECIPIENTS_UNAVAILABLE",
            "The PMS social recipients could not be read.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_sql_keeps_hard_permissions_and_source_scopes() {
        let sql = pms_source_readable_sql();
        assert!(sql.contains("pa.id=a.id"));
        assert!(sql.contains("iam_role_allows(pa.role,pa.company_id,'pms.read')"));
        assert!(sql.contains("pr.kind IN ('refund','correction')"));
        assert!(sql.contains("pr.data->>'requestedBy'=pa.user_id"));
        assert!(sql.contains("pr.data->>'recordedBy'=pa.user_id"));
        assert!(sql.contains("pr.data->>'userId'=pa.user_id"));
        assert!(sql.contains("organization_nodes pn"));
    }

    #[test]
    fn sources_union_is_the_exact_retained_three_branch_union() {
        assert_eq!(PMS_SOURCES_SQL.matches("UNION ALL").count(), 2);
        assert!(PMS_SOURCES_SQL.contains("FROM pms_transfer_requests t"));
        assert!(PMS_SOURCES_SQL.contains("FROM pms_charge_corrections c"));
    }
}
