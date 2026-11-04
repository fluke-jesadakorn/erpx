//! Native physical Asset register, condition, placement, maintenance
//! lifecycle and retirement contract. Consumers use these named exports; no
//! SQL relation or adapter-private type escapes the crate.
pub use crate::adapters::asset_register::{AssetChangeReceipt, PgAssetRegister};
pub use crate::application::asset::{
    AcquisitionCostPort, Asset, AssetActor, AssetChangeOutcome, AssetChangeResult,
    AssetCommandLedger, AssetDetail, AssetEvent, AssetFuture, AssetRepository, AssetWorkspace,
    AssetWorkspaceData, FinanceAsset, FinanceCandidate, MaintenanceReference, NoCommandLedger,
    NodeOption, PersonOption, asset_detail, asset_workspace, change_asset, create_asset,
};
pub use crate::domain::asset::{
    AssetAction, AssetChange, AssetChangeInput, AssetState, AssetTransition, NewAsset, asset_after,
    require_expected_version, require_independent_retirement_review, validate_currency,
    validate_date, validate_reason,
};
