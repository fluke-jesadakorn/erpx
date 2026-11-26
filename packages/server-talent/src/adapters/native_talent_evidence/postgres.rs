//! sqlx/Postgres execution of the native Talent evidence read.
//!
//! This is the only layer in the slice that opens a unit of work and executes
//! SQL. The application handler never sees a connection or a row, and no
//! TypeScript prepares or executes these statements. The owner SELECTs are
//! fixed in the domain; only validated wire facts are bound.

use crate::application::native_talent_evidence::{
    EvidenceFuture, EvidenceQuery, TalentEvidenceRead,
};
use crate::domain::native_talent_evidence::{
    EVIDENCE_APPLICATION_SQL, EVIDENCE_REQUISITION_SQL, EvidenceMetric, unavailable,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

type Error = PgTransactionError<BusinessError>;

fn unavailable_error() -> Error {
    unavailable().into()
}

fn parse_row(text: Option<String>) -> Result<Value, Error> {
    let text = text.ok_or_else(unavailable_error)?;
    serde_json::from_str(&text).map_err(|_| unavailable_error())
}

/// Postgres-backed owner reader. The selected row is decoded from the JSONB
/// projection so every projected column keeps the exact snake_case key and
/// scalar shape the retired TypeScript literal returned.
pub struct PgTalentEvidenceRead {
    pool: DatabasePool,
}

impl PgTalentEvidenceRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl TalentEvidenceRead for PgTalentEvidenceRead {
    fn evidence<'a>(&'a self, query: &'a EvidenceQuery) -> EvidenceFuture<'a, Option<Value>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let metric = query.metric;
        let record_id = query.record_id.clone();
        let company_id = query.company_id.clone();
        let branch_id = query.branch_id.clone();
        let department_id = query.department_id.clone();
        let range = query.range;
        Box::pin(async move {
            let result = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let record_id = record_id.clone();
                    let company_id = company_id.clone();
                    let branch_id = branch_id.clone();
                    let department_id = department_id.clone();
                    Box::pin(async move {
                        let text = match metric {
                            EvidenceMetric::OpenRequisitions => {
                                sqlx::query_scalar::<_, Option<String>>(EVIDENCE_REQUISITION_SQL)
                                    .bind(record_id.as_str())
                                    .bind(company_id.as_str())
                                    .bind(branch_id.as_deref())
                                    .bind(department_id.as_deref())
                                    .fetch_one(&mut *connection)
                                    .await
                                    .map_err(|_| unavailable_error())?
                            }
                            EvidenceMetric::ApplicationStageCounts => {
                                let range = range.ok_or_else(unavailable_error)?;
                                sqlx::query_scalar::<_, Option<String>>(EVIDENCE_APPLICATION_SQL)
                                    .bind(record_id.as_str())
                                    .bind(company_id.as_str())
                                    .bind(branch_id.as_deref())
                                    .bind(department_id.as_deref())
                                    .bind(range.from)
                                    .bind(range.to_exclusive)
                                    .fetch_one(&mut *connection)
                                    .await
                                    .map_err(|_| unavailable_error())?
                            }
                        };
                        let row = parse_row(text)?;
                        Ok::<_, Error>(if row.is_null() { None } else { Some(row) })
                    })
                })
                .await;
            match result {
                Ok(row) => Ok(row),
                Err(PgTransactionError::Business(error)) => Err(error),
                Err(PgTransactionError::Persistence(_)) => Err(unavailable()),
            }
        })
    }
}
