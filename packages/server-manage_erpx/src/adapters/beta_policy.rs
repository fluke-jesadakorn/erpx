use crate::{application::beta_policy::BetaPolicyReadPort, domain::beta_policy::CompanyBetaPolicy};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

/// The owning authorization coordinator calls this before its first snapshot.
/// Acquiring a table lock after an older SERIALIZABLE snapshot is insufficient.
pub async fn lock_beta_policy_sources(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !connection.is_in_transaction() {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "TRANSACTION_REQUIRED",
            "A caller transaction is required.",
        )
        .into());
    }
    sqlx::query("LOCK TABLE beta_policy IN SHARE MODE")
        .execute(connection)
        .await?;
    Ok(())
}
pub struct PgBetaPolicyReader<'c> {
    connection: &'c mut PgConnection,
}
impl<'c> PgBetaPolicyReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}
impl BetaPolicyReadPort for PgBetaPolicyReader<'_> {
    type Error = PgTransactionError<BusinessError>;
    async fn company_beta_policy(
        &mut self,
        company: &CompanyId,
    ) -> Result<CompanyBetaPolicy, Self::Error> {
        // A fact reader never opens, commits, rolls back or retries an owner's transaction.
        if !self.connection.is_in_transaction() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "TRANSACTION_REQUIRED",
                "A caller transaction is required.",
            )
            .into());
        }
        let enabled = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM beta_policy WHERE company_id=$1)",
        )
        .bind(company.as_str())
        .fetch_one(&mut *self.connection)
        .await?;
        Ok(CompanyBetaPolicy {
            company_id: company.clone(),
            enabled,
        })
    }
}
