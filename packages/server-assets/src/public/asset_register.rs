//! Native physical Asset register, condition, placement, maintenance
//! lifecycle and retirement contract. Consumers use these named exports; no
//! SQL relation or adapter-private type escapes the crate.
pub use crate::adapters::asset_register::{
    AcquisitionCostProvider, AcquisitionCostEffect, RetainedAcquisitionCostProvider, AssetChangeReceipt, PgAssetRegister,
};
pub use crate::application::asset::{
    AcquisitionCostPort, Asset, AssetActor, AssetChangeOutcome, AssetChangeResult, AssetCurrencyCatalogue,
    AssetCommandLedger, AssetDetail, AssetEvent, AssetFuture, AssetRepository, AssetWorkspace,
    AssetWorkspaceData, FinanceAsset, FinanceCandidate, MaintenanceReference, NoCommandLedger,
    NodeOption, PersonOption, asset_detail, asset_workspace, change_asset, create_asset,
    create_asset_in_unit, validate_registration_currency, asset_create_purchase_requirements, asset_action_purchase_requirements,
};
pub use crate::domain::asset::{
    AssetAction, AssetChange, AssetChangeInput, AssetState, AssetTransition, NewAsset, asset_after,
    AssetPurchaseDisposition, AssetPurchasePurpose, AssetPurchaseRequirements,
    require_expected_version, require_independent_retirement_review, validate_currency,
    validate_date, validate_reason,
};
