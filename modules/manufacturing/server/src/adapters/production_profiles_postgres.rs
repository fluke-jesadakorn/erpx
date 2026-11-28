//! sqlx/Postgres execution of the production accounting-profiles owner read.
//! This is the only layer in the slice that opens a transaction and executes
//! SQL; the application handler never sees a connection or a row.

use crate::application::production_profiles::{
    AccountingProfilesRead, AccountingProfilesReadFuture,
};
use crate::domain::production_profiles::{ACCOUNTING_PROFILES_SQL, AccountingProfileRow};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

type Error = PgTransactionError<BusinessError>;

fn unavailable_business() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_PRODUCTION_PROFILES_UNAVAILABLE",
        "The native Manufacturing accounting-profiles read is unavailable.",
    )
}

fn unavailable() -> Error {
    unavailable_business().into()
}

/// Postgres-backed owner reader. The statement is fixed in the domain; only
/// the validated company scope is bound, and the full nine-column projection
/// is mapped back to the domain row without a TypeScript execution fallback.
pub struct PgProductionProfilesRead {
    pool: DatabasePool,
}

impl PgProductionProfilesRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl AccountingProfilesRead for PgProductionProfilesRead {
    fn profiles<'a>(
        &'a self,
        company_id: &'a str,
    ) -> AccountingProfilesReadFuture<'a, Vec<AccountingProfileRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company_id = company_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company_id = company_id.clone();
                Box::pin(async move {
                    let rows = sqlx::query(ACCOUNTING_PROFILES_SQL)
                        .bind(company_id.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|row| {
                            Ok(AccountingProfileRow {
                                id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                status: row
                                    .try_get::<String, _>("status")
                                    .map_err(|_| unavailable())?,
                                version: row
                                    .try_get::<i64, _>("version")
                                    .map_err(|_| unavailable())?,
                                currency: row
                                    .try_get::<String, _>("currency")
                                    .map_err(|_| unavailable())?,
                                chart_id: row
                                    .try_get::<String, _>("chart_id")
                                    .map_err(|_| unavailable())?,
                                inventory_account_id: row
                                    .try_get::<String, _>("inventory_account_id")
                                    .map_err(|_| unavailable())?,
                                wip_account_id: row
                                    .try_get::<String, _>("wip_account_id")
                                    .map_err(|_| unavailable())?,
                                labor_account_id: row
                                    .try_get::<String, _>("labor_account_id")
                                    .map_err(|_| unavailable())?,
                                overhead_account_id: row
                                    .try_get::<String, _>("overhead_account_id")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, Error>>()
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable_business(),
            })
        })
    }
}
