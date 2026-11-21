//! Files-owned read used by Records employeeRequestDetail. The caller must
//! enforce request visibility before invoking it; this does not authorize files.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfqRequestAttachment {
    pub id: String,
    pub original_name: String,
    pub media_type: String,
    pub bytes: i64,
    pub checksum_sha256: String,
    pub purpose: String,
    pub revision: i64,
}

/// Mirrors EmployeeRequestsFilesOwnedFacts.readRequestAttachmentsEffect.
/// No page/size limit is imposed and DB errors pass through unchanged. The
/// caller controls transaction, original order of reads and any source fence.
pub async fn read_rfq_request_attachments(
    connection: &mut PgConnection,
    request_id: &str,
) -> Result<Vec<RfqRequestAttachment>, PgTransactionError<BusinessError>> {
    let rows: Vec<(String, String, String, i64, String, String, i64)> = sqlx::query_as("SELECT id,original_name,media_type,bytes,checksum_sha256,purpose,revision FROM attachments WHERE owner_type='employee_request' AND owner_id=$1 AND revoked_at IS NULL ORDER BY created_at,id")
        .bind(request_id)
        .fetch_all(connection)
        .await?;
    if rows.iter().any(|row| {
        row.3.unsigned_abs() > 9_007_199_254_740_991 || row.6.unsigned_abs() > 9_007_199_254_740_991
    }) {
        return Err(PersistenceError::new("DATABASE_NUMBER_OUT_OF_RANGE").into());
    }
    Ok(rows
        .into_iter()
        .map(
            |(id, original_name, media_type, bytes, checksum_sha256, purpose, revision)| {
                RfqRequestAttachment {
                    id,
                    original_name,
                    media_type,
                    bytes,
                    checksum_sha256,
                    purpose,
                    revision,
                }
            },
        )
        .collect())
}
