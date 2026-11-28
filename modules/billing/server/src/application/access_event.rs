//! Application use case for the native Billing evidence-access audit command.
//!
//! The owner boundary checks the retained Platform-write authority, runs the
//! pure domain planner, then delegates the single idempotent append to the
//! owner PostgreSQL port. The composition backend owns the surrounding
//! serializable Control unit and the current session.

use crate::adapters::access_event_postgres::append_access_event;
use crate::domain::access_event::AccessEvent;
use crate::domain::actor::BillingActor;
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, HoldingId, TimestampMillis};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

/// Authorize, plan and record one evidence-access decision. The caller owns
/// the transaction; this use case never opens, commits or rolls it back.
pub async fn record_access_event(
    connection: &mut PgConnection,
    actor: &BillingActor,
    target: &HoldingId,
    command: &AccessEvent,
    now: TimestampMillis,
) -> Result<(), Error> {
    actor.require_cost_command(target, now)?;
    let validated = command.validate()?;
    append_access_event(connection, target, actor, &validated, now).await
}
