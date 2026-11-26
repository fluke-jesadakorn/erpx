//! PostgreSQL execution of the native PMS document entrypoint reads.
//!
//! `DOCUMENT_SQL` is the explicit-column replacement for the TypeScript
//! `SELECT * FROM pms_documents WHERE id=?` planned by the read registry and
//! executed by TypeScript. `POSTINGS_SQL` is the same related-posting statement
//! with a Company existence witness added. Both execute here with sqlx inside
//! the caller's unit of work; no plan, no TypeScript preparation and no caller
//! projection survive.

use crate::application::native_documents::{
    DocumentFuture, DocumentQuery, DocumentRead, PostingQuery,
};
use crate::domain::native_documents::{DocumentRow, RelatedPosting};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

/// All seven owned columns, in the order the retired TypeScript row type
/// exposed them. The Company witness keeps the read inside the caller's scope.
pub const DOCUMENT_SQL: &str = "SELECT id,property_id,guest_account_id,kind,finance_id,snapshot_json,created_at FROM pms_documents WHERE id=$1 AND EXISTS(SELECT 1 FROM pms_properties p WHERE p.id=property_id AND p.company_id=$2)";

/// The retired `DISTINCT pp.journal_id` projection with the guest-account
/// subquery preserved byte-for-byte and a Company scope on the posting's
/// Property. `ORDER BY` makes the unbounded SQL set deterministic on the wire;
/// it adds no field and removes no posting.
pub const POSTINGS_SQL: &str = "SELECT DISTINCT pp.journal_id FROM pms_postings pp JOIN pms_records r ON r.id=pp.record_id AND r.property_id=pp.property_id WHERE pp.property_id=$1 AND r.kind='entry' AND r.data_json::jsonb->>'guestAccountId'=(SELECT guest_account_id FROM pms_documents WHERE id=$2) AND EXISTS(SELECT 1 FROM pms_properties p WHERE p.id=pp.property_id AND p.company_id=$3) ORDER BY pp.journal_id";

pub struct PgDocumentRead {
    pool: DatabasePool,
}

impl PgDocumentRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl DocumentRead for PgDocumentRead {
    fn load<'a>(
        &'a self,
        company: &'a str,
        query: &'a DocumentQuery,
    ) -> DocumentFuture<'a, Option<DocumentRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let id = query.id.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let id = id.clone();
                Box::pin(async move {
                    let row = sqlx::query(DOCUMENT_SQL)
                        .bind(id.as_str())
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| crate::domain::native_documents::unavailable())?;
                    let Some(row) = row else {
                        return Ok(None);
                    };
                    let unavailable = crate::domain::native_documents::unavailable();
                    Ok(Some(DocumentRow {
                        id: row.try_get::<String, _>("id").map_err(|_| unavailable.clone())?,
                        property_id: row
                            .try_get::<String, _>("property_id")
                            .map_err(|_| unavailable.clone())?,
                        guest_account_id: row
                            .try_get::<String, _>("guest_account_id")
                            .map_err(|_| unavailable.clone())?,
                        kind: row.try_get::<String, _>("kind").map_err(|_| unavailable.clone())?,
                        finance_id: row
                            .try_get::<Option<String>, _>("finance_id")
                            .map_err(|_| unavailable.clone())?,
                        snapshot_json: row
                            .try_get::<String, _>("snapshot_json")
                            .map_err(|_| unavailable.clone())?,
                        created_at: row
                            .try_get::<i64, _>("created_at")
                            .map_err(|_| unavailable.clone())?,
                    }))
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => {
                    crate::domain::native_documents::unavailable()
                }
            })
        })
    }

    fn postings<'a>(
        &'a self,
        company: &'a str,
        query: &'a PostingQuery,
    ) -> DocumentFuture<'a, Option<Vec<RelatedPosting>>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let owned = PostingQuery {
            property: query.property.clone(),
            document: query.document.clone(),
        };
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let owned = owned.clone();
                Box::pin(async move {
                    let rows = sqlx::query(POSTINGS_SQL)
                        .bind(owned.property.as_str())
                        .bind(owned.document.as_str())
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| crate::domain::native_documents::unavailable())?;
                    let unavailable = crate::domain::native_documents::unavailable();
                    rows.into_iter()
                        .map(|row| {
                            Ok(RelatedPosting {
                                journal_id: row
                                    .try_get::<String, _>("journal_id")
                                    .map_err(|_| unavailable.clone())?,
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map(Some)
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => {
                    crate::domain::native_documents::unavailable()
                }
            })
        })
    }
}
