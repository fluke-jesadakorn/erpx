//! Helper for updating pos_stripe_events applied_at field.
//!
//! When native complete_sale drains jobs, it must set applied_at on the
//! corresponding callbacks so addon package obligations query correctly clears
//! them. This mirrors the TS stripeEvent function's final `writePosStripeEvents3`
//! call at line 737 of pos-stripe.ts.

use erpx_finance_server::public::FinanceActor;
use erpx_platform_shared_server::public::{CompanyId, TimestampMillis};
use erpx_pos_server::public::{
    commands::{PgPosCommandRepository, PosActor},
    domain::stripe::PosConnectionRow,
};
use erpx_platform_runtime_server::public::postgres::PersistenceError;
use sqlx::PgConnection;

const CALLBACK_UPDATE_SQL: &str = r#"
    UPDATE pos_stripe_events SET applied_at = $1
    WHERE company_id = $2 AND
          account_id IN (
              SELECT DISTINCT p.stripe_account
              FROM pos_payments p
              WHERE p.sale_id = $3
          ) AND
          livemode != FALSE AND
          type IN (
              'charge.succeeded',
              'payment_intent.succeeded'
          ) AND
          (applied_at IS NULL OR applied_at = 0)
    RETURNING id;
"#;

/// Update applied_at on pos_stripe_events for payments in the given sale.
///
/// This mirrors the TS stripeEvent function's final callback at line 737:
/// ```typescript
/// await db.writePosStripeEvents3({ fact0: ports.clock.now(), id: event.id });
/// ```
///
/// Returns the count of updated callback events.
pub async fn apply_callbacks_on_sale_complete(
    connection: &mut PgConnection,
    financial_actor: &FinanceActor,
    holding_id: &HoldingsId,
    company_id: &CompanyId,
    sale_id: &str,
    now: TimestampMillis,
) -> Result<usize, PersistenceError> {
    // Transactions are required for foreign key actions (payments -> stripe_events)
    sqlx::query(CALLBACK_UPDATE_SQL)
        .bind(now)
        .bind(company_id.as_str())
        .bind(sale_id)
        .execute(connection)
        .await
        .map_err(|e| {
            PersistenceError::new(
                "POS_STRIPE_CALLBACKS_UPDATE_FAILED",
                format!("Failed to apply callbacks for sale {}: {}", sale_id, e),
            )
        })?;

    // Return the count of updated rows
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pos_stripe_events WHERE applied_at = $1",
    )
    .bind(now)
    .fetch_one(connection)
    .await
    .map_err(|e| PersistenceError::new("POS_STRIPE_CALLBACKS_COUNT_FAILED", e.to_string())) as usize)
}

const fn holdings_id(holding: &str) -> HoldingId {
    HoldingId::new(holding).expect("Invalid holding ID")
}

type HoldingId = crate::pos_effects::HoldingsId;