//! sqlx execution of the migrated payment-batch reads. The statements are the
//! byte-identical TS literals with only `?` renumbered for sqlx.

use super::application::{BatchFuture, PaymentBatchRead};
use super::domain::{MAX_BATCHES, PaymentBatch, PaymentBatchDetail, unavailable};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sha2::Digest;
use sqlx::Row;

/// `readWorkflowPaymentBatches` in `adapters/workflow-flow-routing-facts.ts`.
pub const SQL_PAYMENT_BATCHES: &str = "SELECT DISTINCT ON (source_id) source_id,id,status,created_by FROM hr_payment_batches WHERE company_id=$1 AND source_type='payroll' AND status<>'cancelled' ORDER BY source_id,created_at DESC,id DESC";

/// `readDefinitionsPaymentBatch` in `adapters/workflow-flow-routing-facts.ts`.
pub const SQL_PAYMENT_BATCH_BY_SOURCE: &str = "SELECT id,status,created_by FROM hr_payment_batches WHERE company_id=$1 AND source_type='payroll' AND source_id=$2 AND status<>'cancelled' ORDER BY created_at DESC,id DESC LIMIT 1";

pub struct PgPaymentBatchRead {
    pool: DatabasePool,
}

impl PgPaymentBatchRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PaymentBatchRead for PgPaymentBatchRead {
    fn batches<'a>(&'a self, company: &'a str) -> BatchFuture<'a, Vec<PaymentBatch>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            let rows = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let company = company.clone();
                    Box::pin(async move {
                        let rows = sqlx::query(SQL_PAYMENT_BATCHES)
                            .bind(company.as_str())
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                        if rows.len() > MAX_BATCHES {
                            return Err(unavailable().into());
                        }
                        rows.into_iter()
                            .map(|row| {
                                Ok(PaymentBatch {
                                    source_id: row.try_get::<String, _>("source_id").map_err(|_| unavailable())?,
                                    id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                    status: row.try_get::<String, _>("status").map_err(|_| unavailable())?,
                                    created_by: row.try_get::<String, _>("created_by").map_err(|_| unavailable())?,
                                })
                            })
                            .collect::<Result<Vec<_>, BusinessError>>()
                            .map_err(Into::into)
                    })
                })
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    PgTransactionError::Persistence(_) => unavailable(),
                })?;
            Ok(rows)
        })
    }

    fn by_source<'a>(
        &'a self,
        company: &'a str,
        source: &'a str,
    ) -> BatchFuture<'a, Option<PaymentBatchDetail>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let source = source.to_owned();
        Box::pin(async move {
            let row = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let company = company.clone();
                    let source = source.clone();
                    Box::pin(async move {
                        let row = sqlx::query(SQL_PAYMENT_BATCH_BY_SOURCE)
                            .bind(company.as_str())
                            .bind(source.as_str())
                            .fetch_optional(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                        let detail: Option<PaymentBatchDetail> = row
                            .map(|row| -> Result<PaymentBatchDetail, BusinessError> {
                                Ok(PaymentBatchDetail {
                                    id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                    status: row.try_get::<String, _>("status").map_err(|_| unavailable())?,
                                    created_by: row.try_get::<String, _>("created_by").map_err(|_| unavailable())?,
                                })
                            })
                            .transpose()?;
                        Ok(detail)
                    })
                })
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    PgTransactionError::Persistence(_) => unavailable(),
                })?;
            Ok(row)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_keep_owner_scope_and_shape() {
        assert!(SQL_PAYMENT_BATCHES.contains("company_id=$1"));
        assert!(SQL_PAYMENT_BATCHES.contains("source_type='payroll'"));
        assert!(SQL_PAYMENT_BATCHES.contains("status<>'cancelled'"));
        assert!(SQL_PAYMENT_BATCHES.contains("DISTINCT ON (source_id)"));
        assert!(SQL_PAYMENT_BATCH_BY_SOURCE.contains("company_id=$1"));
        assert!(SQL_PAYMENT_BATCH_BY_SOURCE.contains("source_id=$2"));
        assert!(SQL_PAYMENT_BATCH_BY_SOURCE.contains("LIMIT 1"));
        assert!(!SQL_PAYMENT_BATCHES.contains('?'));
        assert!(!SQL_PAYMENT_BATCH_BY_SOURCE.contains('?'));
    }

    #[test]
    fn pins_rendered_sql_oracles() {
        for (sql, expected) in [
            (
                SQL_PAYMENT_BATCHES,
                "567f9caef077f1fa5ff662a809bdca5e31e9a992b45ebbe583c3590d9bccf6f5",
            ),
            (
                SQL_PAYMENT_BATCH_BY_SOURCE,
                "fb3b7bf6130a99a6c91cea4eb88390885b4a70b8b50ecadffffca51d42d63220",
            ),
        ] {
            let hex = sha2::Sha256::digest(sql.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            assert_eq!(hex, expected);
        }
    }
}
