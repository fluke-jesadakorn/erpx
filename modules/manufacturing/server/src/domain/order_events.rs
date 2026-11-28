//! Pure domain for the native Manufacturing production order-events owner read.
//! This module owns the exact projection, its bound order scope and the response
//! shape. It emits no SQL, opens no connection and performs no IO.
//!
//! The projection is the byte-for-byte successor of the frozen
//! `RD_ORDER_EVENTS` read plan (`public/native_command.rs`). The only difference
//! is the Postgres bind marker (`$1` instead of `?`); the column list, table,
//! predicate and ordering are unchanged. The table is Manufacturing-owned
//! history: no cross-owner join and no write is performed by this read.

use erpx_platform_shared_server::public::{BusinessError, RecordId};
use serde::Serialize;

/// Exact owner SQL for the production detail order-events projection.
pub const ORDER_EVENTS_SQL: &str =
    "SELECT * FROM manufacturing_order_events WHERE order_id=$1 ORDER BY version";

pub(crate) fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_ORDER_EVENTS_INVALID",
        "Use a bounded Manufacturing order id.",
    )
}

/// The live caller already resolved this order from its WorkContext; the owner
/// validates the wire fact again so a malformed scope never reaches SQL.
pub fn validate_order(value: &str) -> Result<(), BusinessError> {
    if RecordId::new(value.to_owned()).is_err() {
        return Err(invalid());
    }
    Ok(())
}

/// Complete row returned by the retired TypeScript operation. Field names are
/// intentionally snake_case: the live caller returns raw SQL rows and maps
/// `results` directly into the production detail payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OrderEventRow {
    pub id: String,
    pub order_id: String,
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub status: String,
    pub details_json: String,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_scope_is_bounded() {
        assert!(validate_order("order-1").is_ok());
        assert!(validate_order("order_1").is_ok());
        assert!(validate_order("").is_err());
        assert!(validate_order("bad\0id").is_err());
        assert!(validate_order(&"x".repeat(201)).is_err());
    }

    #[test]
    fn row_serializes_to_the_exact_live_caller_field_names() {
        let row = OrderEventRow {
            id: "event-1".into(),
            order_id: "order-1".into(),
            version: 2,
            action: "issue".into(),
            actor_id: "user-1".into(),
            reason: "issued materials".into(),
            status: "in_progress".into(),
            details_json: "{}".into(),
            created_at: 1_700_000_000_000,
        };
        assert_eq!(
            serde_json::to_value(&row).unwrap(),
            serde_json::json!({
                "id": "event-1",
                "order_id": "order-1",
                "version": 2,
                "action": "issue",
                "actor_id": "user-1",
                "reason": "issued materials",
                "status": "in_progress",
                "details_json": "{}",
                "created_at": 1_700_000_000_000i64,
            })
        );
    }

    #[test]
    fn owner_sql_matches_the_frozen_read_plan_with_only_the_bind_marker_changed() {
        let registered = crate::public::native_command::PLAN_ORDER_EVENTS.sql;
        assert_eq!(ORDER_EVENTS_SQL, registered.replace('?', "$1"));
        assert_eq!(ORDER_EVENTS_SQL.matches("$1").count(), 1);
        assert!(ORDER_EVENTS_SQL.starts_with("SELECT "));
        assert!(!ORDER_EVENTS_SQL.contains(';'));
    }
}
