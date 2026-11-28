//! PostgreSQL owner adapter for the native PMS import-batch list read.
//!
//! This is the only layer that executes SQL. The statements are the retained
//! byte-exact projection from `adapters/native-lane-20/postgres.rs` with a
//! Company/Property scope witness added; no caller supplies SQL or a table
//! name. A foreign Property yields the original empty list.
use crate::application::import_batches::{PmsImportBatchFuture, PmsImportBatchRepository};
use crate::domain::import_batches::{PmsImportBatchFacts, PmsImportBatchRow};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::BusinessError;
use sqlx::{PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

/// Company/Property existence witness: the retained lane-20 scope check.
pub const IMPORT_BATCH_SCOPE_SQL: &str =
    "SELECT 1 FROM pms_properties WHERE id=$1 AND company_id=$2";

/// Retained bounded batch-list projection, including `ORDER BY created_at DESC,id`.
pub const IMPORT_BATCHES_SQL: &str = "SELECT id,cutover_date,state,version,created_by,approved_by,opening_journal_id,report_json,created_at FROM pms_import_batches WHERE company_id=$1 AND property_id=$2 ORDER BY created_at DESC,id LIMIT 30";

fn batch_row(row: &PgRow, unavailable: &BusinessError) -> Result<PmsImportBatchRow, Error> {
    Ok(PmsImportBatchRow {
        id: row.try_get("id").map_err(|_| unavailable.clone())?,
        cutover_date: row.try_get("cutover_date").map_err(|_| unavailable.clone())?,
        state: row.try_get("state").map_err(|_| unavailable.clone())?,
        version: row.try_get("version").map_err(|_| unavailable.clone())?,
        created_by: row.try_get("created_by").map_err(|_| unavailable.clone())?,
        approved_by: row.try_get("approved_by").map_err(|_| unavailable.clone())?,
        opening_journal_id: row
            .try_get("opening_journal_id")
            .map_err(|_| unavailable.clone())?,
        report_json: row.try_get("report_json").map_err(|_| unavailable.clone())?,
        created_at: row.try_get("created_at").map_err(|_| unavailable.clone())?,
    })
}

async fn read(
    connection: &mut PgConnection,
    facts: &PmsImportBatchFacts,
) -> Result<Vec<PmsImportBatchRow>, Error> {
    let unavailable = crate::domain::import_batches::read_invalid();
    let scoped = sqlx::query_scalar::<_, i32>(IMPORT_BATCH_SCOPE_SQL)
        .bind(facts.property_id.as_str())
        .bind(facts.company_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| unavailable.clone())?;
    if scoped.is_none() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(IMPORT_BATCHES_SQL)
        .bind(facts.company_id.as_str())
        .bind(facts.property_id.as_str())
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| unavailable.clone())?;
    rows.iter().map(|row| batch_row(row, &unavailable)).collect()
}

/// Read-only PMS import-batch repository bound to the caller's connection.
pub struct PgPmsImportBatchRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgPmsImportBatchRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl PmsImportBatchRepository for PgPmsImportBatchRead<'_> {
    type Error = Error;

    fn batches<'a>(
        &'a mut self,
        facts: &'a PmsImportBatchFacts,
    ) -> PmsImportBatchFuture<'a, Vec<PmsImportBatchRow>, Self::Error> {
        Box::pin(async move { read(self.connection, facts).await })
    }
}
