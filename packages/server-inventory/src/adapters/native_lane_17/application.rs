//! Owner query port for the lane 17 landed-cost read family. No SQL lives here.
//!
//! Composition injects the owner reader behind this port exactly as it does for
//! the other owner-local native routers. The port transports bounded identities
//! and returns fixed row objects; the application never opens a transaction and
//! never composes a statement.

use crate::adapters::native_lane_17::domain::{LaneReadRequest, LaneReadResponse};
use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type LaneReadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait InventoryLane17Read: Send + Sync {
    /// Execute one admitted read and return the owner-shaped row array.
    fn read<'a>(&'a self, request: &'a LaneReadRequest) -> LaneReadFuture<'a, Vec<Value>>;
}

/// The wire response is always wrapped in one fixed envelope.
pub fn envelope(rows: Vec<Value>) -> LaneReadResponse {
    LaneReadResponse { rows }
}
