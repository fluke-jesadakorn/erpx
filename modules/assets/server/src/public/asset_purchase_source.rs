//! Assets-owned source facts only; caller retains IAM, purchase and other owners.
pub use crate::adapters::asset_purchase_source::{
    AssetAttachmentDescriptor, CurrentAssetAttachmentDescriptor, observe_asset_attachment_descriptor,
    CurrentAssetPurchaseSource, lock_asset_purchase_sources, observe_asset_purchase_source,
    CurrentAssetMaintenanceJoin, inspect_asset_maintenance_join,
};
