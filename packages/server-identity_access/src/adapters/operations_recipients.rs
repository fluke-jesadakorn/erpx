//! Identity-owned recipient validation for Operations owner assignment.
//! Returns authority facts only; the caller still owns the command decision.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

/// Validate an active assignment for the candidate in the exception scope.
/// `review` requires records.review; otherwise records.write is required.
pub async fn operations_recipient_allowed(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    user_id: &str,
    review: bool,
    now: i64,
) -> Result<bool, Error> {
    if !c.is_in_transaction() || company.is_empty() || user_id.is_empty() || user_id.len() > 200 {
        return Err(PersistenceError::new("OPERATIONS_RECIPIENT_INVALID").into());
    }
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM assignments a \
         WHERE a.company_id=$1 AND a.user_id=$2 AND a.revoked_at IS NULL \
           AND a.valid_from<=$3 AND a.valid_until>$3 \
           AND iam_assignment_active(a.id,$3) \
           AND iam_principal_active(a.user_id,a.company_id,$3) \
           AND iam_role_allows(a.role,a.company_id, CASE WHEN $6 THEN 'records.review' ELSE 'records.write' END) \
           AND (a.scope_kind='holding' \
             OR (a.scope_kind='branch' AND $4::text IS NOT NULL AND a.scope_node_id=$4) \
             OR (a.scope_kind='department' AND $5::text IS NOT NULL AND a.scope_node_id=$5)))",
    )
    .bind(company)
    .bind(user_id)
    .bind(now)
    .bind(branch)
    .bind(department)
    .bind(review)
    .fetch_one(c)
    .await?;
    Ok(allowed)
}
