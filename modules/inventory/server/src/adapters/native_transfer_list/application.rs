//! Owner query port for the native stock-transfer list read. No SQL lives here.
//!
//! Composition injects the Postgres reader behind this port exactly as it does
//! for the other owner-local native routers. The port transports bounded
//! identities and returns fixed, complete row objects; the application never
//! opens a transaction and never composes a statement.

use crate::adapters::native_transfer_list::domain::{TransferListRequest, TransferListResponse};
use erpx_platform_runtime_server::public::http_support::serde_json::Value;
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type TransferListFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait InventoryTransferListRead: Send + Sync {
    /// Execute the one admitted read and return the complete TransferRow array.
    fn list<'a>(&'a self, request: &'a TransferListRequest) -> TransferListFuture<'a, Vec<Value>>;
}

/// The wire response is always wrapped in one fixed envelope.
pub fn envelope(rows: Vec<Value>) -> TransferListResponse {
    TransferListResponse { rows }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::serde_json::json;

    #[test]
    fn envelope_is_the_fixed_rows_only_shape() {
        let response = envelope(vec![json!({"id": "transfer-1"})]);
        let wire = erpx_platform_runtime_server::public::http_support::serde_json::to_value(&response)
            .expect("envelope serializes");
        assert_eq!(wire, json!({"rows": [{"id": "transfer-1"}]}));
        assert!(envelope(Vec::new()).rows.is_empty());
    }
}
