//! sqlx execution for the native Maintenance lane-36 descriptor read.

use super::application::{DescriptorRead, DescriptorReadFuture};
use super::domain::DescriptorScope;
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, to_value};
use sqlx::Row;

/// Single owner-local descriptor statement. Both Company and target id are
/// bounded binds; no relation or predicate can be supplied by a caller.
pub const DESCRIPTOR_SQL: &str = "SELECT company_id,branch_id,department_id,version FROM maintenance_work_orders WHERE id=$1 AND company_id=$2";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MAINTENANCE_LANE36_READ_UNAVAILABLE",
        "The native Maintenance descriptor read is unavailable.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MAINTENANCE_LANE36_ROW_INVALID",
        "The Maintenance owner returned an invalid descriptor row.",
    )
}

pub struct PgDescriptorRead {
    pool: DatabasePool,
}

impl PgDescriptorRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl DescriptorRead for PgDescriptorRead {
    fn descriptor<'a>(
        &'a self,
        scope: &'a DescriptorScope,
    ) -> DescriptorReadFuture<'a, Option<Value>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let scope = scope.clone();
        Box::pin(async move {
            let target = scope.target_id.clone();
            let company = scope.company_id.clone();
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let target = target.clone();
                let company = company.clone();
                Box::pin(async move {
                    let row = sqlx::query(DESCRIPTOR_SQL)
                        .bind(target.as_str())
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.map(|row| {
                        let value = to_value(super::domain::DescriptorRow {
                            company_id: row.try_get("company_id").map_err(|_| invalid())?,
                            branch_id: row.try_get("branch_id").map_err(|_| invalid())?,
                            department_id: row.try_get("department_id").map_err(|_| invalid())?,
                            version: row.try_get("version").map_err(|_| invalid())?,
                        })
                        .map_err(|_| invalid())?;
                        Ok(value)
                    })
                    .transpose()
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
    fn statement_is_single_owner_local_bounded_read() {
        assert_eq!(
            DESCRIPTOR_SQL,
            "SELECT company_id,branch_id,department_id,version FROM maintenance_work_orders WHERE id=$1 AND company_id=$2"
        );
        assert!(!DESCRIPTOR_SQL.contains("LOCK"));
        assert!(!DESCRIPTOR_SQL.contains("UPDATE"));
        assert_eq!(DESCRIPTOR_SQL.matches("$1").count(), 1);
        assert_eq!(DESCRIPTOR_SQL.matches("$2").count(), 1);
    }
}
