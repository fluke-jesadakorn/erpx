//! Current Identity roster facts for Stock Count Flow. People supplies the
//! active staff roster; this owner evaluates only IAM assignments/principals.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
fn invalid() -> Error {
    PersistenceError::new("STOCK_COUNT_FLOW_PARTICIPANTS_INVALID").into()
}
/// Caller first holds Identity and People owner locks; facts do not grant any
/// action and must be read again before transaction completion.
pub async fn service_count_flow_reviewers(
    c: &mut PgConnection,
    company: &CompanyId,
    active_staff: &[String],
    creator: &str,
    preparer: &str,
    now: i64,
) -> Result<Vec<String>, Error> {
    if !c.is_in_transaction()
        || active_staff.len() > 500
        || active_staff.iter().any(|v| v.len() > 200)
    {
        return Err(invalid());
    }
    Ok(sqlx::query_scalar("SELECT DISTINCT a.user_id FROM assignments a WHERE a.company_id=$1 AND a.user_id=ANY($2::text[]) AND a.user_id<>$3 AND a.user_id<>$4 AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=$5 AND a.valid_until>$5 AND (iam_role_allows(a.role,a.company_id,'workflow.finance_approver') OR iam_role_allows(a.role,a.company_id,'workflow.holding_owner')) ORDER BY a.user_id").bind(company.as_str()).bind(active_staff).bind(creator).bind(preparer).bind(now).fetch_all(c).await?)
}
/// Identities allowed as explicitly named owner/preparer even without HR.
pub async fn service_count_flow_active_members(
    c: &mut PgConnection,
    company: &CompanyId,
    now: i64,
) -> Result<Vec<String>, Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    let ids:Vec<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(user_id)<=200 THEN user_id END FROM (SELECT DISTINCT a.user_id FROM assignments a WHERE a.company_id=$1 AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=$2 AND a.valid_until>$2 ORDER BY a.user_id LIMIT 501) selected ORDER BY user_id").bind(company.as_str()).bind(now).fetch_all(c).await?;
    if ids.len() > 500 {
        return Err(invalid());
    }
    let ids = ids
        .into_iter()
        .map(|id| id.ok_or_else(invalid))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}
