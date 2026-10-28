//! Existing Service physical reference, without new-link/worker/Finance grants.
pub use crate::adapters::field_service_reference::{
    CurrentFieldServiceAssetReference, CurrentFieldServicePhysicalAsset, FieldServiceAssetFacts,
    FieldServicePhysicalAssetFacts, lock_field_service_asset_sources,
    read_current_field_service_asset, read_current_field_service_physical_asset,
};
