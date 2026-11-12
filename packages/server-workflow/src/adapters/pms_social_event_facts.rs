//! Workflow-owned outbox/command-receipt facts for the PMS social projector.
//!
//! Exact port of `packages/server-workflow/src/adapters/pms-social-event-facts.ts`.
//! It runs on the caller's `&mut sqlx::PgConnection` and never opens or commits a
//! transaction.
use crate::domain::pms_social_event::PmsSocialOriginal;
use erpx_server_runtime::public::postgres::PersistenceError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;

/// Exact retained join of the outbox event to its originating command receipt.
pub async fn pms_social_original(
    connection: &mut PgConnection,
    event_id: &str,
    event_type: &str,
) -> Result<Option<PmsSocialOriginal>, BusinessError> {
    let row = sqlx::query(
        "SELECT e.command_id,e.created_at,c.actor_id,c.result_json \
         FROM outbox_events e JOIN command_receipts c ON c.command_id=e.command_id \
         WHERE e.id=$1 AND e.event_type=$2",
    )
    .bind(event_id)
    .bind(event_type)
    .fetch_optional(&mut *connection)
    .await
    .map_err(read_failed)?;
    let Some(row) = row else {
        return Ok(None);
    };
    use sqlx::Row;
    Ok(Some(PmsSocialOriginal {
        command_id: row.try_get("command_id").map_err(read_failed)?,
        created_at: row.try_get("created_at").map_err(read_failed)?,
        actor_id: row.try_get("actor_id").map_err(read_failed)?,
        result_json: row.try_get("result_json").map_err(read_failed)?,
    }))
}

fn read_failed(error: sqlx::Error) -> BusinessError {
    let persistence = PersistenceError::from(error);
    if persistence.retryable() {
        BusinessError::new(
            FailureKind::Conflict,
            "PMS_SOCIAL_RETRY_REQUIRED",
            "The PMS social event facts must be retried.",
        )
    } else {
        BusinessError::new(
            FailureKind::Unavailable,
            "PMS_SOCIAL_READ_FAILED",
            "The PMS social event facts could not be read.",
        )
    }
}
