//! Identity-owned staff notification recipient selection.
//!
//! This adapter resolves the current active assignments that may receive a
//! notification for one Commerce record. It is read-only and grants no command
//! authority; the owning command must still retain its real source, purchase
//! and record policy. Scope matching mirrors the retained Supply notification
//! SQL exactly: holding assignments always match, while branch and department
//! assignments match only the record's branch/department nodes.
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

/// Hard upper bound for one notification fan-out. A larger candidate set fails
/// closed instead of silently truncating recipients.
pub const MAX_NOTIFICATION_RECIPIENTS: usize = 1000;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IAM_NOTIFICATION_RECIPIENTS_UNAVAILABLE",
        "The notification recipients could not be read.",
    )
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.bytes().any(|byte| byte == 0)
}

/// Current active `commerce.purchase` or `inventory.prepare` assignments in the
/// record's Company/branch/department scope. Deduplicated and bounded.
pub async fn supplier_promise_recipients(
    c: &mut PgConnection,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
) -> Result<Vec<String>, BusinessError> {
    if !c.is_in_transaction()
        || !valid_identifier(company_id)
        || branch_id.is_some_and(|value| !valid_identifier(value))
        || department_id.is_some_and(|value| !valid_identifier(value))
    {
        return Err(unavailable());
    }
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT a.user_id FROM assignments a \
         WHERE a.company_id=$1 \
           AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
           AND (iam_role_allows(a.role,a.company_id,'commerce.purchase') \
             OR iam_role_allows(a.role,a.company_id,'inventory.prepare')) \
           AND (a.scope_kind='holding' \
             OR (a.scope_kind='branch' AND $2::text IS NOT NULL AND a.scope_node_id=$2) \
             OR (a.scope_kind='department' AND $3::text IS NOT NULL AND a.scope_node_id=$3)) \
         ORDER BY a.user_id LIMIT $4",
    )
    .bind(company_id)
    .bind(branch_id)
    .bind(department_id)
    .bind((MAX_NOTIFICATION_RECIPIENTS + 1) as i64)
    .fetch_all(&mut *c)
    .await
    .map_err(|_| unavailable())?;
    if rows.len() > MAX_NOTIFICATION_RECIPIENTS {
        return Err(unavailable());
    }
    Ok(rows)
}
