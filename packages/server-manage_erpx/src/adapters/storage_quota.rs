//! Borrowed Company storage facts. Caller retains the owner transaction and
//! acquires lock_beta_policy_sources before its first snapshot.
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};
pub async fn company_storage_limit(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<Option<i64>, PgTransactionError<BusinessError>> {
    if !c.is_in_transaction() {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "TRANSACTION_REQUIRED",
            "A caller transaction is required.",
        )
        .into());
    }
    let row =
        sqlx::query_scalar::<_, i64>("SELECT storage_bytes FROM beta_policy WHERE company_id=$1")
            .bind(company.as_str())
            .fetch_optional(c)
            .await?;
    Ok(row)
}
