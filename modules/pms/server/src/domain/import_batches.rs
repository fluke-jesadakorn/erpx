//! Native PMS cutover-import batch list facts.
//!
//! This is a read-only projection of the `GET .../pms/imports?property=`
//! capability. It ports the exact row shape that the retained TypeScript
//! `pmsImportBatches` returns after its native lane-20 transport: nine
//! snake_case batch columns plus a parsed `report`, with `report_json` omitted.
//! No SQL, HTTP, command or receipt lives in this module.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

/// Maximum number of rows the retained bounded batch read may return.
pub const MAX_BATCHES: usize = 30;

/// Bounded Company/Property selection for one import-batch read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PmsImportBatchFacts {
    pub company_id: String,
    pub property_id: String,
}

/// One PMS-owned `pms_import_batches` row, exactly as the owner SQL projects it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PmsImportBatchRow {
    pub id: String,
    pub cutover_date: String,
    pub state: String,
    pub version: i32,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub opening_journal_id: Option<String>,
    pub report_json: String,
    pub created_at: i64,
}

/// The complete response row. Field order matches the retired TypeScript
/// object spread (`...batch`, then `report`, with `report_json` set to
/// `undefined` and therefore absent from JSON).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PmsImportBatchView {
    pub id: String,
    pub cutover_date: String,
    pub state: String,
    pub version: i32,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub opening_journal_id: Option<String>,
    pub created_at: i64,
    pub report: serde_json::Value,
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_IMPORT_BATCHES_REQUEST_INVALID",
        "Use one bounded import batch read.",
    )
}

pub fn read_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_IMPORT_BATCHES_READ_INVALID",
        "The import batch list could not be read.",
    )
}

/// The retained route validated `property` with `z.string().min(1).max(160)`.
/// The Company selector is composition-supplied and checked as a bounded token;
/// it is never trusted as authority here.
pub fn validate(facts: &PmsImportBatchFacts) -> Result<(), BusinessError> {
    let bounded = |value: &str, max: usize| {
        !value.is_empty()
            && value.len() <= max
            && value.chars().count() <= max
            && !value.chars().any(char::is_control)
    };
    if !bounded(&facts.company_id, 200) || !bounded(&facts.property_id, 160) {
        return Err(request_invalid());
    }
    Ok(())
}

/// Fail closed on an unbounded read before it reaches the response.
pub fn ensure_bounded(rows: &[PmsImportBatchRow]) -> Result<(), BusinessError> {
    if rows.len() <= MAX_BATCHES {
        Ok(())
    } else {
        Err(read_invalid())
    }
}

/// Retained `ORDER BY created_at DESC,id` (id ascending on a timestamp tie).
pub fn ensure_ordered(rows: &[PmsImportBatchRow]) -> Result<(), BusinessError> {
    let ordered = rows.windows(2).all(|pair| {
        pair[0].created_at > pair[1].created_at
            || (pair[0].created_at == pair[1].created_at && pair[0].id < pair[1].id)
    });
    if ordered {
        Ok(())
    } else {
        Err(read_invalid())
    }
}

/// Parse `report_json` into the `report` field and omit the raw column, exactly
/// as `pmsImportBatches` maps `batch.report_json` with `JSON.parse` and then
/// deletes `report_json` from the JSON response.
pub fn project_batches(
    rows: Vec<PmsImportBatchRow>,
) -> Result<Vec<PmsImportBatchView>, BusinessError> {
    rows.into_iter()
        .map(|row| {
            let report = serde_json::from_str(&row.report_json).map_err(|_| read_invalid())?;
            Ok(PmsImportBatchView {
                id: row.id,
                cutover_date: row.cutover_date,
                state: row.state,
                version: row.version,
                created_by: row.created_by,
                approved_by: row.approved_by,
                opening_journal_id: row.opening_journal_id,
                created_at: row.created_at,
                report,
            })
        })
        .collect()
}
