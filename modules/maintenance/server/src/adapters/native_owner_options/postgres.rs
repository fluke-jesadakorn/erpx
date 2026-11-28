//! sqlx execution for the native Maintenance owner-option read.

use super::application::{OwnerOptionRead, OwnerOptionReadFuture};
use super::domain::{OwnerOption, OwnerScope, OWNER_OPTIONS_SQL};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MAINTENANCE_OWNER_OPTIONS_READ_UNAVAILABLE",
        "The native Maintenance owner-option read is unavailable.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MAINTENANCE_OWNER_OPTIONS_ROW_INVALID",
        "The owner returned an invalid owner-option row.",
    )
}

pub struct PgOwnerOptionRead {
    pool: DatabasePool,
}

impl PgOwnerOptionRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl OwnerOptionRead for PgOwnerOptionRead {
    fn owners<'a>(
        &'a self,
        scope: &'a OwnerScope,
    ) -> OwnerOptionReadFuture<'a, Vec<OwnerOption>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = scope.company_id.clone();
        Box::pin(async move {
            let company = company.clone();
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    let rows = sqlx::query(OWNER_OPTIONS_SQL)
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.iter()
                        .map(|row| {
                            Ok(OwnerOption {
                                id: row.try_get("id").map_err(|_| invalid())?,
                                name: row.try_get("name").map_err(|_| invalid())?,
                            })
                        })
                        .collect()
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_is_bound_to_the_pinned_statement() {
        assert_eq!(OWNER_OPTIONS_SQL.matches("$1").count(), 1);
        assert!(OWNER_OPTIONS_SQL.contains("p.company_id=$1"));
        assert!(!OWNER_OPTIONS_SQL.contains("LOCK"));
    }
}
