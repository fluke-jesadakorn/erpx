//! Files-owned Staff Work task attachment metadata read.
//!
//! The native Staff Work detail needs the current attachment metadata for a
//! `staff_task` owner. Bytes, storage, scan state and binding authority stay in
//! Files; this read only exposes the same bounded projection the retained
//! Operations detail used (`id`, `original_name`, `bytes`, `media_type`) over
//! the caller's retained unit.
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sqlx::PgConnection;

const TASK_ATTACHMENT_LIMIT: usize = 500;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskAttachmentFact {
    pub id: String,
    pub original_name: String,
    pub bytes: i64,
    pub media_type: String,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_TASK_ATTACHMENTS_UNAVAILABLE",
        "The Staff Work attachments are unavailable.",
    )
}

/// Current non-revoked `staff_task` attachment metadata for one Company.
/// Ordered by creation time then id, bounded to the retained detail page size.
pub async fn task_attachments(
    connection: &mut PgConnection,
    company_id: &str,
    owner_id: &str,
) -> Result<Vec<TaskAttachmentFact>, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let owner = RecordId::new(owner_id)?;
    let rows = sqlx::query(
        "SELECT id,original_name,bytes,media_type FROM attachments \
         WHERE owner_type='staff_task' AND owner_id=$2 AND company_id=$1 \
         AND revoked_at IS NULL ORDER BY created_at,id LIMIT $3",
    )
    .bind(company.as_str())
    .bind(owner.as_str())
    .bind((TASK_ATTACHMENT_LIMIT + 1) as i64)
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| unavailable())?;
    if rows.len() > TASK_ATTACHMENT_LIMIT {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "FILES_TASK_ATTACHMENTS_TOO_LARGE",
            "This task has too many attachments to display.",
        ));
    }
    rows.into_iter()
        .map(|row| {
            use sqlx::Row;
            Ok(TaskAttachmentFact {
                id: row.try_get("id").map_err(|_| unavailable())?,
                original_name: row.try_get("original_name").map_err(|_| unavailable())?,
                bytes: row.try_get("bytes").map_err(|_| unavailable())?,
                media_type: row.try_get("media_type").map_err(|_| unavailable())?,
            })
        })
        .collect()
}
