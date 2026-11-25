//! sqlx execution of the owner-local lane-28 Manufacturing read families.

use super::application::{
    PostingReferencesRead, PostingReferencesReadFuture, StockHistoryRead, StockHistoryReadFuture,
};
use super::domain::{
    PostingReferencesFacts, PostingRow, StockHistoryFacts, POSTING_REFERENCES_SQL,
    STOCK_HISTORY_SQL, project,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

type Error = PgTransactionError<BusinessError>;

fn unavailable_business() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_LANE28_READ_UNAVAILABLE",
        "The native Manufacturing lane-28 read service is unavailable.",
    )
}

fn unavailable() -> Error {
    unavailable_business().into()
}

/// Postgres-backed owner reader for both lane-28 read families. Only
/// Manufacturing-owned relations supply response values; the cross-owner
/// tables in the posting witness appear only inside an EXISTS predicate.
pub struct PgLane28Read {
    pool: DatabasePool,
}

impl PgLane28Read {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl StockHistoryRead for PgLane28Read {
    fn history<'a>(
        &'a self,
        company: &'a str,
        warehouse: &'a str,
        product: &'a str,
    ) -> StockHistoryReadFuture<'a, StockHistoryFacts> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let warehouse = warehouse.to_owned();
        let product = product.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let warehouse = warehouse.clone();
                let product = product.clone();
                Box::pin(async move {
                    let history = sqlx::query_scalar::<_, bool>(STOCK_HISTORY_SQL)
                        .bind(company.as_str())
                        .bind(warehouse.as_str())
                        .bind(product.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(StockHistoryFacts { history })
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

impl PostingReferencesRead for PgLane28Read {
    fn references<'a>(
        &'a self,
        company: &'a str,
    ) -> PostingReferencesReadFuture<'a, PostingReferencesFacts> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    let rows = sqlx::query(POSTING_REFERENCES_SQL)
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let projected = rows
                        .into_iter()
                        .map(|row| {
                            Ok(PostingRow {
                                id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                order_id: row.try_get::<String, _>("order_id").map_err(|_| unavailable())?,
                                journal_id: row.try_get::<String, _>("journal_id").map_err(|_| unavailable())?,
                                amount_minor: row.try_get::<i64, _>("amount_minor").map_err(|_| unavailable())?,
                                image: row.try_get::<String, _>("image").map_err(|_| unavailable())?,
                                proven: row.try_get::<bool, _>("proven").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, Error>>()?;
                    Ok(project(projected))
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
