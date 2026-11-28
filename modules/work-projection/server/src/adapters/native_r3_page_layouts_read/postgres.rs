//! sqlx execution for the R3 page-layout read. The statement is the
//! byte-identical retained TypeScript projection with `?` replaced by sqlx's
//! `$n`; it is owned by this crate and never crosses the transport.

use crate::adapters::native_r3_page_layouts_read::application::{
    PageLayoutsReadPort, ReadFuture,
};
use crate::adapters::native_r3_page_layouts_read::domain::ReadQuery;
use crate::adapters::page_layouts_read::{read_page_layout_rows, PageLayoutRow};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORK_LAYOUT_UNAVAILABLE",
        "The native Work layout is unavailable.",
    )
}

fn transaction(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Pg-backed owner reader. Composition injects one Holding-scoped pool; the
/// shared unit gives the read the same serializable snapshot as other Work
/// owner adapters. No authority is granted here.
pub struct PgPageLayoutRead {
    pool: DatabasePool,
}

impl PgPageLayoutRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PageLayoutsReadPort for PgPageLayoutRead {
    fn read(&self, query: ReadQuery) -> ReadFuture<'_, Vec<PageLayoutRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = query.company;
        let page_key = query.page_key;
        let user = query.user;
        let base = query.base;
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let page_key = page_key.clone();
                let user = user.clone();
                let base = base.clone();
                Box::pin(async move {
                    read_page_layout_rows(
                        &mut *connection,
                        &company,
                        &page_key,
                        &user,
                        &base,
                    )
                    .await
                })
            })
            .await
            .map_err(transaction)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::page_layouts_read::PAGE_LAYOUTS_READ_SQL;

    #[test]
    fn owner_statement_is_the_retained_predicate_and_projection() {
        // The pinned literal is the TypeScript statement with `?` -> `$n`.
        assert_eq!(
            PAGE_LAYOUTS_READ_SQL,
            "SELECT owner_id,layout_json,version FROM page_layouts WHERE company_id=$1 AND page_key=$2 AND ((owner_id=$3 AND workspace_base=$4) OR (owner_id=$5 AND workspace_base=$6))"
        );
        assert_eq!(PAGE_LAYOUTS_READ_SQL.matches('$').count(), 6);
    }

    #[test]
    fn persistence_failures_are_unavailable_not_leaked_details() {
        let error = transaction(PgTransactionError::Persistence(
            erpx_platform_runtime_server::public::postgres::PersistenceError::new("pool_unavailable"),
        ));
        assert_eq!(error.code, "WORK_LAYOUT_UNAVAILABLE");
    }
}
