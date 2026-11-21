//! People-owned name facts used by Records/Support. No principal or request
//! access is established by these lookups.
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;

/// Owner projection used in the original Support handler LEFT JOIN. This
/// only exposes People facts and does not assert person or assignment state.
pub fn rfq_request_people_sql() -> &'static str {
    "rfq_request_people AS (SELECT company_id,user_id,legal_name,merged_into_id FROM public.hr_people)"
}

/// Equivalent to the People subquery of the request-event projection.
pub async fn rfq_request_actor_name(
    c: &mut PgConnection,
    company: &str,
    user: &str,
) -> Result<String, Error> {
    let name: Option<String> = sqlx::query_scalar("SELECT legal_name FROM hr_people WHERE company_id=$1 AND user_id=$2 AND merged_into_id IS NULL LIMIT 1")
        .bind(company).bind(user).fetch_optional(c).await?;
    Ok(name.unwrap_or_else(|| "Company member".into()))
}

/// Same People fields as the Support handler LEFT JOIN, before Identity
/// current-principal resolution and scoped assignment eligibility.
pub async fn rfq_request_handler_name(
    c: &mut PgConnection,
    company: &str,
    user: &str,
) -> Result<String, Error> {
    let name: Option<String> = sqlx::query_scalar("SELECT legal_name FROM hr_people WHERE company_id=$1 AND user_id=$2 AND merged_into_id IS NULL LIMIT 1")
        .bind(company).bind(user).fetch_optional(c).await?;
    Ok(name.unwrap_or_else(|| user.into()))
}
