//! Native Assets HTTP ingress. The composition backend owns normal Account
//! session resolution, Company/branch/department scope and the serializable
//! owner unit; this package owns the registered route and transport bounds.
pub use crate::adapters::asset_http::{
    AssetCommandResponse, AssetDetailResponse, AssetHttpBackend, AssetHttpCommandRequest,
    AssetHttpFuture, AssetHttpReadReply, AssetHttpReadRequest, AssetHttpTarget,
    AssetRenderedContext, AssetWorkspaceResponse, asset_router,
};
pub use crate::adapters::asset_wire::{AssetHttpCommand, decode_asset_command, normalized_asset_command_input};
