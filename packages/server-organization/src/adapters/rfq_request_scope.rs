//! Organization-owned facts used by Support's request scope validation.
//! These reads do not establish the actor's membership or any permission.
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;

pub async fn rfq_scope_company(c: &mut PgConnection, company: &str) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$1 AND kind='company' AND status='active')")
        .bind(company).fetch_one(c).await?)
}
pub async fn rfq_scope_branch(
    c: &mut PgConnection,
    company: &str,
    branch: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind='branch' AND status='active')")
        .bind(branch).bind(company).fetch_one(c).await?)
}
pub async fn rfq_scope_department(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    department: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind='department' AND branch_id IS NOT DISTINCT FROM $3 AND status='active')")
        .bind(department).bind(company).bind(branch).fetch_one(c).await?)
}
