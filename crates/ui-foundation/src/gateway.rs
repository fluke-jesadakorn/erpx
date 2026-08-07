use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};

use crate::UiError;

/// Typed outbound port implemented by a native or browser transport adapter.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait CommandGateway<C>: Send + Sync {
    type Response: Send;

    async fn execute(
        &self,
        envelope: CommandEnvelope<C>,
    ) -> Result<CommandReceipt<Self::Response>, UiError>;
}

/// Typed outbound port implemented by a read-side transport adapter.
///
/// There is no write-store operation in this interface, so a UI query cannot
/// silently fall back to the command database.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait QueryGateway<Q>: Send + Sync {
    type Response: Send;

    async fn execute(
        &self,
        envelope: QueryEnvelope<Q>,
    ) -> Result<QueryResult<Self::Response>, UiError>;
}
