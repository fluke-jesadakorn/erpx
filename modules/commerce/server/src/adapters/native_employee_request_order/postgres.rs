//! sqlx/Postgres execution for the native employee-request order read.
//!
//! This is the only layer that executes SQL. The retired TypeScript literal is
//! kept verbatim as `SQL_EMPLOYEE_ORDER_TS`; the executable form uses `$1`/`$2`
//! bind parameters and the same six-column projection. `commerce_orders` is
//! Commerce-owned and the read stays scoped to one Company.

use super::application::{CommerceEmployeeOrderRead, EmployeeOrderFuture};
use super::domain::EmployeeOrder;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

/// Retired TypeScript literal, pinned for the parity oracle.
pub const SQL_EMPLOYEE_ORDER_TS: &str = "SELECT kind,company_id,branch_id,department_id,state,version FROM commerce_orders WHERE id=? AND company_id=?";

/// Executable statement: identical projection, Postgres binds, Company scope.
const SQL_EMPLOYEE_ORDER: &str = "SELECT kind,company_id,branch_id,department_id,state,version FROM commerce_orders WHERE id=$1 AND company_id=$2";

/// Registered executable statement for the owner route.
pub const fn statement() -> &'static str {
    SQL_EMPLOYEE_ORDER
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_EMPLOYEE_ORDER_READ_UNAVAILABLE",
        "The native Commerce employee-request order service is unavailable.",
    )
}

/// Postgres-backed owner reader. Read-only: no lock, ledger or write authority.
pub struct PgCommerceEmployeeOrderRead {
    pool: DatabasePool,
}

impl PgCommerceEmployeeOrderRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl CommerceEmployeeOrderRead for PgCommerceEmployeeOrderRead {
    fn read<'a>(
        &'a self,
        company: &'a str,
        order_id: &'a str,
    ) -> EmployeeOrderFuture<'a, Option<EmployeeOrder>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let order_id = order_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let order_id = order_id.clone();
                Box::pin(async move {
                    let row = sqlx::query(SQL_EMPLOYEE_ORDER)
                        .bind(order_id.as_str())
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.map(|row| {
                        Ok(EmployeeOrder {
                            kind: row.try_get("kind").map_err(|_| unavailable())?,
                            company_id: row.try_get("company_id").map_err(|_| unavailable())?,
                            branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
                            department_id: row.try_get("department_id").map_err(|_| unavailable())?,
                            state: row.try_get("state").map_err(|_| unavailable())?,
                            version: row.try_get("version").map_err(|_| unavailable())?,
                        })
                    })
                    .transpose()
                    .map_err(PgTransactionError::Business)
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
    fn executable_statement_keeps_exact_parity_and_owner_scope() {
        assert!(SQL_EMPLOYEE_ORDER_TS.contains('?'));
        assert!(!statement().contains('?'));
        assert_eq!(
            statement().replace("$1", "?").replace("$2", "?"),
            SQL_EMPLOYEE_ORDER_TS
        );
        for column in [
            "kind",
            "company_id",
            "branch_id",
            "department_id",
            "state",
            "version",
        ] {
            assert!(statement().contains(column), "missing {column}");
        }
        assert!(statement().starts_with("SELECT kind,company_id"));
        assert!(statement().contains("id=$1 AND company_id=$2"));
        assert!(!statement().contains("FOR UPDATE"));
        assert!(!statement().contains("LOCK TABLE"));
    }
}
