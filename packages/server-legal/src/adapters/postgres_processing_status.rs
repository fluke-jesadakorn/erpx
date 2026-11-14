//! Bounded canonical processing read. This descriptor is source evidence only;
//! composition must authorize the current viewer against its item before use.
use crate::domain::processing::{LegalProcessingStatus, PROCESSING_RESULT_LIMIT};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;

pub struct PgLegalProcessingStatusSource {
    status: LegalProcessingStatus,
    raw_result: Option<String>,
    payload_oversized: bool,
    company: CompanyId,
    transaction: (String, i32, String, String),
    fingerprint: [u8; 32],
}
impl PgLegalProcessingStatusSource {
    pub fn item_id(&self) -> &RecordId {
        &self.status.item_id
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if transaction(c).await? != self.transaction {
            return Err(changed().into());
        }
        let current = load_legal_processing_status(c, &self.company, &self.status.id).await?;
        if current.fingerprint != self.fingerprint {
            return Err(changed().into());
        }
        Ok(())
    }
    /// Decode only after the current viewer's item access has succeeded.
    pub fn into_status(mut self) -> Result<LegalProcessingStatus, Error> {
        if self.payload_oversized {
            return Err(corrupt().into());
        }
        self.status.result = self
            .raw_result
            .map(|value| serde_json::from_str(&value).map_err(|_| corrupt()))
            .transpose()?;
        Ok(self.status)
    }
}

pub async fn lock_legal_processing_status_sources(c: &mut PgConnection) -> Result<(), Error> {
    sqlx::query("LOCK TABLE legal_processing_jobs IN ROW SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

pub async fn load_legal_processing_status(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &RecordId,
) -> Result<PgLegalProcessingStatusSource, Error> {
    let transaction = transaction(c).await?;
    // Every variable-length value is capped inside PostgreSQL before transfer.
    // No actor_json, checksum, provider material or other users' data is read.
    let row = sqlx::query("SELECT CASE WHEN octet_length(item_id)<=800 THEN item_id END item_id,CASE WHEN octet_length(document_id)<=800 THEN document_id END document_id,CASE WHEN octet_length(state)<=128 THEN state END state,attempts::bigint attempts,octet_length(error)::bigint error_bytes,CASE WHEN octet_length(error)<=65536 THEN error END error,octet_length(result_json)::bigint result_bytes,CASE WHEN octet_length(result_json)<=$3 THEN result_json END result_json FROM legal_processing_jobs WHERE id=$1 AND company_id=$2 FOR SHARE")
        .bind(id.as_str()).bind(company.as_str()).bind(PROCESSING_RESULT_LIMIT as i64).fetch_optional(&mut *c).await?
        .ok_or_else(|| BusinessError::new(FailureKind::NotFound,"LEGAL_NOT_FOUND","Processing job or document unavailable."))?;
    let item: String = row
        .try_get::<Option<String>, _>("item_id")?
        .ok_or_else(corrupt)?;
    let document: String = row
        .try_get::<Option<String>, _>("document_id")?
        .ok_or_else(corrupt)?;
    let state: String = row
        .try_get::<Option<String>, _>("state")?
        .ok_or_else(corrupt)?;
    let attempts: i64 = row.try_get("attempts")?;
    let error_bytes: Option<i64> = row.try_get("error_bytes")?;
    let result_bytes: Option<i64> = row.try_get("result_bytes")?;
    let payload_oversized = error_bytes.is_some_and(|n| n > 65536)
        || result_bytes.is_some_and(|n| n > PROCESSING_RESULT_LIMIT as i64);
    let error: Option<String> = row.try_get("error")?;
    let raw: Option<String> = row.try_get("result_json")?;
    let fingerprint: [u8; 32] = Sha256::digest(
        serde_json::to_vec(&(
            &item,
            &document,
            &state,
            attempts,
            &error,
            &raw,
            error_bytes,
            result_bytes,
        ))
        .map_err(|_| corrupt())?,
    )
    .into();
    let status = LegalProcessingStatus {
        id: id.clone(),
        item_id: RecordId::new(item)?,
        document_id: RecordId::new(document)?,
        state,
        attempts: u32::try_from(attempts).map_err(|_| corrupt())?,
        result: None,
        error,
    };
    Ok(PgLegalProcessingStatusSource {
        status,
        raw_result: raw,
        payload_oversized,
        company: company.clone(),
        transaction,
        fingerprint,
    })
}
async fn transaction(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn corrupt() -> PersistenceError {
    PersistenceError::new("LEGAL_PROCESSING_CORRUPT_RECORD")
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "LEGAL_PROCESSING_STATUS_CHANGED",
        "The processing status source changed.",
    )
}
