//! PostgreSQL port implementation for the native Billing evidence-access
//! audit command.
//!
//! This is the only owner-side SQL for access decisions. The caller owns the
//! serializable Control unit; this module performs one idempotent append in
//! that unit. It never mints a session, actor or Holding scope.

use crate::domain::access_event::{AccessEvent, AccessOutcome, ValidatedAccessEvent};
use crate::domain::actor::BillingActor;
use crate::domain::command::request_invalid;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CommandKey, HoldingId, RecordId, TimestampMillis};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

/// Append one validated evidence-access decision in the caller's transaction.
/// The insert is idempotent on the event id so an exact retry cannot duplicate
/// an audit row.
pub async fn append_access_event(
    connection: &mut PgConnection,
    target: &HoldingId,
    actor: &BillingActor,
    event: &ValidatedAccessEvent,
    now: TimestampMillis,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO billing_evidence_access_events\
         (id,evidence_id,holding_id,actor,outcome,reason,created_at)\
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO NOTHING",
    )
    .bind(&event.event_id)
    .bind(&event.evidence_id)
    .bind(target.as_str())
    .bind(actor.user_id.as_str())
    .bind(event.outcome)
    .bind(&event.reason)
    .bind(now.get())
    .execute(&mut *connection)
    .await?;
    Ok(())
}

pub fn access_event_command_key(key: &str) -> Result<CommandKey, BusinessError> {
    CommandKey::new(key).map_err(|_| request_invalid())
}

pub fn access_event_id(value: &str) -> Result<RecordId, BusinessError> {
    RecordId::new(value).map_err(|_| request_invalid())
}

pub fn access_event_outcome(value: &str) -> Result<AccessOutcome, BusinessError> {
    match value {
        "denied" => Ok(AccessOutcome::Denied),
        "allowed" => Ok(AccessOutcome::Allowed),
        _ => Err(request_invalid()),
    }
}

pub fn build_access_event(
    key: CommandKey,
    event_id: RecordId,
    evidence_id: RecordId,
    outcome: AccessOutcome,
    reason: String,
) -> AccessEvent {
    AccessEvent {
        key,
        event_id,
        evidence_id,
        outcome,
        reason,
    }
}
