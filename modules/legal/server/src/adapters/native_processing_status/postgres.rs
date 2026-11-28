//! sqlx execution for the native Legal processing-status read.
//!
//! The statement is a fixed constant and Company-scoped by bind. No request
//! value selects a table, a column or a predicate; the only bound values are the
//! opaque job id and the current Company scope. All variable-length columns are
//! capped inside the owner before they leave PostgreSQL, matching the retired
//! owner read's corruption guard.

use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

use super::application::{ProcessingStatusFuture, ProcessingStatusSource};
use super::domain::ProcessingStatusRead;

/// Bounded point read of one processing job. The Company bind is the owner's
/// scope, not a trusted request value.
pub const SQL_STATUS: &str = "SELECT id,item_id,document_id,state,result_json,error,attempts::bigint attempts FROM legal_processing_jobs WHERE id=$1 AND company_id=$2";

/// Maximum retained result/error text. Beyond this the record is treated as
/// corrupt rather than streamed into a response.
pub const RESULT_LIMIT: i32 = 262_144;

/// Pg-backed owner reader. Composition owns the pool and mounts the HTTP router.
pub struct PgProcessingStatusSource {
    pool: DatabasePool,
}

impl PgProcessingStatusSource {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_LEGAL_PROCESSING_STATUS_UNAVAILABLE",
        "The native Legal processing-status service is unavailable.",
    )
}

fn corrupt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEGAL_PROCESSING_CORRUPT_RECORD",
        "The processing status record is corrupt.",
    )
}

impl ProcessingStatusSource for PgProcessingStatusSource {
    fn status<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
    ) -> ProcessingStatusFuture<'a, Option<ProcessingStatusRead>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let id = id.to_owned();
        Box::pin(async move {
            let row = sqlx::query(SQL_STATUS)
                .bind(id.as_str())
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|r| {
                let raw = r
                    .try_get::<Option<String>, _>("result_json")
                    .map_err(|_| corrupt())?;
                let error = r
                    .try_get::<Option<String>, _>("error")
                    .map_err(|_| corrupt())?;
                if raw.as_ref().is_some_and(|v| v.len() > RESULT_LIMIT as usize)
                    || error.as_ref().is_some_and(|v| v.len() > RESULT_LIMIT as usize)
                {
                    return Err(corrupt());
                }
                let result = match raw {
                    Some(value) => {
                        serde_json::from_str::<serde_json::Value>(&value).map_err(|_| corrupt())?
                    }
                    None => serde_json::Value::Null,
                };
                Ok(ProcessingStatusRead {
                    id: r.try_get("id").map_err(|_| corrupt())?,
                    item_id: r.try_get("item_id").map_err(|_| corrupt())?,
                    document_id: r.try_get("document_id").map_err(|_| corrupt())?,
                    state: r.try_get("state").map_err(|_| corrupt())?,
                    error,
                    attempts: r.try_get("attempts").map_err(|_| corrupt())?,
                    job_id: id.clone(),
                    result,
                })
            })
            .transpose()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hash(sql: &str) -> String {
        format!("{:x}", Sha256::digest(sql.as_bytes()))
    }

    #[test]
    fn statement_is_fixed_and_company_scoped() {
        assert!(SQL_STATUS.contains("company_id=$2"), "missing Company scope");
        assert!(!SQL_STATUS.contains('?'), "placeholder must be $n");
        assert!(SQL_STATUS.contains("attempts::bigint attempts"));
        assert!(SQL_STATUS.contains("result_json"));
        assert_eq!(
            hash(SQL_STATUS),
            "2a343af53b0f1eca7950834399f7ec340ae4d5eb802e6f5fd9ea8071fec88a35"
        );
    }
}
