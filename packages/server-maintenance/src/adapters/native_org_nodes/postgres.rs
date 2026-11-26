//! sqlx execution for the native Maintenance organization-node read.

use super::application::{OrgNodeRead, OrgNodeReadFuture};
use super::domain::{NodeScope, ORG_NODES_SQL, OrgNode};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MAINTENANCE_ORG_NODES_READ_UNAVAILABLE",
        "The native Maintenance organization-node read is unavailable.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MAINTENANCE_ORG_NODES_ROW_INVALID",
        "The owner returned an invalid organization-node row.",
    )
}

pub struct PgOrgNodeRead {
    pool: DatabasePool,
}

impl PgOrgNodeRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl OrgNodeRead for PgOrgNodeRead {
    fn nodes<'a>(&'a self, scope: &'a NodeScope) -> OrgNodeReadFuture<'a, Vec<OrgNode>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = scope.company_id.clone();
        let branch = scope.branch_id.clone();
        let department = scope.department_id.clone();
        Box::pin(async move {
            let company = company.clone();
            let branch = branch.clone();
            let department = department.clone();
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let branch = branch.clone();
                let department = department.clone();
                Box::pin(async move {
                    // Six positional binds match the pinned statement: Company,
                    // branch x3 and department x2. Absent branch/department bind
                    // SQL NULL so the TS `?::text IS NULL` scope branches hold.
                    let branch = branch.as_deref();
                    let department = department.as_deref();
                    let rows = sqlx::query(ORG_NODES_SQL)
                        .bind(company.as_str())
                        .bind(branch)
                        .bind(branch)
                        .bind(branch)
                        .bind(department)
                        .bind(department)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.iter()
                        .map(|row| {
                            Ok(OrgNode {
                                id: row.try_get("id").map_err(|_| invalid())?,
                                kind: row.try_get("kind").map_err(|_| invalid())?,
                                display_name: row.try_get("display_name").map_err(|_| invalid())?,
                                branch_id: row.try_get("branch_id").map_err(|_| invalid())?,
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
        assert_eq!(ORG_NODES_SQL.matches("$1").count(), 1);
        assert_eq!(ORG_NODES_SQL.matches("$6").count(), 1);
        assert!(ORG_NODES_SQL.contains("company_id=$1"));
        assert!(ORG_NODES_SQL.contains("($5::text IS NULL OR id=$6)"));
        assert!(!ORG_NODES_SQL.contains("LOCK"));
    }
}
