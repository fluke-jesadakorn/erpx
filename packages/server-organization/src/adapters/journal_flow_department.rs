//! Organization-owned display facts for a retained journal Flow roster.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
pub async fn lock_journal_flow_department_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ORGANIZATION_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE organization_nodes IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
/// B0 uses an active same-Company node if one exists, otherwise "Company".
pub async fn journal_flow_team_name(
    c: &mut PgConnection,
    company: &CompanyId,
    department: Option<&str>,
) -> Result<String, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ORGANIZATION_TRANSACTION_REQUIRED").into());
    }
    let Some(department) = department else {
        return Ok("Company".into());
    };
    Ok(sqlx::query_scalar::<_,String>("SELECT display_name FROM organization_nodes WHERE company_id=$1 AND id=$2 AND status='active'")
       .bind(company.as_str()).bind(department).fetch_optional(c).await?.unwrap_or_else(||"Company".into()))
}
